//! Modrinth Studios addition: small grab-bag of app-only commands that don't
//! belong in `theseus` (app-lib) because they're purely cosmetic/runtime and
//! specific to this fork, not general launcher functionality. New plugin
//! file, doesn't touch any upstream module.

use crate::api::Result;
use tauri::{Manager, Runtime};

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("studio")
        .invoke_handler(tauri::generate_handler![
            studio_set_background_image,
            studio_set_app_icon
        ])
        .build()
}

async fn copy_into_studio_dir<R: Runtime>(
    app: &tauri::AppHandle<R>,
    source_path: &str,
    dest_stem: &str,
) -> Result<String> {
    let config_dir = app.path().app_config_dir()?;
    let studio_dir = config_dir.join("studio");
    tokio::fs::create_dir_all(&studio_dir).await?;

    let source = std::path::PathBuf::from(source_path);
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let dest = studio_dir.join(format!("{dest_stem}.{extension}"));

    tokio::fs::copy(&source, &dest).await?;

    Ok(dest.to_string_lossy().into_owned())
}

/// Copies a user-picked image into `$APPCONFIG/studio/` (overwriting any
/// previous background) and returns the destination path, so the frontend
/// can reference it with `convertFileSrc` — the webview can't load arbitrary
/// filesystem paths directly, only ones inside the asset-protocol scope
/// declared in `apps/app/tauri.conf.json`.
#[tauri::command]
pub async fn studio_set_background_image<R: Runtime>(
    app: tauri::AppHandle<R>,
    source_path: String,
) -> Result<String> {
    copy_into_studio_dir(&app, &source_path, "background").await
}

/// Copies a user-picked app icon into `$APPCONFIG/studio/` and returns the
/// destination path. Copying it (instead of using the originally-picked path
/// directly) matters for two reasons: the original path is very likely
/// outside the webview's asset-protocol scope, so the in-app title bar
/// preview (loaded via `convertFileSrc`) would silently fail to display it;
/// and the source file could later be moved, renamed, or deleted by the
/// user, which would otherwise break the icon on the next launch.
#[tauri::command]
pub async fn studio_set_app_icon<R: Runtime>(
    app: tauri::AppHandle<R>,
    source_path: String,
) -> Result<String> {
    copy_into_studio_dir(&app, &source_path, "icon").await
}
