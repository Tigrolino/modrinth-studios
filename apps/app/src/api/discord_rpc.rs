//! Modrinth Studios addition: exposes `theseus::discord_rpc` to the
//! frontend. New plugin file — doesn't touch any existing upstream command
//! module. See `packages/app-lib/src/api/discord_rpc.rs` for what this
//! actually does.

use crate::api::Result;
use theseus::discord_rpc::{self, DiscordRpcSettings};

pub fn init<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("discord-rpc")
        .invoke_handler(tauri::generate_handler![
            discord_rpc_get_settings,
            discord_rpc_set_settings,
        ])
        .build()
}

#[tauri::command]
pub async fn discord_rpc_get_settings() -> Result<DiscordRpcSettings> {
    Ok(discord_rpc::get_settings().await?)
}

/// Saves the settings, then immediately re-applies the current Discord
/// activity so the change shows up without restarting the app.
#[tauri::command]
pub async fn discord_rpc_set_settings(
    settings: DiscordRpcSettings,
) -> Result<()> {
    discord_rpc::set_settings(settings).await?;
    Ok(())
}
