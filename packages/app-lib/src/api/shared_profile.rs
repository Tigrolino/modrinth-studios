//! Modrinth Studios addition: shared Minecraft folders. An instance can opt
//! into a "shared profile" so its worlds, config, resource packs, server
//! list, and/or options.txt are the *same files* as every other instance
//! using that profile — editing one changes them all, since they're the
//! literal same files on disk (a directory junction/symlink for the
//! directory items, a hard link for the two single-file items; see
//! `crate::util::link`). Which of those actually get shared is controlled
//! per-profile (`SharedProfile::share_*`); mods, the loader, and the
//! Minecraft version are never shared — those are usually the whole reason
//! for having separate instances in the first place.
//!
//! Not to be confused with this app's *other*, unrelated "shared instance"
//! feature (`crate::api::instance::shared`, multiplayer-managed instances) —
//! same word, completely different thing. This module's naming avoids that
//! one entirely (`SharedProfile`, not `SharedInstance`).
//!
//! Kept in its own tables rather than a column on `instances`, for the same
//! reason as `studio_playtime_corrections` (see that module): this fork's
//! `.cargo/config.toml` forces `SQLX_OFFLINE`, so any change to a
//! `sqlx::query!`-checked query against `instances` needs a regenerated
//! `.sqlx` cache via `cargo sqlx prepare`, which isn't guaranteed to be
//! available in every environment this fork gets built in. Separate tables +
//! the plain runtime `sqlx::query()` API sidestep that entirely.
//!
//! # Design: every item is independent, and nothing is ever half-applied
//!
//! An earlier version of this module tried to link/unlink all of an
//! instance's items in one all-or-nothing pass, using `?` to bail out the
//! moment any single item failed. In practice, that caused real data-safety
//! problems: resource packs are by far the most failure-prone item (deeply
//! nested asset paths are exactly the kind of thing that trips Windows'
//! classic 260-character `MAX_PATH` limit once relocated under
//! `shared_profiles/<uuid>/`, and they're also concurrently touched by this
//! app's own content-sync/watcher machinery) — and a resource-pack failure
//! partway through used to leave `saves`/`config` already linked on disk
//! while the database still recorded "not shared" for the whole instance,
//! a real, observed case of on-disk and database state disagreeing.
//!
//! `reconcile_links()` now processes every item independently: each one is
//! compared against what it *should* be (from the profile's current
//! `share_*` flags) and linked or unlinked on its own, with its own error
//! handled and recorded rather than aborting the rest. The instance→profile
//! membership row is written to reflect the requested membership regardless
//! of any individual item's failure — reconciliation only ever asks "does
//! this one item currently match what it should be?" (via
//! `crate::util::link::is_link` for the directory items, or the sidecar
//! `LinkState` marker for the two hard-linked file items — see that struct's
//! doc comment for why they need different detection), never "what did the
//! last attempt leave behind?", so it's always safe to call again —
//! including from the exact same UI action, or automatically the next time
//! this profile's settings are edited.
//!
//! **Real data risk**: joining or leaving a shared profile moves or links
//! real save data. Joining, when the instance already has its own real local
//! data, backs that data up (renamed, not deleted) rather than silently
//! discarding it. Leaving is intentionally destructive on the *instance's*
//! side, by request: turning an item off (or off entirely) empties it out
//! locally — an empty `saves`/`config`/`resourcepacks` folder, no
//! `options.txt`/`servers.dat` — rather than restoring a private copy of
//! whatever was being shared. The shared copy itself is untouched by this
//! and keeps living under `shared_profiles/<id>/` for as long as any other
//! instance is still using it (or the profile itself still exists); it only
//! actually goes away once the shared profile is deleted via
//! `delete_shared_profile`, which — for the same reason — is now a genuine,
//! unrecoverable deletion of that data, not a safe detach.
//!
//! The one exception is the shared profile's **owner** — the instance that
//! originally created it (`SharedProfile::owner_instance_id`). By request,
//! that one instance is treated as holding a "master copy": detaching an
//! item from it, however that happens, restores a real private copy instead
//! of emptying it out, so the instance that started sharing something never
//! loses it. Everyone else's data risk is as described above. See
//! `join_item()`/`leave_item_empty()`/`leave_item_restore()` below for
//! exactly what happens in each case.
//!
//! **options.txt/servers.dat caveat**: these are hard-linked rather than
//! symlinked (a Windows file symlink needs admin/Developer Mode; a hard link
//! doesn't). That only keeps working as long as the game *edits the file in
//! place*. If a Minecraft version ever writes one via the common "write a
//! temp file, then rename it over the original" pattern instead, the rename
//! severs the hard link — the instance's copy and the shared copy silently
//! become two independent files from that point on, with no error raised
//! anywhere. Worth actually testing (change a video setting or add a server
//! in one instance, confirm it shows up in a sibling instance).

