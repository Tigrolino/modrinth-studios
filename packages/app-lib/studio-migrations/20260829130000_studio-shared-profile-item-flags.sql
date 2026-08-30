-- Modrinth Studios addition: per-item control over what a shared Minecraft
-- folder actually shares, plus the server list as a new shareable item.
-- Defaults to 1 (shared) on every existing row so profiles created before
-- this migration keep behaving exactly as they did (all four original items
-- shared) — servers.dat is new, so existing profiles start sharing it too;
-- turn it off per-profile in the "Manage shared folders" UI if that's not
-- wanted for an existing group.
ALTER TABLE studio_shared_profiles ADD COLUMN share_saves INTEGER NOT NULL DEFAULT 1;
ALTER TABLE studio_shared_profiles ADD COLUMN share_config INTEGER NOT NULL DEFAULT 1;
ALTER TABLE studio_shared_profiles ADD COLUMN share_resourcepacks INTEGER NOT NULL DEFAULT 1;
ALTER TABLE studio_shared_profiles ADD COLUMN share_options INTEGER NOT NULL DEFAULT 1;
ALTER TABLE studio_shared_profiles ADD COLUMN share_servers INTEGER NOT NULL DEFAULT 1;
