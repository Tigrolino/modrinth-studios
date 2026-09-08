use crate::state::DirectoryInfo;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions,
};
use sqlx::{Pool, Sqlite};
use std::path::Path;
use std::time::Duration;

pub(crate) async fn connect(
    app_identifier: &str,
) -> crate::Result<Pool<Sqlite>> {
    let settings_dir = DirectoryInfo::initial_settings_dir_path(app_identifier)
        .ok_or(crate::ErrorKind::FSError(
            "Could not find valid config dir".to_string(),
        ))?;

    crate::util::io::create_dir_all(&settings_dir).await?;

    let db_path = settings_dir.join("app.db");

    connect_app_db(&db_path).await
}

async fn connect_app_db(db_path: &Path) -> crate::Result<Pool<Sqlite>> {
    super::db_backup::maybe_backup_existing_app_db(db_path).await?;
    open_migrated_app_db(db_path).await
}

async fn open_migrated_app_db(db_path: &Path) -> crate::Result<Pool<Sqlite>> {
    let pool = open_app_db_pool(db_path).await?;

    if let Err(err) = stale_data_cleanup(&pool).await {
        tracing::warn!(
            "Failed to clean up stale data from state database before migrations: {err}"
        );
    }

    // Modrinth Studios fix: strip any of Studio's own migration versions out
    // of the legacy sqlx::migrate!()-tracked table *before* migrate!() runs
    // and validates it — see studio_migrations.rs for the full why. Skipping
    // this step (or running it after migrate!()) makes Studio's own
    // migrator hit the exact "previously applied but is missing in the
    // resolved migrations" error this fix exists to solve, just against
    // itself instead of the official app.
    super::studio_migrations::reconcile_legacy_rows(&pool).await?;

    // Modrinth Studios fix: the shared AppData database may also have
    // migrations applied by a *different build* of this same crate than the
    // one currently running — e.g. the person's separate official Modrinth
    // App install has since shipped upstream migrations Studio's own fork
    // hasn't caught up to yet, and vice versa for the Studio-only
    // migrations `reconcile_legacy_rows` just cleared out above. By default
    // sqlx::migrate!() hard-errors the moment `_sqlx_migrations` contains
    // any row it doesn't resolve locally ("was previously applied but is
    // missing in the resolved migrations") — `set_ignore_missing` tells it
    // to tolerate that instead of refusing to start, which is exactly the
    // documented use case for two applications sharing one database.
    let mut migrator = sqlx::migrate!();
    migrator.set_ignore_missing(true);
    migrator.run(&pool).await?;

    // Apply any of Studio's own schema additions that are still genuinely
    // new (a fresh install, or a Studio migration added after this fix).
    // Must run after the migrate!() call above, since some of Studio's
    // migrations reference tables upstream's migrations create.
    super::studio_migrations::apply_pending(&pool).await?;

    record_current_app_version(&pool).await?;

    if let Err(err) = stale_data_cleanup(&pool).await {
        tracing::warn!(
            "Failed to clean up stale data from state database: {err}"
        );
    }

    Ok(pool)
}

async fn open_app_db_pool(db_path: &Path) -> crate::Result<Pool<Sqlite>> {
    let conn_options = SqliteConnectOptions::new()
        .filename(db_path)
        .busy_timeout(Duration::from_secs(30))
        .journal_mode(SqliteJournalMode::Wal)
        .optimize_on_close(true, None)
        .create_if_missing(true);

    Ok(SqlitePoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .idle_timeout(Duration::from_secs(120))
        .max_lifetime(None)
        .connect_with(conn_options)
        .await?)
}

async fn record_current_app_version(pool: &Pool<Sqlite>) -> crate::Result<()> {
    sqlx::query!(
        "
		INSERT INTO app_metadata (key, value, updated_at)
		VALUES ('app_version', ?, unixepoch())
		ON CONFLICT(key) DO UPDATE SET
			value = excluded.value,
			updated_at = excluded.updated_at
		",
        env!("CARGO_PKG_VERSION"),
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Cleans up data from the database that is no longer referenced, but must be
/// kept around for a little while to allow users to recover from accidental
/// deletions.
async fn stale_data_cleanup(pool: &Pool<Sqlite>) -> crate::Result<()> {
    let mut tx = pool.begin().await?;

    let has_skin_tables = sqlx::query!(
		"SELECT COUNT(*) AS \"count!: i64\" FROM sqlite_master WHERE type = 'table' AND name IN ('custom_minecraft_skins', 'minecraft_users')",
	)
	.fetch_one(&mut *tx)
	.await?
	.count == 2;

    if has_skin_tables {
        sqlx::query!(
			"DELETE FROM custom_minecraft_skins WHERE minecraft_user_uuid NOT IN (SELECT uuid FROM minecraft_users)"
		)
		.execute(&mut *tx)
		.await?;
    }

    tx.commit().await?;

    Ok(())
}