use crate::state::State;
use crate::state::instances::Instance;
use crate::state::instances::adapters::sqlite::instance_rows;
use crate::state::instances::watcher;
use crate::util::link;
use async_walkdir::WalkDir;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const MAX_NAME_LENGTH: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedProfile {
    pub id: String,
    pub name: String,
    pub share_saves: bool,
    pub share_config: bool,
    pub share_resourcepacks: bool,
    pub share_options: bool,
    pub share_servers: bool,
    /// The instance that originally created this shared folder, if known
    /// (and if that instance still exists — see the migration that added
    /// this column). That instance is treated as the shared folder's "master
    /// copy" holder: unlike every other member, detaching an item from it
    /// (by leaving, by an item being turned off, or by the shared folder
    /// being deleted) restores a real private copy instead of emptying the
    /// item out, so it never loses data it was the one to start sharing. See
    /// `leave_item_restore()`.
    pub owner_instance_id: Option<String>,
}

enum LinkKind {
    Dir,
    File,
}

struct LinkedItem {
    /// Path relative to the instance's own folder (and to the shared
    /// profile's folder — the layout mirrors 1:1). Doubles as this item's
    /// stable identifier in error messages.
    relative_path: &'static str,
    kind: LinkKind,
    flag: fn(&SharedProfile) -> bool,
}

/// Every item a shared profile *can* share, and how to read whether a given
/// profile currently wants to share it. Mods/shaderpacks/datapacks and the
/// loader/version deliberately aren't here — see the module doc comment.
const LINKED_ITEMS: &[LinkedItem] = &[
    LinkedItem {
        relative_path: "saves",
        kind: LinkKind::Dir,
        flag: |p| p.share_saves,
    },
    LinkedItem {
        relative_path: "config",
        kind: LinkKind::Dir,
        flag: |p| p.share_config,
    },
    LinkedItem {
        relative_path: "resourcepacks",
        kind: LinkKind::Dir,
        flag: |p| p.share_resourcepacks,
    },
    LinkedItem {
        relative_path: "options.txt",
        kind: LinkKind::File,
        flag: |p| p.share_options,
    },
    LinkedItem {
        relative_path: "servers.dat",
        kind: LinkKind::File,
        flag: |p| p.share_servers,
    },
];

fn validate_name(name: &str) -> crate::Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(crate::ErrorKind::InputError(
            "Shared folder name cannot be empty".to_string(),
        )
        .into());
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(crate::ErrorKind::InputError(format!(
            "Shared folder name cannot exceed {MAX_NAME_LENGTH} characters"
        ))
        .into());
    }
    Ok(name)
}

fn shared_profile_from_row(row: &SqliteRow) -> crate::Result<SharedProfile> {
    Ok(SharedProfile {
        id: row.try_get::<String, _>("id")?,
        name: row.try_get::<String, _>("name")?,
        share_saves: row.try_get::<bool, _>("share_saves")?,
        share_config: row.try_get::<bool, _>("share_config")?,
        share_resourcepacks: row.try_get::<bool, _>("share_resourcepacks")?,
        share_options: row.try_get::<bool, _>("share_options")?,
        share_servers: row.try_get::<bool, _>("share_servers")?,
        owner_instance_id: row.try_get::<Option<String>, _>("owner_instance_id")?,
    })
}

const SELECT_COLUMNS: &str = "id, name, share_saves, share_config, share_resourcepacks, share_options, share_servers, owner_instance_id";

pub async fn list_shared_profiles() -> crate::Result<Vec<SharedProfile>> {
    let state = State::get().await?;
    let rows = sqlx::query(&format!(
        "SELECT {SELECT_COLUMNS} FROM studio_shared_profiles ORDER BY name COLLATE NOCASE"
    ))
    .fetch_all(&state.pool)
    .await?;

    rows.iter().map(shared_profile_from_row).collect()
}

async fn get_shared_profile_row(
    id: &str,
    pool: &SqlitePool,
) -> crate::Result<Option<SharedProfile>> {
    let row = sqlx::query(&format!(
        "SELECT {SELECT_COLUMNS} FROM studio_shared_profiles WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some(row) => Some(shared_profile_from_row(&row)?),
        None => None,
    })
}

