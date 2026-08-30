-- Modrinth Studios addition: tracks which instance originally created each
-- shared folder, so that instance can be treated as a "master copy" holder —
-- see packages/app-lib/src/api/shared_profile.rs (leave_item_restore) for
-- how this is used. Deliberately no foreign key constraint here (unlike
-- studio_instance_shared_profiles' instance_id column) — this is a
-- best-effort pointer rather than a hard relationship, and the app already
-- has to handle the owner instance no longer existing (it may have been
-- deleted) by falling back to ordinary empty-out-on-leave behavior for
-- everyone, checked in application code rather than enforced by the schema.
ALTER TABLE studio_shared_profiles ADD COLUMN owner_instance_id TEXT;
