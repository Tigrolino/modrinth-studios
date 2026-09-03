//! Modrinth Studios addition: keeps an instance's on-disk folder name in
//! sync with its display name when it's renamed via Settings — this is the
//! "when I rename it in Modrinth it renames the folder" half of the
//! instance-rename feature. `reconcile_instance_paths.rs` handles the
//! *other* direction (a folder renamed externally in Explorer, without
//! breaking tracking); the two are complementary and don't step on each
//! other — this always leaves the instance's marker file in place (renaming
//! a folder brings every file inside it along, untouched), so if someone
//! later renames the result again in Explorer, reconciliation still
//! recognizes it correctly.

use super::create_instance::resolve_instance_path;
use crate::state::State;
use crate::state::instances::adapters::sqlite::instance_rows;
use crate::state::instances::watcher;

/// Renames `instance_id`'s on-disk folder to match `new_name`, if it
/// doesn't already (sanitized/collision-resolved exactly like a brand new
/// instance's folder would be — see `resolve_instance_path()`). No-op if
/// that resolves to the same folder name the instance already has.
///
/// Returns an error, without touching the database or the folder, if the
/// instance is currently running — never rename a live instance's folder
/// out from under it. On any other failure (most commonly Windows refusing
/// because something still has a file inside the folder open) the folder is
/// left exactly as it was and the error is returned; the caller should stop
/// there and not go on to apply the rest of an edit (in particular, not the
/// display-name change itself), so the name and the folder can never end up
/// disagreeing with each other.
pub(crate) async fn rename_instance_folder_for_name_change(
    instance_id: &str,
    new_name: &str,
    state: &State,
) -> crate::Result<()> {
    let Some(instance) =
        instance_rows::get_instance_by_id(instance_id, &state.pool).await?
    else {
        return Ok(());
    };

    let running = state
        .process_manager
        .get_all()
        .into_iter()
        .any(|process| process.instance_id == instance_id);
    if running {
        return Err(crate::ErrorKind::InputError(
            "Can't rename this instance's folder while it's running — stop it first."
                .to_string(),
        )
        .into());
    }

    let (new_path, new_full_path) =
        resolve_instance_path(new_name, None, Some(instance_id), state).await?;

    if new_path == instance.path {
        // Sanitizes/collision-resolves to the exact folder name it already
        // has (e.g. renaming "Final Station" to "final station" — Modrinth
        // doesn't distinguish those on disk either way) — nothing to move.
        return Ok(());
    }

    let old_path = instance.path;
    let old_full_path = state.directories.instances_dir().join(&old_path);

    // Drop this instance's watches *before* renaming. On Windows, the
    // watcher holds an open directory handle on every instance folder for
    // as long as the app is running (see watch_instance_folder()) — which
    // by itself is enough for Windows to refuse a rename, including one
    // coming from this same process. This is also, incidentally, exactly
    // why an *external* rename in Explorer fails with "folder in use" the
    // whole time the app is open at all, regardless of which page is
    // showing — but here we control both ends, so we can just stop
    // watching, rename, then watch the new location instead.
    watcher::unwatch_instance_folder(
        instance_id,
        &old_path,
        &state.file_watcher,
        &state.directories,
    )
    .await;

    // Modrinth Studios addition: `unwatch_instance_folder()` above asks the
    // underlying OS-level watch (`ReadDirectoryChangesW` on Windows) to
    // stop, but that isn't guaranteed to release its directory handle the
    // instant the call returns — a rename attempted immediately afterward
    // can still lose a race against that teardown. Retry a few times with a
    // short, increasing delay before giving up; this only ever helps with
    // that specific race and costs nothing when the rename just succeeds on
    // the first try (the overwhelmingly common case).
    let mut last_err = None;
    let mut renamed = false;
    for attempt in 0..5u32 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(
                150 * u64::from(attempt),
            ))
            .await;
        }
        match crate::util::io::rename_or_move(&old_full_path, &new_full_path)
            .await
        {
            Ok(()) => {
                renamed = true;
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }

    if !renamed {
        // The rename didn't happen — re-establish the watch we just dropped
        // so a failed rename doesn't also silently stop live-update
        // tracking for this instance until the next launch.
        watcher::watch_instance_folder(
            instance_id,
            &old_path,
            &state.file_watcher,
            &state.directories,
        )
        .await;
        // last_err is always Some here — the loop above only exits with
        // renamed still false after every attempt has recorded an error.
        return Err(last_err
            .expect("rename retry loop always records an error on failure")
            .into());
    }

    instance_rows::update_instance_path(instance_id, &new_path, &state.pool)
        .await?;

    watcher::watch_instance_folder(
        instance_id,
        &new_path,
        &state.file_watcher,
        &state.directories,
    )
    .await;

    Ok(())
}