/// Creates a new, empty shared folder. `owner_instance_id`, if given, is
/// recorded as the folder's "owner" — the instance the frontend created it
/// from, which is always the same instance that immediately joins it right
/// after (see `SharedProfile::owner_instance_id`'s doc comment for what that
/// buys it). Passing `None` is fine — the shared folder just behaves as it
/// did before this feature existed, with no owner-backup safety net for
/// anyone.
pub async fn create_shared_profile(
    name: String,
    owner_instance_id: Option<String>,
) -> crate::Result<SharedProfile> {
    let name = validate_name(&name)?.to_string();
    let state = State::get().await?;
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        "INSERT INTO studio_shared_profiles
            (id, name, created, share_saves, share_config, share_resourcepacks, share_options, share_servers, owner_instance_id)
         VALUES (?, ?, ?, 1, 1, 1, 1, 1, ?)",
    )
    .bind(&id)
    .bind(&name)
    .bind(now)
    .bind(&owner_instance_id)
    .execute(&state.pool)
    .await?;

    Ok(SharedProfile {
        id,
        name,
        share_saves: true,
        share_config: true,
        share_resourcepacks: true,
        share_options: true,
        share_servers: true,
        owner_instance_id,
    })
}

pub async fn rename_shared_profile(
    id: String,
    new_name: String,
) -> crate::Result<SharedProfile> {
    let new_name = validate_name(&new_name)?.to_string();
    let state = State::get().await?;

    let result =
        sqlx::query("UPDATE studio_shared_profiles SET name = ? WHERE id = ?")
            .bind(&new_name)
            .bind(&id)
            .execute(&state.pool)
            .await?;

    if result.rows_affected() == 0 {
        return Err(crate::ErrorKind::InputError(format!(
            "Unknown shared folder {id}"
        ))
        .into());
    }

    get_shared_profile_row(&id, &state.pool).await?.ok_or_else(|| {
        crate::ErrorKind::InputError(format!("Unknown shared folder {id}"))
            .as_error()
    })
}

