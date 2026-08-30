-- Modrinth Studios addition: shared Minecraft folders. An instance can opt
-- into a "shared profile" so its worlds, config, resource packs, and
-- options.txt are the *same files* as every other instance using that
-- profile (editing one changes them all) — mods, the loader, and the
-- Minecraft version stay per-instance, since those are usually the whole
-- reason for having separate instances in the first place.
--
-- Kept in two new tables rather than a column on `instances`, for the same
-- reason as studio_playtime_corrections: packages/app-lib/.cargo/config.toml
-- forces SQLX_OFFLINE, so touching a `sqlx::query!`-checked query against
-- `instances` needs a regenerated `.sqlx` cache via `cargo sqlx prepare`,
-- which isn't available in every environment this fork gets built in. A
-- separate table + plain runtime `sqlx::query()` calls (see
-- packages/app-lib/src/api/shared_profile.rs) sidestep that entirely.
CREATE TABLE studio_shared_profiles (
	id TEXT NOT NULL,
	name TEXT NOT NULL,
	created INTEGER NOT NULL,

	PRIMARY KEY (id)
);

-- A row here means that instance's saves/config/resourcepacks/options.txt
-- are currently reparse points (directory junctions on Windows, symlinks on
-- macOS/Linux; options.txt is a hard link everywhere) pointing into the
-- shared profile's own folder, rather than real local files/folders. No row
-- means the instance has its own private copies, same as before this
-- feature existed — see shared_profile_dir()/set_instance_shared_profile()
-- in packages/app-lib/src/api/shared_profile.rs.
CREATE TABLE studio_instance_shared_profiles (
	instance_id TEXT NOT NULL,
	shared_profile_id TEXT NOT NULL,
	modified INTEGER NOT NULL,

	PRIMARY KEY (instance_id),
	FOREIGN KEY (instance_id) REFERENCES instances(id) ON DELETE CASCADE,
	FOREIGN KEY (shared_profile_id) REFERENCES studio_shared_profiles(id) ON DELETE CASCADE
);

CREATE INDEX studio_instance_shared_profiles_profile_id
	ON studio_instance_shared_profiles(shared_profile_id);
