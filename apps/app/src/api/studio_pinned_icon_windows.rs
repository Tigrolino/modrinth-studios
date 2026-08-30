//! Modrinth Studios addition: Windows-only half of `studio_apply_pinned_icon`
//! (see `studio.rs`). A shortcut pinned to the taskbar (or Start menu) shows
//! *its own* icon — set via the shortcut file's `IconLocation` property —
//! completely independent of whatever the running window sets for itself at
//! runtime with `setIcon()`. That's why the accent-tinted default icon (see
//! `applyAccentIconTint()` in `use-studio-appearance.ts`) kept reverting to
//! the exe's built-in icon specifically once pinned: nothing was ever
//! telling the *shortcut* about it.
//!
//! This writes the given icon to disk as a real `.ico` file, then finds any
//! `.lnk` shortcut in the two folders Windows itself uses for taskbar pins
//! and Start-menu entries, and — for any that look like they belong to this
//! app — points its `IconLocation` at that file, using the same
//! `IShellLinkW`/`IPersistFile` COM APIs Windows uses internally to create
//! shortcuts in the first place. This never touches the installed `.exe` in
//! any way; only the person's own, freely-editable shortcut files.
//!
//! A shortcut only gets touched if its target actually resolves to *this
//! exact running exe* (`std::env::current_exe()`) — checked by searching the
//! `.lnk`'s raw bytes for the exe's path, both as UTF-16LE (how modern LNKs
//! store it) and as plain ASCII (the legacy `LinkInfo.LocalBasePath`
//! encoding), case-insensitively. This is a cheap, COM-free way to read a
//! `.lnk`'s target without pinning down `IShellLinkW::GetPath`'s exact
//! signature. It matters more than it might sound: a dev build
//! (`target/debug/...exe`) and a separately-installed release build are
//! *different exes*, and matching on anything looser (e.g. a filename
//! containing "modrinth" — an earlier version of this code did exactly that,
//! and it's *also* wrong in the other direction: Windows names a shortcut it
//! auto-generates from "Pin to taskbar" after the exe's own version-info
//! metadata, which for a dev/debug build may not contain "modrinth" at all,
//! silently skipping the one shortcut we actually needed to touch) would
//! either grab the wrong install's shortcut or miss the right one. The
//! target check is the only thing this relies on now. Every failure mode
//! here is still just "the icon doesn't update," never anything
//! destructive.
//!
//! Returns a short plain-text summary of what it found/did (folders
//! scanned, `.lnk` count, how many matched, how many were actually updated)
//! so a failure can be diagnosed from the frontend's console log instead of
//! guessing blind — see `applyAccentIconTint()` in `use-studio-appearance.ts`.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
    IPersistFile, STGM_READWRITE,
};
use windows::Win32::UI::Shell::{
    IShellLinkW, SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify, ShellLink,
};
use windows::core::{Interface, PCWSTR};

use crate::api::Result;

