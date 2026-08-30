//! Modrinth Studios addition: Studio's own schema additions (new tables,
//! columns, and one-time data seeds) used to live as ordinary files in
//! `packages/app-lib/migrations/`, tracked by `sqlx::migrate!()` in the same
//! `_sqlx_migrations` table as every upstream Modrinth migration.
//!
//! That table lives in the *same* SQLite database file the official,
//! unmodified Modrinth app uses, because Studio never changed the shared
//! Tauri `identifier` (`ModrinthApp`, see `apps/app/tauri.conf.json`) — both
//! apps resolve to the same AppData folder and therefore the same
//! `app.db`. Studio's own compiled binary happily recorded its extra
//! migrations in `_sqlx_migrations`, but the official app's own compiled-in
//! migrator (which only knows about upstream's own files) then refused to
//! open the database at all, with "migration ... was previously applied but
//! is missing in the resolved migrations" — because as far as ITS migrator
//! is concerned, those rows are unrecognized entries in its tracking table.
//!
//! Fix: Studio's own additions now live in
//! `packages/app-lib/studio-migrations/` instead — outside the folder
//! `sqlx::migrate!()` embeds by default (`./migrations`) — so they are never
//! written to `_sqlx_migrations` at all. Instead they're tracked in a
//! separate `studio_migrations` table that only Studio's own code ever
//! queries. The official app doesn't validate that a database contains
//! *only* tables it recognizes, so an extra table is completely invisible to
//! it. Each Studio migration still only ever runs once, the same guarantee
//! `sqlx::migrate!()` gave.
//!
//! For anyone who already had Studio installed before this change (and
//! therefore already has these 5 versions recorded as successful in
//! `_sqlx_migrations`, with their tables/columns/data already applied), the
//! legacy record needs backfilling into `studio_migrations` WITHOUT
//! re-running the SQL (re-running a plain `CREATE TABLE` would fail
//! outright, and re-running the sidebar-default `UPDATE` would silently
//! override a user's own preference on every single launch).
//!
//! That backfill has to happen in two stages around `sqlx::migrate!()`
//! itself, not just once after it — `sqlx::migrate!().run(&pool)` validates,
//! *before applying anything new*, that every row already in
//! `_sqlx_migrations` corresponds to one of the migrations it has resolved.
//! Since Studio's own `sqlx::migrate!()` no longer resolves Studio's 5
//! migrations either (they moved out of the tracked folder), Studio's own
//! next launch after upgrading hit the *exact* "previously applied but is
//! missing in the resolved migrations" error this whole fix exists to solve
//! — just against itself instead of the official app — because the 5 legacy
//! rows were still sitting in `_sqlx_migrations` when `sqlx::migrate!()` ran
//! and checked. `reconcile_legacy_rows()` below strips those rows out
//! *before* `sqlx::migrate!()` gets a chance to validate the table, and
//! `apply_pending()` (called after, once upstream's own tables definitely
//! exist) applies anything that's still genuinely new.
use sqlx::{Executor, Pool, Sqlite};

struct StudioMigration {
    version: i64,
    description: &'static str,
    sql: &'static str,
}

// Keep this list in the same chronological order the files were originally
// applied in under `sqlx::migrate!()`, so a fresh install (with nothing to
// backfill) applies them in the same relative order as before.
const STUDIO_MIGRATIONS: &[StudioMigration] = &[
    StudioMigration {
        version: 20260825190000,
        description: "studio-sidebar-closed-default",
        sql: include_str!(
            "../../studio-migrations/20260825190000_studio-sidebar-closed-default.sql"
        ),
    },
    StudioMigration {
        version: 20260826120000,
        description: "studio-playtime-correction",
        sql: include_str!(
            "../../studio-migrations/20260826120000_studio-playtime-correction.sql"
        ),
    },
    StudioMigration {
        version: 20260829120000,
        description: "studio-shared-profiles",
        sql: include_str!("../../studio-migrations/20260829120000_studio-shared-profiles.sql"),
    },
    StudioMigration {
        version: 20260829130000,
        description: "studio-shared-profile-item-flags",
        sql: include_str!(
            "../../studio-migrations/20260829130000_studio-shared-profile-item-flags.sql"
        ),
    },
    StudioMigration {
        version: 20260829140000,
        description: "studio-shared-profile-owner",
        sql: include_str!(
            "../../studio-migrations/20260829140000_studio-shared-profile-owner.sql"
        ),
    },
    StudioMigration {
        version: 20260830120000,
        description: "studio-discord-rpc-settings",
        sql: include_str!(
            "../../studio-migrations/20260830120000_studio-discord-rpc-settings.sql"
        ),
    },
];

