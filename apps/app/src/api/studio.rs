//! Modrinth Studios addition: small grab-bag of app-only commands specific
//! to this fork — mostly cosmetic/runtime (backgrounds, icons), plus the
//! occasional thin Tauri wrapper (like the Storage page's usage command)
//! for real `theseus` (app-lib) functionality that's fork-only and
//! therefore doesn't belong in an upstream-shared API file. New plugin
//! file, doesn't touch any upstream module.

use crate::api::Result;
use tauri::{Manager, Runtime};

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("studio")
        .invoke_handler(tauri::generate_handler![
            studio_set_background_images,
            studio_set_background_folder,
            studio_set_background_videos,
            studio_set_background_video_folder,
            studio_set_app_icon,
            studio_set_splash_background,
            studio_set_generated_app_icon,
            studio_apply_pinned_icon,
            studio_instance_storage_usage,
            studio_instance_storage_usage_single,
            studio_system_storage_overview
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

/// Same idea as `studio_set_background_folder`, but for every video file
/// found directly inside a user-picked folder (not recursive), copied into
/// the video pool's own batch dir. Mirrors `studio_set_background_folder`'s
/// "Choose folder..." convenience for the Video tab, which was previously
/// missing (Video only had "Choose video(s)..." — picking every video out of
/// a folder one at a time felt off compared to the Image tab).
#[tauri::command]
pub async fn studio_set_background_video_folder<R: Runtime>(
    app: tauri::AppHandle<R>,
    folder_path: String,
) -> Result<Vec<String>> {
    let folder = std::path::PathBuf::from(&folder_path);
    let mut sources = Vec::new();
    let mut entries = tokio::fs::read_dir(&folder).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() && is_video_file(&path) {
            sources.push(path);
        }
    }
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    sources.sort();

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

/// Writes a runtime-generated icon (the app's own ring mark recolored to the
/// current accent color — see `applyAccentIconTint()` in
/// `use-studio-appearance.ts`) to `$APPCONFIG/studio/` so it can be applied
/// as the running window/taskbar icon via `setIcon()`.
///
/// Modrinth Studios addition: this used to always write the exact same
/// filename (`generated-icon.png`), overwritten in place on every accent
/// change. That's the same class of bug `copy_into_studio_dir` documents
/// above (Windows/the webview can cache an icon by its file path, not just
/// its bytes) — and on Windows it explains a real symptom: the tinted icon
/// shows correctly in the title bar, alt-tab, and Task Manager (which all
/// query the live window icon) but the taskbar button itself can keep
/// showing a stale icon, because taskbar icon resolution is documented to
/// cache more aggressively and by path. Giving each write a fresh filename
/// (mirroring `copy_into_studio_dir`'s pattern, including its best-effort
/// cleanup of earlier copies) guarantees there's nothing stale at the old
/// path for anything to keep serving.
#[tauri::command]
pub async fn studio_set_generated_app_icon<R: Runtime>(
    app: tauri::AppHandle<R>,
    bytes: Vec<u8>,
) -> Result<String> {
    let config_dir = app.path().app_config_dir()?;
    let studio_dir = config_dir.join("studio");
    tokio::fs::create_dir_all(&studio_dir).await?;

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dest = studio_dir.join(format!("generated-icon-{unique}.png"));
    tokio::fs::write(&dest, &bytes).await?;

    if let Ok(mut entries) = tokio::fs::read_dir(&studio_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path == dest {
                continue;
            }
            let is_old_copy = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("generated-icon-"));
            if is_old_copy {
                let _ = tokio::fs::remove_file(&path).await;
            }
        }
    }

    Ok(dest.to_string_lossy().into_owned())
}

/// Modrinth Studios addition: applies `bytes` (a `.ico` file, see
/// `wrapPngAsIco()` in `use-studio-appearance.ts`) as the icon for any
/// taskbar-pinned or Start-menu shortcut pointing at this app, by editing
/// the shortcut file(s) directly — see `studio_pinned_icon_windows.rs` for
/// why that's needed (a pinned shortcut's icon is independent of whatever
/// the running window sets for itself) and why it's safe (it never touches
/// the installed exe, only the person's own shortcut files). No-op on
/// platforms other than Windows, where this class of problem doesn't exist.
#[tauri::command]
pub async fn studio_apply_pinned_icon<R: Runtime>(
    app: tauri::AppHandle<R>,
    bytes: Vec<u8>,
) -> Result<String> {
    #[cfg(windows)]
    {
        let config_dir = app.path().app_config_dir()?;
        let studio_dir = config_dir.join("studio");
        let summary = crate::api::studio_pinned_icon_windows::apply_pinned_icon(&studio_dir, bytes)?;
        Ok(summary)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, bytes);
        Ok("not windows, no-op".to_string())
    }
}

/// Modrinth Studios addition: backs the Settings > Storage page — see
/// `theseus::instance::instance_storage_usage()` for the actual disk-usage
/// walk. Thin Tauri wrapper only; kept here rather than in the upstream
/// `api/instance.rs` so this fork-only feature doesn't touch a file shared
/// with upstream.
#[tauri::command]
pub async fn studio_instance_storage_usage()
-> Result<Vec<theseus::instance::InstanceStorageUsage>> {
    Ok(theseus::instance::instance_storage_usage().await?)
}

/// Modrinth Studios addition: backs the instance page header's "storage next
/// to playtime" display (behind the toggle on Settings > Storage) — walks
/// only the one instance asked for, instead of every instance like
/// `studio_instance_storage_usage` above, so opening an instance page never
/// pays for measuring unrelated instances. See
/// `theseus::instance::instance_storage_usage_single()`.
#[tauri::command]
pub async fn studio_instance_storage_usage_single(
    instance_id: String,
) -> Result<Option<theseus::instance::InstanceStorageUsage>> {
    Ok(theseus::instance::instance_storage_usage_single(&instance_id).await?)
}

/// Modrinth Studios addition: backs the Storage page's Steam-style overview
/// bar at the top — see `theseus::instance::system_storage_overview()` for
/// how the segments are actually computed.
#[tauri::command]
pub async fn studio_system_storage_overview()
-> Result<theseus::instance::SystemStorageOverview> {
    Ok(theseus::instance::system_storage_overview().await?)
}