fn to_wide_null(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// The two Windows-managed, per-user folders whose `.lnk` files can affect
/// the icon someone sees for this app outside of the running window itself:
/// a taskbar pin, and a Start-menu entry (which uses the same icon
/// resolution as Start-menu search/tiles).
fn candidate_shortcut_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let appdata = PathBuf::from(appdata);
        dirs.push(
            appdata
                .join("Microsoft")
                .join("Internet Explorer")
                .join("Quick Launch")
                .join("User Pinned")
                .join("TaskBar"),
        );
        dirs.push(
            appdata
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    dirs
}

fn contains_bytes_ci(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && needle.len() <= haystack.len()
        && haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

/// Whether `lnk_path`'s target is (very likely) `exe_path` — see the module
/// doc comment for why this reads raw bytes instead of using
/// `IShellLinkW::GetPath`.
fn shortcut_targets_exe(lnk_path: &Path, exe_path: &Path) -> bool {
    let Ok(raw) = std::fs::read(lnk_path) else {
        return false;
    };
    let exe_str = exe_path.to_string_lossy();

    let utf16le_needle: Vec<u8> = exe_str
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    let ansi_needle: Vec<u8> = exe_str.bytes().collect();

    contains_bytes_ci(&raw, &utf16le_needle) || contains_bytes_ci(&raw, &ansi_needle)
}

/// Points `lnk_path`'s `IconLocation` at `icon_path` and reports whether it
/// actually got all the way through. Every failure here just quietly gives
/// up on that one shortcut — there's nothing destructive to roll back,
/// since this only ever changes an icon reference, never the shortcut's
/// target or anything else about it.
unsafe fn set_shortcut_icon(lnk_path: &Path, icon_path: &Path) -> std::result::Result<(), &'static str> {
    let shell_link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
        .map_err(|_| "CoCreateInstance(ShellLink) failed")?;
    let persist_file: IPersistFile = shell_link
        .cast()
        .map_err(|_| "cast to IPersistFile failed")?;

    let lnk_wide = to_wide_null(lnk_path.as_os_str());
    unsafe { persist_file.Load(PCWSTR(lnk_wide.as_ptr()), STGM_READWRITE) }
        .map_err(|_| "IPersistFile::Load failed")?;

    let icon_wide = to_wide_null(icon_path.as_os_str());
    unsafe { shell_link.SetIconLocation(PCWSTR(icon_wide.as_ptr()), 0) }
        .map_err(|_| "IShellLinkW::SetIconLocation failed")?;

    unsafe { persist_file.Save(PCWSTR(lnk_wide.as_ptr()), true) }
        .map_err(|_| "IPersistFile::Save failed")?;

    Ok(())
}

pub fn apply_pinned_icon(studio_dir: &Path, bytes: Vec<u8>) -> Result<String> {
    std::fs::create_dir_all(studio_dir)?;

    // Modrinth Studios addition: same reasoning as `generated-icon-*.png` in
    // studio.rs — a fixed filename (`pinned-icon.ico`), overwritten in
    // place, risks Windows' own shell icon cache (documented to key at
    // least partly on path, not just bytes) serving a stale bitmap for the
    // *shortcut's* icon too, on top of the taskbar-button caching this
    // module already exists to work around. A fresh filename per write,
    // with best-effort cleanup of earlier ones, closes that off the same
    // way.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let icon_path = studio_dir.join(format!("pinned-icon-{unique}.ico"));
    std::fs::write(&icon_path, &bytes)?;

    // If this can't be resolved, there's nothing safe to match against.
    let Ok(exe_path) = std::env::current_exe() else {
        return Ok(format!(
            "wrote {} but couldn't resolve current_exe(), so no shortcut was checked",
            icon_path.display()
        ));
    };

    // Safe to call even if COM is already initialized on this thread (e.g.
    // by WebView2) — a mismatched/redundant init or uninit here can't do
    // anything worse than this specific operation silently not applying.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

    let mut lnk_count = 0u32;
    let mut matched_count = 0u32;
    let mut updated_count = 0u32;
    let mut last_error: Option<&'static str> = None;

    for dir in candidate_shortcut_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_lnk = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"));
            if !is_lnk {
                continue;
            }
            lnk_count += 1;
            if !shortcut_targets_exe(&path, &exe_path) {
                continue;
            }
            matched_count += 1;
            match unsafe { set_shortcut_icon(&path, &icon_path) } {
                Ok(()) => updated_count += 1,
                Err(e) => last_error = Some(e),
            }
        }
    }

    // Modrinth Studios addition: SetIconLocation + Save above already
    // succeeded at this point (that's what `updated_count` reflects) — the
    // shortcut file on disk is correct. But Explorer/the taskbar cache
    // shell icons more aggressively than they re-read shortcut files, and
    // rewriting a .lnk's bytes on disk doesn't by itself guarantee a
    // repaint. SHChangeNotify(SHCNE_ASSOCCHANGED) is the standard Shell API
    // for telling Explorer "an item's association/icon changed, refresh
    // your icon cache" — a notification only, doesn't touch any files, so
    // there's nothing to go wrong beyond it simply not helping.
    if updated_count > 0 {
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
    }

    unsafe { CoUninitialize() };

    // Cleanup runs last and deliberately after every shortcut update has
    // already been attempted: if a COM call above failed partway through
    // for a shortcut still pointing at an *older* icon file, deleting that
    // older file first would turn "stale icon" into "broken icon" for it.
    if let Ok(entries) = std::fs::read_dir(studio_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path == icon_path {
                continue;
            }
            let is_old_copy = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("pinned-icon-"));
            if is_old_copy {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    Ok(format!(
        "exe={} icon={} lnk_scanned={lnk_count} target_matched={matched_count} icon_updated={updated_count}{}",
        exe_path.display(),
        icon_path.display(),
        last_error.map(|e| format!(" last_error={e}")).unwrap_or_default(),
    ))
}
