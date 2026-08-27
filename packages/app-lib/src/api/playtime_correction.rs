//! Modrinth Studios addition: manual per-instance playtime correction, so
//! someone switching from another launcher (Prism, MultiMC, etc.) can credit
//! hours Modrinth itself never tracked. Kept in its own table (see the
//! `studio_playtime_corrections` migration) instead of a new column on
//! `instances`, and uses the plain runtime `sqlx::query()` API rather than
//! the compile-time-checked `sqlx::query!` macro upstream's instance queries
//! use — `packages/app-lib/.cargo/config.toml` forces `SQLX_OFFLINE`, so any
//! change to a `query!` call needs a regenerated `.sqlx` cache via
//! `cargo sqlx prepare`. A brand-new module using only the runtime API sides
//! steps that entirely and can't touch upstream's own queries.
//!
//! The correction is stored and shown completely separately from
//! `Instance::submitted_time_played` / `recent_time_played` (the actual
//! Modrinth-tracked total) — it's only added to that total for display.

use crate::Result;
use crate::state::State;
use sqlx::Row;

/// Current correction, in seconds. `0` if none has ever been set for this
/// instance.
pub async fn get_correction_seconds(instance_id: &str) -> Result<i64> {
    let state = State::get().await?;

    let row = sqlx::query(
        "SELECT correction_seconds FROM studio_playtime_corrections WHERE instance_id = ?",
    )
    .bind(instance_id)
    .fetch_optional(&state.pool)
    .await?;

    Ok(match row {
        Some(row) => row.try_get::<i64, _>("correction_seconds")?,
        None => 0,
    })
}

/// Sets the correction to an exact value (in seconds) — this overwrites
/// whatever was stored before, it does not add to it. The settings UI always
/// shows the current value and lets it be edited in place, rather than
/// accumulating a little more on every save.
pub async fn set_correction_seconds(
    instance_id: &str,
    seconds: i64,
) -> Result<()> {
    let state = State::get().await?;
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        "
		INSERT INTO studio_playtime_corrections (instance_id, correction_seconds, modified)
		VALUES (?, ?, ?)
		ON CONFLICT(instance_id) DO UPDATE SET
			correction_seconds = excluded.correction_seconds,
			modified = excluded.modified
		",
    )
    .bind(instance_id)
    .bind(seconds)
    .bind(now)
    .execute(&state.pool)
    .await?;

    Ok(())
}
