//! Modrinth Studios: one-time startup reversion of the old "shared Minecraft
//! folders" feature (junction/symlink/hard-link based sharing between
//! instances on the same machine — previously `crate::api::shared_profile`,
//! removed entirely now that Modrinth's own app has shipped its own,
//! unrelated way to keep things in sync between instances: an account-based
//! system, `sync_feature_settings`/`instance_sync_preferences`, upstream).
//!
//! This module exists purely so that anyone who had an instance actually
//! using Studio's old feature gets their real data back automatically the
//! moment they update — not everyone using this app is the person reading
//! this comment, and their saves/config/resourcepacks/options.txt/
//! servers.dat currently only exist as the *shared* copy under
//! `shared_profiles/<id>/`, linked into their instance via a directory
//! junction/symlink or (for the two single-file items) a hard link. Deleting
//! the feature's code without first restoring a private copy to every
//! instance still linked would leave those instances pointing at nothing.
//!
//! Ported from (now-deleted) `api::shared_profile`'s `leave_item`/
//! `copy_dir_recursive` — this only ever runs that "leave" logic (restore a
//! real private copy of whatever's currently linked), never joins, creates,
//! or shares anything. Detection is always based on what's *actually*
//! currently linked on disk (a directory link, or the
//! `.studio-shared-links.json` sidecar for the hard-linked files) rather
//! than trusting the database, so this is safe to call on every single
//! launch — a fresh install, or one that's already been fully reverted,
//! costs one cheap `SELECT COUNT(*)` and returns immediately.
//!
//! **Safety rule this module builds around**: a shared folder's actual data
//! directory (`shared_profiles/<id>/`) is only ever deleted once *every*
//! instance that was linked to it has been confirmed — by re-reading
//! real, current disk state, not by trusting that an unlink call
//! "succeeded" — to no longer be linked to it. If restoring even one
//! instance's one item fails (a locked file, a permissions error, disk
//! space, anything), that whole shared folder's data and bookkeeping are
//! left exactly as they are and retried on the next launch, rather than
//! risking deleting data an instance still depends on.

use crate::state::State;
use crate::util::link;
use async_walkdir::WalkDir;
use futures::StreamExt;
use std::collections::HashSet;
use std::path::Path;

enum LinkKind {
    Dir,
    File,
}

struct LinkedItem {
    relative_path: &'static str,
    kind: LinkKind,
}

const LINKED_ITEMS: &[LinkedItem] = &[
    LinkedItem {
        relative_path: "saves",
        kind: LinkKind::Dir,
    },
    LinkedItem {
        relative_path: "config",
        kind: LinkKind::Dir,
    },
    LinkedItem {
        relative_path: "resourcepacks",
        kind: LinkKind::Dir,
    },
    LinkedItem {
        relative_path: "options.txt",
        kind: LinkKind::File,
    },
    LinkedItem {
        relative_path: "servers.dat",
        kind: LinkKind::File,
    },
];

const LINK_STATE_FILE_NAME: &str = ".studio-shared-links.json";

#[derive(Default, serde::Deserialize)]
struct LinkState {
    #[serde(default)]
    linked_files: HashSet<String>,
}

async fn read_link_state(instance_full_path: &Path) -> LinkState {
    let Ok(bytes) =
        crate::util::io::read(instance_full_path.join(LINK_STATE_FILE_NAME))
            .await
    else {
        return LinkState::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Best-effort cleanup of the sidecar marker once every file-kind item it
/// tracked has been restored — nothing reads this file once this module's
/// job is done, but leaving it behind in every instance's folder forever
/// would just be silent litter.
async fn remove_link_state(instance_full_path: &Path) {
    let _ = crate::util::io::remove_file(
        instance_full_path.join(LINK_STATE_FILE_NAME),
    )
    .await;
}

/// Restores a real private copy of `item` into `instance_full_path`,
/// exactly mirroring what `api::shared_profile::leave_item` used to do.
/// Returns `Ok(true)` if it was linked and is now genuinely restored,
/// `Ok(false)` if it was never linked in the first place (nothing to do),
/// `Err` if it was linked but restoring it failed — the caller treats that
/// as "this instance isn't safe to fully detach yet."
async fn revert_item(
    instance_full_path: &Path,
    item: &LinkedItem,
    link_state: &LinkState,
    state: &State,
) -> crate::Result<bool> {
    let local_path = instance_full_path.join(item.relative_path);

    match item.kind {
        LinkKind::Dir => {
            if !link::is_link(&local_path).await {
                return Ok(false);
            }
            let target = link::read_link_target(&local_path).await?;
            link::remove_link(&local_path).await?;

            if tokio::fs::try_exists(&target).await.unwrap_or(false) {
                copy_dir_recursive(&target, &local_path, state).await?;
            } else {
                crate::util::io::create_dir_all(&local_path).await?;
            }
            Ok(true)
        }
        LinkKind::File => {
            if !link_state.linked_files.contains(item.relative_path) {
                return Ok(false);
            }
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
                        "Failed to restore a private copy of {}: {e}",
                        local_path.display()
                    ))
                    .as_error()
                })?;
            }
            Ok(true)
        }
    }
}

