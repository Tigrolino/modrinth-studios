-- Modrinth Studios addition: cache for the expensive, per-replay metadata
-- `list_replays` extracts from each recording's embedded zip (duration,
-- recorded-at, server/world name, etc.). Without this, opening the Replays
-- tab meant opening and inflating every single replay archive on disk,
-- every single time the tab was visited — fine for a handful of replays,
-- painfully slow for someone with hundreds/thousands (as reported).
--
-- Keyed by (instance_id, kind, file_name); `file_size`/`modified_at` are a
-- cheap staleness check against the file currently on disk — if either
-- differs from what's cached, `list_replays` re-opens that one archive and
-- overwrites this row, same as `reconcile_source_screenshots` does for the
-- Screenshots tab's own on-disk index (just without that one's SHA1-based
-- rename detection, which replays don't need: a renamed replay just gets
-- treated as a new file and its zip re-read once, no worse than before this
-- cache existed).
CREATE TABLE studio_replay_metadata_cache (
	instance_id TEXT NOT NULL,
	kind TEXT NOT NULL,
	file_name TEXT NOT NULL,
	file_size INTEGER NOT NULL,
	modified_at INTEGER NOT NULL,
	duration_ms INTEGER,
	recorded_at INTEGER,
	minecraft_version TEXT,
	server_name TEXT,
	singleplayer INTEGER,
	world_name TEXT,
	display_name TEXT,
	cached_at INTEGER NOT NULL,

	PRIMARY KEY (instance_id, kind, file_name),
	FOREIGN KEY (instance_id) REFERENCES instances(id) ON DELETE CASCADE
);
