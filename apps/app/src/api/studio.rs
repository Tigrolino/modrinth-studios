//! Modrinth Studios addition: small grab-bag of app-only commands that don't
//! belong in `theseus` (app-lib) because they're purely cosmetic/runtime and
//! specific to this fork, not general launcher functionality. New plugin
//! file, doesn't touch any upstream module.

use crate::api::Result;
use tauri::{Manager, Runtime};

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("studio")
        .invoke_handler(tauri::generate_handler![
            studio_set_background_images,
            studio_set_background_folder,
            studio_set_background_videos,
            studio_set_app_icon,
            studio_set_splash_background
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

    // Modrinth Studios addition: this used to always copy to the exact same
    // filename (e.g. always `background.png`), so picking a *different*
    // image with the same extension produced the exact same destination
    // path/URL as before. The webview loads these via convertFileSrc, and
    // both it and the OS can cache an image by URL — overwriting the file
    // in place without changing its name meant a fresh pick could silently
    // keep showing the old file's cached bytes instead of the new one. This
    // is the "some images just don't work, the same ones work other times"
    // bug — it depended on cache state, not the image itself. Giving every
    // copy a unique name guarantees a new URL each time, so there's nothing
    // stale to serve.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dest = studio_dir.join(format!("{dest_stem}-{unique}.{extension}"));

    tokio::fs::copy(&source, &dest).await?;

    // Best-effort cleanup of this stem's earlier copies (background-*/
    // icon-*) so repeatedly changing the image doesn't pile up files in the
    // config dir forever. A cleanup failure here must never fail the
    // command — the new image is already safely in place either way.
    if let Ok(mut entries) = tokio::fs::read_dir(&studio_dir).await {
        let prefix = format!("{dest_stem}-");
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path == dest {
                continue;
            }
            let is_old_copy = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix));
            if is_old_copy {
                let _ = tokio::fs::remove_file(&path).await;
            }
        }
    }

    Ok(dest.to_string_lossy().into_owned())
}

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];

fn is_image_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "webm", "mov", "m4v"];

fn is_video_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// A fresh, empty `$APPCONFIG/studio/<root_name>/<unique>/` directory, with
/// every previous batch *under that same root* removed first. Each pick
/// fully replaces its pool (there's no "add one more" — the frontend always
/// re-sends the whole set), so nothing here should accumulate across repeated
/// picks the way a single overwritten file naturally would. `root_name` scopes
/// this per pool type (`"backgrounds"` for images, `"background-videos"` for
/// video) — using separate roots, rather than one shared folder for both,
/// matters because switching from Image to Video mode and picking a video
/// must never delete the previously-picked *image* pool's files (or vice
/// versa): each pool needs to survive being temporarily not the active mode.
async fn fresh_batch_dir<R: Runtime>(
    app: &tauri::AppHandle<R>,
    root_name: &str,
) -> Result<std::path::PathBuf> {
    let config_dir = app.path().app_config_dir()?;
    let root = config_dir.join("studio").join(root_name);
    if root.exists() {
        // Best-effort: failing to clean up old batches must never block
        // setting the new one.
        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let batch_dir = root.join(unique.to_string());
    tokio::fs::create_dir_all(&batch_dir).await?;
    Ok(batch_dir)
}

async fn fresh_backgrounds_batch_dir<R: Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<std::path::PathBuf> {
    fresh_batch_dir(app, "backgrounds").await
}

async fn fresh_video_batch_dir<R: Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<std::path::PathBuf> {
    fresh_batch_dir(app, "background-videos").await
}

async fn copy_images_into(
    batch_dir: &std::path::Path,
    sources: &[std::path::PathBuf],
) -> Result<Vec<String>> {
    let mut dest_paths = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let extension = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png");
        let dest = batch_dir.join(format!("{index}.{extension}"));
        tokio::fs::copy(source, &dest).await?;
        dest_paths.push(dest.to_string_lossy().into_owned());
    }
    Ok(dest_paths)
}

/// Copies one or more user-picked image files into a fresh
/// `$APPCONFIG/studio/backgrounds/` batch and returns their destination
/// paths, so the frontend can reference them with `convertFileSrc` — the
/// webview can't load arbitrary filesystem paths directly, only ones inside
/// the asset-protocol scope declared in `apps/app/tauri.conf.json`. Any
/// non-image path is silently skipped rather than failing the whole pick,
/// since the file dialog's own extension filter should already keep these
/// out in normal use.
#[tauri::command]
pub async fn studio_set_background_images<R: Runtime>(
    app: tauri::AppHandle<R>,
    source_paths: Vec<String>,
) -> Result<Vec<String>> {
    let sources: Vec<std::path::PathBuf> = source_paths
        .into_iter()
        .map(std::path::PathBuf::from)
        .filter(|path| is_image_file(path))
        .collect();
    let batch_dir = fresh_backgrounds_batch_dir(&app).await?;
    copy_images_into(&batch_dir, &sources).await
}

/// Same idea as `studio_set_background_images`, but for every image file
/// found directly inside a user-picked folder (not recursive — subfolders
/// aren't descended into). Returns an empty list if the folder has no
/// recognized image files; the frontend treats that as "nothing to show" and
/// leaves the previous background pool alone rather than clearing it.
#[tauri::command]
pub async fn studio_set_background_folder<R: Runtime>(
    app: tauri::AppHandle<R>,
    folder_path: String,
) -> Result<Vec<String>> {
    let folder = std::path::PathBuf::from(&folder_path);
    let mut sources = Vec::new();
    let mut entries = tokio::fs::read_dir(&folder).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() && is_image_file(&path) {
            sources.push(path);
        }
    }
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    // Stable, predictable order for "loop" mode — otherwise this would follow
    // whatever arbitrary order the OS's directory listing happens to return.
    sources.sort();

    let batch_dir = fresh_backgrounds_batch_dir(&app).await?;
    copy_images_into(&batch_dir, &sources).await
}

/// Same idea as `studio_set_background_images`, but for one or more
/// user-picked video files, copied into their own
/// `$APPCONFIG/studio/background-videos/` batch (kept entirely separate from
/// the image pool's folder — see `fresh_batch_dir`'s doc comment for why).
#[tauri::command]
pub async fn studio_set_background_videos<R: Runtime>(
    app: tauri::AppHandle<R>,
    source_paths: Vec<String>,
) -> Result<Vec<String>> {
    let sources: Vec<std::path::PathBuf> = source_paths
        .into_iter()
        .map(std::path::PathBuf::from)
        .filter(|path| is_video_file(path))
        .collect();
    let batch_dir = fresh_video_batch_dir(&app).await?;
    copy_images_into(&batch_dir, &sources).await
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

/// Copies a user-picked image into `$APPCONFIG/studio/` to use as the startup
/// (splash screen) background, in place of the default cube artwork. Same
/// copy-into-config-dir reasoning as `studio_set_app_icon`: the webview can
/// only load images from inside the asset-protocol scope, and the originally
/// picked file could later be moved or deleted.
#[tauri::command]
pub async fn studio_set_splash_background<R: Runtime>(
    app: tauri::AppHandle<R>,
    source_path: String,
) -> Result<String> {
    copy_into_studio_dir(&app, &source_path, "splash-background").await
}
