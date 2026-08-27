-- Modrinth Studios addition: manual per-instance playtime correction.
--
-- Kept in its own table rather than a new column on `instances` so this stays
-- fully isolated from upstream's `instances` table and the compile-time-
-- checked `sqlx::query!` calls against it (packages/app-lib/.cargo/config.toml
-- forces SQLX_OFFLINE, so any change to those queries needs a regenerated
-- `.sqlx` cache via `cargo sqlx prepare` — a new table + plain runtime
-- `sqlx::query()` calls in a brand-new module avoid that entirely).
--
-- Lets someone switching from another launcher (Prism, MultiMC, etc.) credit
-- hours Modrinth never tracked, without altering the real tracked total
-- (`instances.submitted_time_played` / `recent_time_played`) — the two stay
-- separate and are only added together for display.
CREATE TABLE studio_playtime_corrections (
	instance_id TEXT NOT NULL,
	correction_seconds INTEGER NOT NULL DEFAULT 0,
	modified INTEGER NOT NULL,

	PRIMARY KEY (instance_id),
	FOREIGN KEY (instance_id) REFERENCES instances(id) ON DELETE CASCADE
);