/// Updates which items `id` shares, then re-syncs every instance currently
/// using it to match — an instance that was sharing resource packs and just
/// had that turned off gets its resource packs unlinked and emptied out
/// right away (`leave_item_empty` — or a real copy restored via
/// `leave_item_restore`, if that instance happens to be this profile's
/// owner), and one that just had worlds turned on gets linked in, without
/// needing to leave and rejoin the profile. Instances that are currently
/// running are skipped (same as any other change to a running instance's
/// shared folder) — they'll pick up the new selection next time they're
/// stopped and something else touches this profile.
pub async fn update_shared_profile_items(
    id: String,
    share_saves: bool,
    share_config: bool,
    share_resourcepacks: bool,
    share_options: bool,
    share_servers: bool,
) -> crate::Result<SharedProfile> {
    let state = State::get().await?;

    let result = sqlx::query(
        "
		UPDATE studio_shared_profiles
		SET share_saves = ?, share_config = ?, share_resourcepacks = ?, share_options = ?, share_servers = ?
		WHERE id = ?
		",
    )
    .bind(share_saves)
    .bind(share_config)
    .bind(share_resourcepacks)
    .bind(share_options)
    .bind(share_servers)
    .bind(&id)
    .execute(&state.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(crate::ErrorKind::InputError(format!(
            "Unknown shared folder {id}"
        ))
        .into());
    }

    let profile = get_shared_profile_row(&id, &state.pool).await?.ok_or_else(|| {
        crate::ErrorKind::InputError(format!("Unknown shared folder {id}"))
            .as_error()
    })?;

    let member_instance_ids: Vec<String> = sqlx::query_scalar(
        "SELECT instance_id FROM studio_instance_shared_profiles WHERE shared_profile_id = ?",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    for instance_id in member_instance_ids {
        let Some(instance) =
            instance_rows::get_instance_by_id(&instance_id, &state.pool).await?
        else {
            continue;
        };

        let running = state
            .process_manager
            .get_all()
            .into_iter()
            .any(|process| process.instance_id == instance_id);
        if running {
            continue;
        }

        let is_owner = profile.owner_instance_id.as_deref() == Some(instance_id.as_str());
        let outcome = apply_profile_items(
            &instance_id,
            &instance,
            Some(&profile),
            is_owner,
            &state,
        )
        .await;
        for (item, error) in &outcome.failures {
            tracing::warn!(
                "Shared folder '{}': failed to update '{item}' for instance {instance_id}: {error}",
                profile.name
            );
        }
    }

    Ok(profile)
}

/// Deletes a shared profile — every instance currently using it is detached
/// first (see `leave_item_empty`/`leave_item_restore`), exactly as if each
/// had individually turned sharing off, so the instance-side unlink/watcher
/// dance always happens cleanly before the shared folder disappears out from
/// under it. **This is a genuine, unrecoverable deletion**: detaching no
/// longer restores a private copy of the shared data for anyone except the
/// profile's owner (see the module doc comment), so once every member has
/// been detached and the shared folder itself is removed below, that data
/// is gone for everyone else — not moved anywhere. Refuses outright (before
/// touching anything) if this profile has an owner and that owner is
/// currently running — see the running-check below for why.
pub async fn delete_shared_profile(id: String) -> crate::Result<()> {
    let state = State::get().await?;

    // If this shared folder has an owner, its backup restore (see
    // `leave_item_restore`) has to actually run as part of detaching it
    // below — and that can't happen safely while the owner instance is
    // running (same reason every other shared-folder change refuses a
    // running instance). Rather than silently skip the owner's backup in
    // that case, refuse the whole deletion so the "master copy" guarantee
    // never quietly fails to apply.
    if let Some(profile) = get_shared_profile_row(&id, &state.pool).await?
        && let Some(owner_instance_id) = &profile.owner_instance_id
    {
        let owner_running = state
            .process_manager
            .get_all()
            .into_iter()
            .any(|process| &process.instance_id == owner_instance_id);
        if owner_running {
            let owner_name = instance_rows::get_instance_by_id(owner_instance_id, &state.pool)
                .await
                .ok()
                .flatten()
                .map(|instance| instance.name)
                .unwrap_or_else(|| owner_instance_id.clone());
            return Err(crate::ErrorKind::InputError(format!(
                "Can't delete this shared folder while '{owner_name}', the instance that created it, is running. Stop it first so its backup copy can be restored safely."
            ))
            .into());
        }
    }

    let member_instance_ids: Vec<String> = sqlx::query_scalar(
        "SELECT instance_id FROM studio_instance_shared_profiles WHERE shared_profile_id = ?",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    for instance_id in member_instance_ids {
        // Best-effort: a running instance can't be safely detached, but a
        // deletion shouldn't be blocked entirely by one busy instance either
        // — it's left recorded as a member of a profile that's about to
        // stop existing, which reconcile_links treats the same as "not
        // shared" the next time anything touches it (get_shared_profile_row
        // will simply return None for a deleted id).
        if let Err(error) = set_instance_shared_profile(&instance_id, None).await {
            tracing::warn!(
                "Shared folder deletion: couldn't detach instance {instance_id}: {error}"
            );
        }
    }

    let result = sqlx::query("DELETE FROM studio_shared_profiles WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(crate::ErrorKind::InputError(format!(
            "Unknown shared folder {id}"
        ))
        .into());
    }

    // Best-effort: every instance that could be detached has already been
    // safely copied back onto its own path above, so this is just tidying
    // up what's now an orphaned folder — not worth failing the whole
    // deletion over.
    let _ = crate::util::io::remove_dir_all(
        state.directories.shared_profile_dir(&id),
    )
    .await;

    Ok(())
}

pub async fn get_instance_shared_profile(
    instance_id: &str,
) -> crate::Result<Option<SharedProfile>> {
    let state = State::get().await?;
    let row = sqlx::query(&format!(
        "
		SELECT {SELECT_COLUMNS}
		FROM studio_instance_shared_profiles AS memberships
		INNER JOIN studio_shared_profiles AS shared_profiles
			ON shared_profiles.id = memberships.shared_profile_id
		WHERE memberships.instance_id = ?
		"
    ))
    .bind(instance_id)
    .fetch_optional(&state.pool)
    .await?;

    Ok(match row {
        Some(row) => Some(shared_profile_from_row(&row)?),
        None => None,
    })
}