async fn ensure_table(pool: &Pool<Sqlite>) -> crate::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS studio_migrations (
			version BIGINT NOT NULL,
			description TEXT NOT NULL,
			applied_at INTEGER NOT NULL,

			PRIMARY KEY (version)
		)",
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn is_recorded(pool: &Pool<Sqlite>, version: i64) -> crate::Result<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM studio_migrations WHERE version = ?")
        .bind(version)
        .fetch_one(pool)
        .await?;

    Ok(count > 0)
}

/// Strips any of Studio's own migration versions out of the legacy
/// `sqlx::migrate!()`-tracked `_sqlx_migrations` table, backfilling their
/// "already applied" status into `studio_migrations` first so nothing is
/// lost. Must run **before** `sqlx::migrate!().run(&pool)` — see the module
/// doc comment for why.
pub(crate) async fn reconcile_legacy_rows(pool: &Pool<Sqlite>) -> crate::Result<()> {
    ensure_table(pool).await?;

    let legacy_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;
    if legacy_table_count == 0 {
        return Ok(());
    }

    for migration in STUDIO_MIGRATIONS {
        if is_recorded(pool, migration.version).await? {
            continue;
        }

        let mut tx = pool.begin().await?;

        let legacy_applied_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = ? AND success = 1",
        )
        .bind(migration.version)
        .fetch_one(&mut *tx)
        .await?;

        if legacy_applied_count == 0 {
            tx.rollback().await?;
            continue;
        }

        tracing::info!(
            "Backfilling already-applied Modrinth Studios migration {} ({}) into studio_migrations, and removing its now-redundant entry from _sqlx_migrations",
            migration.version,
            migration.description
        );

        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = ?")
            .bind(migration.version)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            "INSERT INTO studio_migrations (version, description, applied_at) VALUES (?, ?, unixepoch())",
        )
        .bind(migration.version)
        .bind(migration.description)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
    }

    Ok(())
}

/// Applies every Studio migration that isn't yet recorded in
/// `studio_migrations` (a fresh install, or a new Studio migration added
/// after this fix that has no legacy row to backfill from). Must be called
/// **after** `sqlx::migrate!().run(&pool)` has already brought the database
/// up to date with upstream's own migrations, since some of Studio's
/// migrations reference tables (e.g. `instances`) that upstream's own
/// migrations are what create.
pub(crate) async fn apply_pending(pool: &Pool<Sqlite>) -> crate::Result<()> {
    ensure_table(pool).await?;

    for migration in STUDIO_MIGRATIONS {
        if is_recorded(pool, migration.version).await? {
            continue;
        }

        tracing::info!(
            "Applying Modrinth Studios migration {} ({})",
            migration.version,
            migration.description
        );

        let mut tx = pool.begin().await?;

        // `tx.execute(sqlx::raw_sql(...))` rather than
        // `sqlx::raw_sql(...).execute(&mut *tx)` — the latter fails to
        // compile through the tauri::command macro's generated futures with
        // "implementation of `Executor` is not general enough"; the two are
        // otherwise equivalent (raw_sql's own `execute` just calls back into
        // the executor's), see https://github.com/launchbadge/sqlx/issues/3581.
        tx.execute(sqlx::raw_sql(migration.sql)).await?;

        sqlx::query(
            "INSERT INTO studio_migrations (version, description, applied_at) VALUES (?, ?, unixepoch())",
        )
        .bind(migration.version)
        .bind(migration.description)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
    }

    Ok(())
}
