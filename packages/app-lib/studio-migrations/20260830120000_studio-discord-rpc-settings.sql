-- Modrinth Studios addition: customizable Discord Rich Presence. Kept in its
-- own singleton table (id is always 1) rather than new columns on the
-- existing `settings` table, for the same reason studio_playtime_corrections
-- already avoids touching upstream-queried tables: packages/app-lib/.cargo/
-- config.toml forces SQLX_OFFLINE, so any change to a `sqlx::query!`-checked
-- query against `settings` would need a regenerated `.sqlx` cache via
-- `cargo sqlx prepare`, which isn't guaranteed to be available in every
-- environment this fork gets built in.
CREATE TABLE studio_discord_rpc_settings (
	id INTEGER NOT NULL DEFAULT 1,

	-- 'default' | 'detailed' | 'minimal' | 'custom' — see
	-- packages/app-lib/src/api/discord_rpc.rs for what each one does.
	mode TEXT NOT NULL DEFAULT 'default',
	-- 'playing' | 'listening' | 'watching' | 'competing' — the verb Discord
	-- shows before the activity (e.g. "Playing X" vs "Watching X").
	activity_type TEXT NOT NULL DEFAULT 'playing',
	show_elapsed_time INTEGER NOT NULL DEFAULT 1,

	-- Used only when mode = 'custom'. May contain {instance}/{loader}/
	-- {version} placeholders.
	custom_state_template TEXT NOT NULL DEFAULT '',
	custom_details_template TEXT NOT NULL DEFAULT '',

	idle_text TEXT NOT NULL DEFAULT 'Idling...',

	-- Up to two custom Rich Presence buttons. Both label and URL must be set
	-- for a given button to actually show.
	button_1_label TEXT,
	button_1_url TEXT,
	button_2_label TEXT,
	button_2_url TEXT,

	PRIMARY KEY (id),
	CHECK (id = 1)
);