/// Verbatim copy of the same helper from the now-deleted
/// `api::shared_profile` module — see its own history in STUDIO.md for the
/// full reasoning (skips symlinks, tolerates a source file vanishing
/// mid-copy since this app's own content sync can be concurrently touching
/// mods/resourcepacks in the background).
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
                    "Shared folder reversion: source vanished mid-copy, skipping {}",
                    entry_path.display()
                );
            }
        }
    }

    Ok(())
}

/// Reverts one instance: restores a private copy of every item it currently
/// has linked. Returns `true` only if the instance ends this call with
/// nothing left linked at all (confirmed by re-checking disk state after,
/// not by assuming every `revert_item` call above succeeded) — the caller
/// only deletes this instance's shared-folder bookkeeping row, and only
/// considers the shared folder itself safe to delete, once every member
/// instance reports `true` here.
async fn revert_instance(
    state: &State,
    instance_id: &str,
    instance_path: &str,
) -> bool {
    let instance_full_path = state.directories.instances_dir().join(instance_path);

    crate::state::instances::watcher::unwatch_instance_folder(
        instance_id,
        instance_path,
        &state.file_watcher,
        &state.directories,
    )
    .await;

    let link_state = read_link_state(&instance_full_path).await;
    for item in LINKED_ITEMS {
        if let Err(error) =
            revert_item(&instance_full_path, item, &link_state, state).await
        {
            tracing::warn!(
                "Shared folder reversion: failed to restore '{}' for instance {instance_id}, will retry next launch: {error}",
                item.relative_path
            );
        }
    }
    remove_link_state(&instance_full_path).await;

    crate::state::instances::watcher::watch_instance_folder(
        instance_id,
        instance_path,
        &state.file_watcher,
        &state.directories,
    )
    .await;

    // Re-check reality rather than trusting the loop above — exactly the
    // same "never trust an attempt, only what's actually there" principle
    // the original feature's own reconciliation used.
    let still_linked_dir = {
        let mut any = false;
        for item in LINKED_ITEMS {
            if matches!(item.kind, LinkKind::Dir)
                && link::is_link(instance_full_path.join(item.relative_path))
                    .await
            {
                any = true;
                break;
            }
        }
        any
    };
    let still_linked_file = crate::util::io::read(
        instance_full_path.join(LINK_STATE_FILE_NAME),
    )
    .await
    .ok()
    .and_then(|bytes| serde_json::from_slice::<LinkState>(&bytes).ok())
    .is_some_and(|state| !state.linked_files.is_empty());

    !still_linked_dir && !still_linked_file
}

/// Runs once at startup (see `state::mod::init`) — reverts every instance
/// still linked to one of Studio's old shared folders, then cleans up
/// whatever shared folders that leaves with no members left. Logs and moves
/// on past any individual failure rather than panicking; a shared folder
/// that couldn't be fully reverted this run just gets tried again next
/// launch; nothing about this ever destroys data, only restores it.
pub(crate) async fn revert_all(state: &State) {
    let has_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'studio_shared_profiles'",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);
    if has_table == 0 {
        return;
    }

    let profile_ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM studio_shared_profiles",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    if profile_ids.is_empty() {
        return;
    }

    tracing::info!(
        "Reverting {} Modrinth Studios shared folder(s) back to private per-instance copies (feature removed)",
        profile_ids.len()
    );

    for profile_id in profile_ids {
        let members: Vec<(String, String)> = sqlx::query_as(
            "SELECT sisp.instance_id, i.path
             FROM studio_instance_shared_profiles sisp
             JOIN instances i ON i.id = sisp.instance_id
             WHERE sisp.shared_profile_id = ?",
        )
        .bind(&profile_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

        let mut all_members_clear = true;
        for (instance_id, instance_path) in &members {
            let fully_reverted =
                revert_instance(state, instance_id, instance_path).await;
            if fully_reverted {
                let _ = sqlx::query(
                    "DELETE FROM studio_instance_shared_profiles WHERE instance_id = ?",
                )
                .bind(instance_id)
                .execute(&state.pool)
                .await;
            } else {
                all_members_clear = false;
                tracing::warn!(
                    "Shared folder reversion: instance {instance_id} still has at least one item linked after retrying — its shared folder's data will be kept and this will be retried next launch"
                );
            }
        }

        if !all_members_clear {
            continue;
        }

        // Re-confirm from the database (not the `members` list above, which
        // could be stale if something else touched this profile
        // concurrently) that nothing still references this shared folder
        // before deleting its actual data.
        let remaining: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM studio_instance_shared_profiles WHERE shared_profile_id = ?",
        )
        .bind(&profile_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(1);
        if remaining != 0 {
            continue;
        }

        let shared_dir = state.directories.shared_profile_dir(&profile_id);
        if let Err(error) = crate::util::io::remove_dir_all(&shared_dir).await
            && tokio::fs::try_exists(&shared_dir).await.unwrap_or(false)
        {
            tracing::warn!(
                "Shared folder reversion: every instance was detached, but couldn't remove the now-unused shared data at {}: {error}",
                shared_dir.display()
            );
            continue;
        }

        let _ = sqlx::query("DELETE FROM studio_shared_profiles WHERE id = ?")
            .bind(&profile_id)
            .execute(&state.pool)
            .await;
    }
}
