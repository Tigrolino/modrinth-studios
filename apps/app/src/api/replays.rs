//! Modrinth Studios addition: exposes `theseus::replays` to the frontend.
//! New plugin file — doesn't touch any existing upstream command module.
//! "Show in folder" doesn't need a command here: the frontend already has
//! `plugin:utils|highlight_in_folder`, and `Replay::folder` (returned by
//! `replays_list`) plus the instance's already-known full path is enough to
//! build the absolute path, same as `showWorldInFolder` does for worlds.

use crate::api::Result;
use tauri::Runtime;
use theseus::instance::get_full_path;
pub use theseus::replays::{Replay, ReplayKind};
use theseus::replays;

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("replays")
        .invoke_handler(tauri::generate_handler![
            replays_has_any,
            replays_list,
            replays_delete,
            replays_rename,
            replays_import,
            replays_thumbnail,
        ])
        .build()
}

#[tauri::command]
pub async fn replays_has_any(instance_id: &str) -> Result<bool> {
    let instance = get_full_path(instance_id).await?;
    Ok(replays::has_replays(&instance).await?)
}

#[tauri::command]
pub async fn replays_list(instance_id: &str) -> Result<Vec<Replay>> {
    let instance = get_full_path(instance_id).await?;
    Ok(replays::list_replays(&instance, instance_id).await?)
}

#[tauri::command]
pub async fn replays_delete(
    instance_id: &str,
    kind: ReplayKind,
    file_name: &str,
) -> Result<()> {
    let instance = get_full_path(instance_id).await?;
    replays::delete_replay(&instance, kind, file_name).await?;
    Ok(())
}

#[tauri::command]
pub async fn replays_rename(
    instance_id: &str,
    kind: ReplayKind,
    file_name: &str,
    new_file_name: &str,
) -> Result<()> {
    let instance = get_full_path(instance_id).await?;
    replays::rename_replay(&instance, kind, file_name, new_file_name)
        .await?;
    Ok(())
}

/// `source_path` is an absolute path to a replay file the user picked via
/// the native file dialog on the frontend.
#[tauri::command]
pub async fn replays_import(
    instance_id: &str,
    source_path: &str,
) -> Result<()> {
    let instance = get_full_path(instance_id).await?;
    replays::import_replay(&instance, source_path).await?;
    Ok(())
}

/// Returns a `data:` URL for the replay's embedded thumbnail, or `None` if
/// it doesn't have one. Deliberately separate from `replays_list` — see the
/// comment on `get_replay_thumbnail` for why.
#[tauri::command]
pub async fn replays_thumbnail(
    instance_id: &str,
    kind: ReplayKind,
    file_name: &str,
) -> Result<Option<String>> {
    let instance = get_full_path(instance_id).await?;
    Ok(replays::get_replay_thumbnail(&instance, kind, file_name).await?)
}
