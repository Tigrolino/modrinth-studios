//! Modrinth Studios addition: exposes `theseus::api::playtime_correction` to
//! the frontend. New plugin file — doesn't touch any existing upstream
//! command module. See `packages/app-lib/src/api/playtime_correction.rs` for
//! why this is a separate table/module instead of a new `instances` column.

use crate::api::Result;
use theseus::playtime_correction;

pub fn init<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("playtime-correction")
        .invoke_handler(tauri::generate_handler![
            playtime_correction_get,
            playtime_correction_set,
        ])
        .build()
}

/// Current correction, in seconds. `0` if none has been set.
#[tauri::command]
pub async fn playtime_correction_get(instance_id: &str) -> Result<i64> {
    Ok(playtime_correction::get_correction_seconds(instance_id).await?)
}

/// Sets the correction to an exact value, in seconds — overwrites, not adds.
#[tauri::command]
pub async fn playtime_correction_set(
    instance_id: &str,
    seconds: i64,
) -> Result<()> {
    playtime_correction::set_correction_seconds(instance_id, seconds)
        .await?;
    Ok(())
}
