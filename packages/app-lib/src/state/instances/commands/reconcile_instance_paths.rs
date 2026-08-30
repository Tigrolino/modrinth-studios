//! Modrinth Studios addition: instances are tracked purely by a `path`
//! column in the database (see `CreateInstance`/`Instance`) — nothing on
//! disk records which folder belongs to which instance. That means renaming
//! an instance's folder outside the app (in Explorer, or any other file
//! manager) silently breaks it: the database still points at the old,
//! now-missing folder name, while the renamed folder isn't recognized as
//! anything at all.
//!
//! The fix is a small marker file written into every instance folder's
//! root, holding that instance's own stable id — independent of whatever
//! the folder is named. `reconcile_instance_paths()` below runs once at
//! startup and uses it to match a renamed folder back to its database
//! record, updating the tracked path to follow.

use crate::state::State;
use crate::state::instances::adapters::sqlite::instance_rows;
use crate::util::io;
use std::collections::HashSet;
use std::path::Path;

const INSTANCE_MARKER_FILE_NAME: &str = ".modrinth-studio-instance.json";

#[derive(serde::Serialize, serde::Deserialize)]
struct InstanceMarker {
    id: String,
}

/// Writes (or overwrites) the marker file for `instance_id` inside
/// `instance_dir`. Best-effort and silent on failure — this is a
/// convenience against one specific external-rename scenario, not
/// something instance creation or startup should ever be blocked by.
pub(crate) async fn write_instance_marker(
    instance_dir: &Path,
    instance_id: &str,
) {
    let marker = InstanceMarker {
        id: instance_id.to_string(),
    };
    let Ok(json) = serde_json::to_vec_pretty(&marker) else {
        return;
    };
    if let Err(e) =
        io::write(instance_dir.join(INSTANCE_MARKER_FILE_NAME), &json).await
    {
        tracing::warn!(
            "Failed to write instance marker for {instance_id}: {e}"
        );
    }
}

async fn read_instance_marker_id(instance_dir: &Path) -> Option<String> {
    let bytes =
        tokio::fs::read(instance_dir.join(INSTANCE_MARKER_FILE_NAME))
            .await
            .ok()?;
    let marker: InstanceMarker = serde_json::from_slice(&bytes).ok()?;
    Some(marker.id)
}

/// Run once at startup, before the file watcher is set up for existing
/// instances (see `State::init()` — this must run first, so the watcher
/// registers against corrected paths). Two jobs:
///
/// 1. **Backfill** — writes the marker file into any *currently tracked*
///    instance's folder that doesn't have one yet, so instances that
///    existed before this feature was added become rename-safe too, not
///    just newly-created ones. This can't retroactively recover a rename
///    that already happened before the marker existed — there was nothing
///    to read at the time — but protects every instance going forward.
/// 2. **Reconciliation** — finds instances whose database `path` no longer
///    exists on disk ("missing") and folders under the instances directory
///    that don't match any tracked instance's path ("orphans"), and for any
///    orphan whose marker file's id matches a missing instance, updates
///    that instance's `path` in the database to the orphan's current
///    folder name — this is what actually makes "I renamed the folder in
///    Explorer" not break the instance. A folder with no marker, or a
///    marker that doesn't match any missing instance, is left completely
///    alone: it might just be unrelated clutter, or a folder the person
///    copied by hand outside the app, and either way there's nothing safe
///    to infer about it.
pub(crate) async fn reconcile_instance_paths(
    state: &State,
) -> crate::Result<()> {
    let instances_dir = state.directories.instances_dir();
    let tracked = instance_rows::list_instances(&state.pool).await?;

    // Modrinth Studios addition: this used to check + backfill each tracked
    // instance one at a time, awaiting every `metadata()`/marker read+write
    // in sequence before moving to the next — with 20-30 instances, that's
    // 20-30 round trips of disk latency stacked up on *every single app
    // launch*, which is the actual cause of "modrinth takes ages to load
    // now". Two fixes: run every instance's check concurrently
    // (`join_all`, same reasoning as `instance_storage_usage()`'s own fix —
    // this is I/O-bound, not CPU-bound, so there's real wall-clock time to
    // overlap), and skip the marker write entirely for an instance that
    // already has a correct one — the backfill only actually needs to
    // *write* anything the first time a given instance is ever seen by
    // this feature, not on every single boot after that forever.
    let checked = futures::future::join_all(tracked.iter().map(|instance| {
        let instances_dir = &instances_dir;
        async move {
            let full_path = instances_dir.join(&instance.path);
            let exists = tokio::fs::metadata(&full_path).await.is_ok();
            if exists {
                let already_correct =
                    read_instance_marker_id(&full_path).await.as_deref()
                        == Some(instance.id.as_str());
                if !already_correct {
                    write_instance_marker(&full_path, &instance.id).await;
                }
            }
            (instance.clone(), exists)
        }
    }))
    .await;

    let mut missing = Vec::new();
    let mut tracked_paths: HashSet<String> = HashSet::new();
    for (instance, exists) in checked {
        tracked_paths.insert(instance.path.clone());
        if !exists {
            missing.push(instance);
        }
    }

    if missing.is_empty() {
        return Ok(());
    }

    let Ok(mut entries) = tokio::fs::read_dir(&instances_dir).await else {
        return Ok(());
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        if missing.is_empty() {
            break;
        }

        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let path = entry.path();
        let Some(folder_name) = path.file_name().and_then(|n| n.to_str())
        else {
            continue;
        };
        if tracked_paths.contains(folder_name) {
            continue;
        }

        let Some(marker_id) = read_instance_marker_id(&path).await else {
            continue;
        };
        let Some(position) =
            missing.iter().position(|instance| instance.id == marker_id)
        else {
            continue;
        };

        let instance = missing.remove(position);
        tracing::info!(
            "Instance '{}' ({}) appears to have been renamed on disk from '{}' to '{folder_name}' — updating its tracked path and name",
            instance.name,
            instance.id,
            instance.path,
        );

        // Modrinth Studios addition: also update the display name to match
        // — see update_instance_path_and_name()'s doc comment for why. Only
        // reached when the folder name actually changed (that's the whole
        // reconciliation branch this is in), so no need to check it differs
        // from the current name first.
        if let Err(e) = instance_rows::update_instance_path_and_name(
            &instance.id,
            folder_name,
            folder_name,
            &state.pool,
        )
        .await
        {
            tracing::warn!(
                "Failed to update tracked path for renamed instance {}: {e}",
                instance.id
            );
            continue;
        }

        tracked_paths.insert(folder_name.to_string());

        crate::state::instances::watcher::watch_instance_folder(
            &instance.id,
            folder_name,
            &state.file_watcher,
            &state.directories,
        )
        .await;

        let _ = crate::event::emit::emit_instance(
            &instance.id,
            crate::event::InstancePayloadType::Edited,
        )
        .await;
    }

    Ok(())
}
