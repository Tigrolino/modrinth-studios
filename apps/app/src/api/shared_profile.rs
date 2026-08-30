//! Modrinth Studios addition: exposes `theseus::shared_profile` to the
//! frontend. New plugin file — doesn't touch any existing upstream command
//! module. See `packages/app-lib/src/api/shared_profile.rs` for what this
//! actually does and why it's a separate pair of tables instead of a new
//! `instances` column.

use crate::api::Result;
use theseus::shared_profile::{self, SharedProfile};

pub fn init<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("shared-profile")
        .invoke_handler(tauri::generate_handler![
            shared_profile_list,
            shared_profile_create,
            shared_profile_rename,
            shared_profile_update_items,
            shared_profile_delete,
            shared_profile_get_for_instance,
            shared_profile_set_for_instance,
        ])
        .build()
}

#[tauri::command]
pub async fn shared_profile_list() -> Result<Vec<SharedProfile>> {
    Ok(shared_profile::list_shared_profiles().await?)
}

#[tauri::command]
pub async fn shared_profile_create(
    name: String,
    owner_instance_id: Option<String>,
) -> Result<SharedProfile> {
    Ok(shared_profile::create_shared_profile(name, owner_instance_id).await?)
}

#[tauri::command]
pub async fn shared_profile_rename(
    id: String,
    new_name: String,
) -> Result<SharedProfile> {
    Ok(shared_profile::rename_shared_profile(id, new_name).await?)
}

/// Updates which items a shared folder shares, then re-syncs every instance
/// currently using it (skipping any that are running right now).
#[tauri::command]
pub async fn shared_profile_update_items(
    id: String,
    share_saves: bool,
    share_config: bool,
    share_resourcepacks: bool,
    share_options: bool,
    share_servers: bool,
) -> Result<SharedProfile> {
    Ok(shared_profile::update_shared_profile_items(
        id,
        share_saves,
        share_config,
        share_resourcepacks,
        share_options,
        share_servers,
    )
    .await?)
}

#[tauri::command]
pub async fn shared_profile_delete(id: String) -> Result<()> {
    shared_profile::delete_shared_profile(id).await?;
    Ok(())
}

/// The shared profile `instance_id` currently belongs to, if any.
#[tauri::command]
pub async fn shared_profile_get_for_instance(
    instance_id: &str,
) -> Result<Option<SharedProfile>> {
    Ok(shared_profile::get_instance_shared_profile(instance_id).await?)
}

/// Joins `instance_id` to `shared_profile_id`, or leaves whatever shared
/// profile it's currently in if `shared_profile_id` is `null`. Refuses while
/// the instance is running.
#[tauri::command]
pub async fn shared_profile_set_for_instance(
    instance_id: &str,
    shared_profile_id: Option<String>,
) -> Result<()> {
    shared_profile::set_instance_shared_profile(
        instance_id,
        shared_profile_id.as_deref(),
    )
    .await?;
    Ok(())
}
