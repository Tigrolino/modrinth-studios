//! Modrinth Studios addition: cross-platform helpers for linking one path to
//! another, used by the shared Minecraft folder feature (see
//! `crate::shared_profile`) to make an instance's `saves`/`config`/
//! `resourcepacks` folders and `options.txt` file actually *be* the shared
//! profile's copies, rather than separate copies that would need to be kept
//! in sync.
//!
//! - Directories use an NTFS junction on Windows (via the `junction` crate)
//!   or a symlink everywhere else. A junction was chosen over a real Windows
//!   directory symlink specifically because symlinks need
//!   `SeCreateSymbolicLinkPrivilege` — granted to admins, or when Developer
//!   Mode is on — while junctions work for any user on the same machine, no
//!   special privilege required. Both are "reparse points" as far as Windows
//!   and Rust's `std::fs` are concerned, so the rest of the app (the storage
//!   walk, the file watcher, Explorer, Minecraft itself) sees an ordinary
//!   folder either way.
//! - The single `options.txt` file uses a hard link, both on Windows and
//!   elsewhere — no admin/dev-mode requirement on any platform, and edits to
//!   either path show up in the other immediately since they're the same
//!   file on disk under the hood.
//!
//! Every function here only ever touches the *link*, never the data on the
//! other end of it — removing a link never deletes the shared content, only
//! the local reparse point/hard link pointing at it.

use std::path::{Path, PathBuf};

/// True if `path` is currently a link created by this module (a directory
/// junction/symlink, or a hard-linked file) rather than a real, private
/// folder/file — used to decide whether an instance is already sharing
/// before linking/unlinking it.
pub async fn is_link(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref().to_path_buf();
    tokio::task::spawn_blocking(move || is_link_sync(&path))
        .await
        .unwrap_or(false)
}

fn is_link_sync(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() {
        return true;
    }
    // Windows note: `is_symlink()` already covers junctions on current Rust
    // (both are surfaced as reparse points), but check the raw attribute too
    // as a defensive fallback in case that ever changes — cheap and can only
    // ever turn a false negative into a correct `true`.
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// Resolves what a directory link (junction on Windows, symlink elsewhere)
/// at `path` currently points to. Only meaningful for the directory links
/// this module creates — a hard-linked file (options.txt/servers.dat) has no
/// separate "target" to read, since both paths just name the same
/// underlying file; callers never need this for those.
pub async fn read_link_target(path: impl AsRef<Path>) -> crate::Result<PathBuf> {
    let path = path.as_ref();
    tokio::fs::read_link(path).await.map_err(|e| {
        crate::ErrorKind::FSError(format!(
            "Failed to resolve shared folder link at {}: {e}",
            path.display()
        ))
        .as_error()
    })
}

/// Creates `link_path` as a link to the directory `target` (which must
/// already exist). `link_path` must not already exist.
pub async fn create_dir_link(
    target: impl AsRef<Path>,
    link_path: impl AsRef<Path>,
) -> crate::Result<()> {
    let target = target.as_ref().to_path_buf();
    let link_path = link_path.as_ref().to_path_buf();
    tokio::task::spawn_blocking(move || create_dir_link_sync(&target, &link_path))
        .await
        .map_err(|e| {
            crate::ErrorKind::OtherError(format!(
                "shared folder link task panicked: {e}"
            ))
            .as_error()
        })??;
    Ok(())
}

#[cfg(windows)]
fn create_dir_link_sync(target: &Path, link_path: &Path) -> crate::Result<()> {
    junction::create(target, link_path).map_err(|e| {
        crate::ErrorKind::FSError(format!(
            "Failed to create shared folder junction at {}: {e}",
            link_path.display()
        ))
        .as_error()
    })
}

#[cfg(not(windows))]
fn create_dir_link_sync(target: &Path, link_path: &Path) -> crate::Result<()> {
    std::os::unix::fs::symlink(target, link_path).map_err(|e| {
        crate::ErrorKind::FSError(format!(
            "Failed to create shared folder symlink at {}: {e}",
            link_path.display()
        ))
        .as_error()
    })
}

/// Creates `link_path` as a hard link to the file `target` (which must
/// already exist). `link_path` must not already exist. Both paths need to be
/// on the same filesystem/volume — always true here, since shared profiles
/// live under the same app data directory as every instance.
pub async fn create_file_link(
    target: impl AsRef<Path>,
    link_path: impl AsRef<Path>,
) -> crate::Result<()> {
    let target = target.as_ref().to_path_buf();
    let link_path = link_path.as_ref().to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::fs::hard_link(&target, &link_path).map_err(|e| {
            crate::ErrorKind::FSError(format!(
                "Failed to create shared folder hard link at {}: {e}",
                link_path.display()
            ))
            .as_error()
        })
    })
    .await
    .map_err(|e| {
        crate::ErrorKind::OtherError(format!(
            "shared folder link task panicked: {e}"
        ))
        .as_error()
    })??;
    Ok(())
}

/// Removes a link created by `create_dir_link`/`create_file_link` at `path`
/// — only the link/reparse point/hard link entry itself, never the shared
/// data on the other end of it. Does nothing (returns `Ok`) if `path`
/// doesn't exist.
pub async fn remove_link(path: impl AsRef<Path>) -> crate::Result<()> {
    let path = path.as_ref().to_path_buf();
    tokio::task::spawn_blocking(move || remove_link_sync(&path))
        .await
        .map_err(|e| {
            crate::ErrorKind::OtherError(format!(
                "shared folder unlink task panicked: {e}"
            ))
            .as_error()
        })??;
    Ok(())
}

fn remove_link_sync(path: &Path) -> crate::Result<()> {
    if std::fs::symlink_metadata(path).is_err() {
        return Ok(());
    }

    let is_dir_link = {
        #[cfg(windows)]
        {
            // A Windows junction reports as a directory (unlike a Unix
            // symlink-to-dir, which reports as a symlink) — `remove_dir`
            // removes just the reparse point without touching its target's
            // contents, which is exactly what `std::fs::RemoveDirectoryW`
            // does for a reparse point.
            path.is_dir()
        }
        #[cfg(not(windows))]
        {
            false
        }
    };

    let result = if is_dir_link {
        std::fs::remove_dir(path)
    } else {
        std::fs::remove_file(path)
    };

    result.map_err(|e| {
        crate::ErrorKind::FSError(format!(
            "Failed to remove shared folder link at {}: {e}",
            path.display()
        ))
        .as_error()
    })
}