async fn current_shared_profile_id(
    instance_id: &str,
    pool: &SqlitePool,
) -> crate::Result<Option<String>> {
    let id: Option<String> = sqlx::query_scalar(
        "SELECT shared_profile_id FROM studio_instance_shared_profiles WHERE instance_id = ?",
    )
    .bind(instance_id)
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

/// Joins `instance_id` to `shared_profile_id`, or leaves whatever shared
/// profile it's currently in if `shared_profile_id` is `None`. Refuses
/// (touching neither the database nor any files) if the instance is
/// currently running, or if `shared_profile_id` doesn't name a real shared
/// profile. A no-op if the instance is already associated with exactly the
/// requested profile (use `update_shared_profile_items` to change *which
/// items* an existing membership shares).
///
/// The membership itself is recorded regardless of whether every individual
/// item could be linked — see the module doc comment for why. If any item
/// failed, this returns an error describing which one(s), but the
/// membership change and whichever items *did* succeed are kept, not rolled
/// back; calling this again (or editing the profile's items) will simply
/// retry whatever didn't take.
pub async fn set_instance_shared_profile(
    instance_id: &str,
    shared_profile_id: Option<&str>,
) -> crate::Result<()> {
    let state = State::get().await?;

    let Some(instance) =
        instance_rows::get_instance_by_id(instance_id, &state.pool).await?
    else {
        return Err(crate::ErrorKind::InputError(format!(
            "Unknown instance {instance_id}"
        ))
        .into());
    };

    let running = state
        .process_manager
        .get_all()
        .into_iter()
        .any(|process| process.instance_id == instance_id);
    if running {
        return Err(crate::ErrorKind::InputError(
            "Can't change this instance's shared folder while it's running, stop it first."
                .to_string(),
        )
        .into());
    }

    let current_id = current_shared_profile_id(instance_id, &state.pool).await?;
    if current_id.as_deref() == shared_profile_id {
        return Ok(());
    }

    let target_profile = match shared_profile_id {
        Some(id) => Some(get_shared_profile_row(id, &state.pool).await?.ok_or_else(
            || {
                crate::ErrorKind::InputError(format!(
                    "Unknown shared folder {id}"
                ))
                .as_error()
            },
        )?),
        None => None,
    };

    // Whichever profile this instance is currently in (if any) is the one
    // being left below — fetch it (rather than just its id) so we can tell
    // whether this instance is its "owner" (see `SharedProfile::
    // owner_instance_id`) and, if so, restore a real private copy instead of
    // emptying it out.
    let leaving_profile = match &current_id {
        Some(id) => get_shared_profile_row(id, &state.pool).await?,
        None => None,
    };
    let is_owner_of_current = leaving_profile
        .as_ref()
        .is_some_and(|p| p.owner_instance_id.as_deref() == Some(instance_id));

    let mut failures = Vec::new();

    if current_id.is_some() && shared_profile_id.is_some() {
        // Switching from one profile to another: fully leave the old one
        // first (unlinks everything currently linked, regardless of which
        // profile it belonged to — reconcile_links only looks at what's
        // *currently* linked), then join the new one fresh, rather than
        // trying to re-target an existing link in place.
        let leave_outcome = apply_profile_items(
            instance_id,
            &instance,
            None,
            is_owner_of_current,
            &state,
        )
        .await;
        failures.extend(leave_outcome.failures);
    }

    // This call doubles as "join the new profile" (when `shared_profile_id`
    // is `Some`, where restoring an owner backup is meaningless — nothing is
    // being left here) and as "leave to no profile at all" (when it's
    // `None`, the plain-toggle-off case) — only the second one should ever
    // restore an owner backup.
    let restore_owner_on_leave = shared_profile_id.is_none() && is_owner_of_current;
    let join_outcome = apply_profile_items(
        instance_id,
        &instance,
        target_profile.as_ref(),
        restore_owner_on_leave,
        &state,
    )
    .await;
    failures.extend(join_outcome.failures);

    match shared_profile_id {
        Some(target_id) => {
            let now = chrono::Utc::now().timestamp();
            sqlx::query(
                "
				INSERT INTO studio_instance_shared_profiles (instance_id, shared_profile_id, modified)
				VALUES (?, ?, ?)
				ON CONFLICT(instance_id) DO UPDATE SET
					shared_profile_id = excluded.shared_profile_id,
					modified = excluded.modified
				",
            )
            .bind(instance_id)
            .bind(target_id)
            .bind(now)
            .execute(&state.pool)
            .await?;
        }
        None => {
            sqlx::query(
                "DELETE FROM studio_instance_shared_profiles WHERE instance_id = ?",
            )
            .bind(instance_id)
            .execute(&state.pool)
            .await?;
        }
    }

    if !failures.is_empty() {
        let summary = failures
            .iter()
            .map(|(item, error)| format!("{item} ({error})"))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(crate::ErrorKind::OtherError(format!(
            "Some items couldn't be updated — the rest were applied. {summary}"
        ))
        .into());
    }

    Ok(())
}

struct ReconcileOutcome {
    /// (item's relative path, error message) for anything that failed.
    failures: Vec<(&'static str, String)>,
}

async fn apply_profile_items(
    instance_id: &str,
    instance: &Instance,
    profile: Option<&SharedProfile>,
    restore_owner: bool,
    state: &State,
) -> ReconcileOutcome {
    let instance_full_path =
        state.directories.instances_dir().join(&instance.path);

    // Drop the file watcher's handles on this instance's saves/config
    // subfolders before touching them — same reason as the instance-folder
    // rename feature: on Windows, the watcher holds an open directory handle
    // on each of those for as long as the app runs, which by itself is
    // enough to block replacing them with a junction (or restoring a real
    // folder in their place).
    watcher::unwatch_instance_folder(
        instance_id,
        &instance.path,
        &state.file_watcher,
        &state.directories,
    )
    .await;

    let outcome =
        reconcile_links(&instance_full_path, profile, restore_owner, state).await;

    watcher::watch_instance_folder(
        instance_id,
        &instance.path,
        &state.file_watcher,
        &state.directories,
    )
    .await;

    outcome
}

/// Sidecar marker recording which File-kind items (`options.txt`,
/// `servers.dat`) are currently hard-linked into a shared profile. Needed
/// because, unlike a directory junction/symlink, a hard link is
/// indistinguishable from an ordinary file from the filesystem alone — same
/// file type, no separate "target" to read, and `crate::util::link::is_link`
/// (which checks for a reparse point) correctly reports `false` for it every
/// time. Dir-kind items don't need this; a junction/symlink is unambiguous.
const LINK_STATE_FILE_NAME: &str = ".studio-shared-links.json";

#[derive(Default, Serialize, Deserialize)]
struct LinkState {
    linked_files: HashSet<String>,
}

async fn read_link_state(instance_full_path: &Path) -> LinkState {
    let Ok(bytes) =
        crate::util::io::read(instance_full_path.join(LINK_STATE_FILE_NAME)).await
    else {
        return LinkState::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Best-effort — a failure here only means the next reconcile might
/// re-attempt a join/leave that already actually succeeded on disk (harmless
/// for File-kind items either way), never a loss of the underlying data.
async fn write_link_state(instance_full_path: &Path, state: &LinkState) {
    let path = instance_full_path.join(LINK_STATE_FILE_NAME);
    if state.linked_files.is_empty() {
        let _ = crate::util::io::remove_file(&path).await;
        return;
    }
    if let Ok(json) = serde_json::to_vec(state) {
        if let Err(error) = crate::util::io::write(&path, &json).await {
            tracing::warn!(
                "Shared folder: failed to persist link-state marker at {}: {error}",
                path.display()
            );
        }
    }
}

/// Makes `instance_full_path`'s linked items match what `profile` says they
/// should be (`None` = everything private) — link whatever's newly enabled,
/// unlink whatever's newly disabled, leave everything else untouched. Every
/// item is attempted independently: one item's failure is recorded and
/// skipped rather than aborting the rest, so — critically — a resource-pack
/// failure (by far the most common one; see the module doc comment) never
/// blocks worlds or config from linking correctly. Dir-kind items are driven
/// purely by their *current* on-disk state (`crate::util::link::is_link`);
/// File-kind items use the sidecar `LinkState` marker since a hard link has
/// no on-disk "is this a link?" signal to check. Either way, never trusts
/// what a previous call *attempted* — only what actually landed — so this is
/// always safe to call again.
///
/// `restore_owner` controls what unlinking an item actually does: normally
/// (`false`) it's emptied out, per the module doc comment's "real data risk"
/// section. When `true` — passed by callers only when `instance_full_path`
/// belongs to the shared profile's designated owner (see
/// `SharedProfile::owner_instance_id`) and only for the specific reconcile
/// call that's leaving it — a real private copy is restored instead
/// (`leave_item_restore`), so the owner never loses data it started sharing.
async fn reconcile_links(
    instance_full_path: &Path,
    profile: Option<&SharedProfile>,
    restore_owner: bool,
    state: &State,
) -> ReconcileOutcome {
    let mut failures = Vec::new();
    let mut link_state = read_link_state(instance_full_path).await;
    let mut link_state_changed = false;

    let profile_dir = profile.map(|p| state.directories.shared_profile_dir(&p.id));
    if let Some(dir) = &profile_dir
        && let Err(error) = crate::util::io::create_dir_all(dir).await
    {
        // Nothing below can succeed without this — record it against every
        // item that wanted to be linked, rather than a single opaque
        // failure with no item attribution.
        let message = error.to_string();
        for item in LINKED_ITEMS {
            if (item.flag)(profile.expect("profile_dir is only Some when profile is Some")) {
                failures.push((item.relative_path, message.clone()));
            }
        }
        return ReconcileOutcome { failures };
    }

    for item in LINKED_ITEMS {
        let local_path = instance_full_path.join(item.relative_path);
        let currently_linked = match item.kind {
            LinkKind::Dir => link::is_link(&local_path).await,
            LinkKind::File => link_state.linked_files.contains(item.relative_path),
        };
        let should_be_linked = profile.is_some_and(|p| (item.flag)(p));

        let result = if should_be_linked && !currently_linked {
            join_item(
                instance_full_path,
                profile_dir
                    .as_ref()
                    .expect("should_be_linked implies profile_dir is Some"),
                item,
            )
            .await
        } else if !should_be_linked && currently_linked {
            if restore_owner {
                leave_item_restore(instance_full_path, item, state).await
            } else {
                leave_item_empty(instance_full_path, item).await
            }
        } else {
            Ok(())
        };

        if result.is_ok() && matches!(item.kind, LinkKind::File) {
            let changed = if should_be_linked && !currently_linked {
                link_state.linked_files.insert(item.relative_path.to_string())
            } else if !should_be_linked && currently_linked {
                link_state.linked_files.remove(item.relative_path)
            } else {
                false
            };
            link_state_changed = link_state_changed || changed;
        }

        if let Err(error) = result {
            failures.push((item.relative_path, error.to_string()));
        }
    }

    if link_state_changed {
        write_link_state(instance_full_path, &link_state).await;
    }

    ReconcileOutcome { failures }
}

/// Detaches a shared item from the instance and empties it out locally —
/// the default behavior, used for every instance except a shared profile's
/// owner (see `leave_item_restore` for that case). An unlinked
/// `saves`/`config`/`resourcepacks` becomes a fresh empty folder; an
/// unlinked `options.txt`/`servers.dat` is simply removed (Minecraft writes
/// a normal set of defaults for either on its next launch). Never touches
/// the shared copy itself — it's still sitting under `shared_profiles/<id>/`,
/// and other instances still in the profile are unaffected; that data only
/// actually disappears if the shared profile itself is deleted (see
/// `delete_shared_profile`).
async fn leave_item_empty(
    instance_full_path: &Path,
    item: &LinkedItem,
) -> crate::Result<()> {
    let local_path = instance_full_path.join(item.relative_path);

    match item.kind {
        LinkKind::Dir => {
            if !link::is_link(&local_path).await {
                return Ok(());
            }
            link::remove_link(&local_path).await?;
            crate::util::io::create_dir_all(&local_path).await?;
        }
        LinkKind::File => {
            // A hard link has no distinct "is a link" signal to check —
            // `reconcile_links` already knows (via the `LinkState` sidecar)
            // that this path is currently hard-linked before calling us.
            // Removing the directory entry doesn't delete the underlying
            // data; the shared copy's own hard link keeps it alive.
            if tokio::fs::try_exists(&local_path).await.unwrap_or(false) {
                tokio::fs::remove_file(&local_path).await.map_err(|e| {
                    crate::ErrorKind::FSError(format!(
                        "Failed to unlink {}: {e}",
                        local_path.display()
                    ))
                    .as_error()
                })?;
            }
        }
    }

    Ok(())
}

/// Like `leave_item_empty`, but restores a full private copy of the shared
/// content instead of leaving the item empty. Used only when
/// `instance_full_path` belongs to the shared profile's designated "owner"
/// (`SharedProfile::owner_instance_id`) — the instance that originally
/// created it — so that instance keeps a real "master copy" and never loses
/// data it started sharing, however it's detached: leaving the profile,
/// turning one item off while staying a member, or the shared profile being
/// deleted out from under it.
async fn leave_item_restore(
    instance_full_path: &Path,
    item: &LinkedItem,
    state: &State,
) -> crate::Result<()> {
    let local_path = instance_full_path.join(item.relative_path);

    match item.kind {
        LinkKind::Dir => {
            if !link::is_link(&local_path).await {
                return Ok(());
            }
            let target = link::read_link_target(&local_path).await?;
            link::remove_link(&local_path).await?;

            if tokio::fs::try_exists(&target).await.unwrap_or(false) {
                copy_dir_recursive(&target, &local_path, state).await?;
            } else {
                crate::util::io::create_dir_all(&local_path).await?;
            }
        }
        LinkKind::File => {
            // Same hard-link situation as `leave_item_empty` — this path
            // already holds the right bytes (it's literally the same file
            // as the shared copy) — but here we keep them: read the current
            // bytes out, remove this directory entry (which does *not*
            // delete the data — the shared copy's own hard link keeps it
            // alive), then write those bytes back as an independent file.
            if let Ok(bytes) = tokio::fs::read(&local_path).await {
                tokio::fs::remove_file(&local_path).await.map_err(|e| {
                    crate::ErrorKind::FSError(format!(
                        "Failed to unlink {}: {e}",
                        local_path.display()
                    ))
                    .as_error()
                })?;
                tokio::fs::write(&local_path, bytes).await.map_err(|e| {
                    crate::ErrorKind::FSError(format!(
                        "Failed to restore the owner's private copy of {}: {e}",
                        local_path.display()
                    ))
                    .as_error()
                })?;
            }
        }
    }

    Ok(())
}

/// Recursively copies a directory's contents from `source` to `target`
/// (which must not already exist as anything other than empty/missing).
/// Used only by `leave_item_restore`, to give a shared profile's owner a
/// real copy of the shared data rather than an empty folder. Mirrors
/// `crate::install::recovery::copy_directory`'s approach — skips symlinks
/// entirely rather than following them, since nothing under
/// saves/config/resourcepacks is expected to contain one. Individual files
/// that vanish between being listed and being copied (most commonly because
/// this app's own content-sync is concurrently touching mods/resourcepacks
/// in the background — see the module doc comment) are skipped with a
/// warning rather than failing the whole copy; anything else is a real error
/// and still propagates.
async fn copy_dir_recursive(
    source: &Path,
    target: &Path,
    state: &State,
) -> crate::Result<()> {
    crate::util::io::create_dir_all(target).await?;

    let mut walker = WalkDir::new(source);
    while let Some(entry) = walker.next().await {
        let entry = entry.map_err(|error| {
            crate::ErrorKind::FSError(format!(
                "Failed to read shared folder path: {error}"
            ))
        })?;
        let entry_path = entry.path();
        let relative_path = entry_path.strip_prefix(source)?;
        let target_path = target.join(relative_path);
        let file_type = entry.file_type().await?;

        if file_type.is_dir() {
            crate::util::io::create_dir_all(&target_path).await?;
        } else if file_type.is_file() {
            if let Err(error) = crate::util::fetch::copy(
                &entry_path,
                &target_path,
                &state.io_semaphore,
            )
            .await
            {
                if tokio::fs::try_exists(&entry_path).await.unwrap_or(true) {
                    return Err(error);
                }
                tracing::warn!(
                    "Shared folder copy: source vanished mid-copy, skipping {}",
                    entry_path.display()
                );
            }
        }
    }

    Ok(())
}

/// Links an instance's own path for `item` into the shared profile's copy.
/// If the shared profile doesn't have this item yet, this instance's own
/// real data (if any) seeds it — otherwise an empty one is created so
/// there's something to link to. If the shared profile *already* has this
/// item and the instance also has its own real local data, the instance's
/// copy is renamed (never deleted) to `<name>.pre-shared-backup` first, so
/// nothing is silently lost.
async fn join_item(
    instance_full_path: &Path,
    shared_profile_dir: &Path,
    item: &LinkedItem,
) -> crate::Result<()> {
    let local_path = instance_full_path.join(item.relative_path);
    let shared_path = shared_profile_dir.join(item.relative_path);

    let shared_exists = tokio::fs::try_exists(&shared_path).await.unwrap_or(false);
    let local_exists = tokio::fs::try_exists(&local_path).await.unwrap_or(false);

    if !shared_exists {
        if local_exists {
            crate::util::io::rename_or_move(&local_path, &shared_path)
                .await
                .map_err(|e| {
                    crate::ErrorKind::FSError(format!(
                        "Failed to move {} into its new shared folder: {e}",
                        local_path.display()
                    ))
                    .as_error()
                })?;
        } else {
            match item.kind {
                LinkKind::Dir => {
                    crate::util::io::create_dir_all(&shared_path).await?;
                }
                LinkKind::File => {
                    tokio::fs::write(&shared_path, b"").await.map_err(|e| {
                        crate::ErrorKind::FSError(format!(
                            "Failed to create {}: {e}",
                            shared_path.display()
                        ))
                        .as_error()
                    })?;
                }
            }
        }
    } else if local_exists {
        let backup_path = backup_path_for(&local_path);
        crate::util::io::rename_or_move(&local_path, &backup_path)
            .await
            .map_err(|e| {
                crate::ErrorKind::FSError(format!(
                    "Failed to back up {} before linking it to the shared folder: {e}",
                    local_path.display()
                ))
                .as_error()
            })?;
    }

    match item.kind {
        LinkKind::Dir => link::create_dir_link(&shared_path, &local_path).await?,
        LinkKind::File => {
            link::create_file_link(&shared_path, &local_path).await?
        }
    }

    Ok(())
}

fn backup_path_for(path: &Path) -> PathBuf {
    let base_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut candidate = path.with_file_name(format!("{base_name}.pre-shared-backup"));
    let mut attempt = 2;
    while candidate.exists() {
        candidate =
            path.with_file_name(format!("{base_name}.pre-shared-backup-{attempt}"));
        attempt += 1;
    }
    candidate
}
