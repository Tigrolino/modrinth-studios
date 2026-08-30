-- Modrinth Studios: default the built-in "hide right sidebar" behavior to ON
-- so the sidebar starts collapsed and the show/hide affordance is active,
-- without changing the underlying column's stored default (keeps this
-- migration additive-only and easy to drop if upstream ever changes this).
UPDATE settings SET toggle_sidebar = TRUE;
