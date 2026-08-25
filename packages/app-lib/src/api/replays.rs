//! Modrinth Studios addition: support for the ReplayMod / Flashback replay
//! recording mods.
//!
//! Neither mod publishes a stable, versioned file format spec, so everything
//! here is intentionally lenient: we read whatever fields happen to be in
//! the zip's metadata JSON and fall back to filesystem info (size, mtime,
//! filename) for anything we can't find. This is a brand-new module (no
//! upstream Modrinth code touched) so it should never conflict with future
//! `git merge`s from upstream.

use crate::util::io;
use crate::{Result, ErrorKind};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReplayKind {
    ReplayMod,
    Flashback,
}

impl ReplayKind {
    /// Folder (relative to the instance root) each mod stores its
    /// recordings in.
    pub fn relative_folder(self) -> &'static str {
        match self {
            ReplayKind::ReplayMod => "replay_recordings",
            ReplayKind::Flashback => "flashback/replays",
        }
    }

    fn folder(self, instance: &Path) -> PathBuf {
        instance.join(self.relative_folder())
    }

    fn all() -> [ReplayKind; 2] {
        [ReplayKind::ReplayMod, ReplayKind::Flashback]
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Replay {
    pub kind: ReplayKind,
    /// Folder the replay lives in, relative to the instance root (e.g.
    /// `replay_recordings` or `flashback/replays`). Lets the frontend build
    /// an absolute path (via the instance's already-known full path) without
    /// having to duplicate the kind -> folder mapping.
    pub folder: String,
    pub file_name: String,
    pub name: String,
    pub size: u64,
    /// Unix seconds, from the file's last-modified time.
    pub modified: i64,
    pub duration_ms: Option<u64>,
    /// Unix seconds, from the replay's own metadata if it has one.
    pub recorded_at: Option<i64>,
    pub minecraft_version: Option<String>,
    pub server_name: Option<String>,
    pub singleplayer: Option<bool>,
}

/// Cheap existence check so the frontend can decide whether to show the
/// "Replays" tab at all without paying for full metadata parsing.
pub async fn has_replays(instance: &Path) -> Result<bool> {
    for kind in ReplayKind::all() {
        let folder = kind.folder(instance);
        if let Ok(mut dir) = io::read_dir(&folder).await {
            if let Ok(Some(_)) = dir.next_entry().await {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub async fn list_replays(instance: &Path) -> Result<Vec<Replay>> {
    let mut replays = Vec::new();

    for kind in ReplayKind::all() {
        let folder = kind.folder(instance);
        let Ok(mut dir) = io::read_dir(&folder).await else {
            continue;
        };

        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();

            let Ok(file_type) = entry.file_type().await else {
                continue;
            };
            // ReplayMod occasionally leaves a `.mcpr.crash` folder behind
            // for recordings that never finished writing; skip directories.
            if file_type.is_dir() {
                continue;
            }

            let Some(file_name) = path.file_name().and_then(|f| f.to_str())
            else {
                continue;
            };
            if file_name.ends_with(".crc32") || file_name.ends_with(".lock") {
                continue;
            }

            let Ok(fs_metadata) = entry.metadata().await else {
                continue;
            };
            let modified = fs_metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let mut replay = Replay {
                kind,
                folder: kind.relative_folder().to_string(),
                file_name: file_name.to_string(),
                name: pretty_name(file_name),
                size: fs_metadata.len(),
                modified,
                duration_ms: None,
                recorded_at: None,
                minecraft_version: None,
                server_name: None,
                singleplayer: None,
            };

            if let Ok(bytes) = read_zip_metadata_json(&path) {
                apply_metadata_json(&mut replay, &bytes);
            }

            replays.push(replay);
        }
    }

    replays.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(replays)
}

fn pretty_name(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);
    let stem = stem.strip_suffix("-flashback").unwrap_or(stem);
    stem.replace(['_', '-'], " ")
}

/// Both ReplayMod (`.mcpr`) and Flashback recordings are zip archives with a
/// `metaData.json` / `metadata.json` entry at the root.
fn read_zip_metadata_json(path: &Path) -> std::io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    })?;
    for name in ["metaData.json", "metadata.json"] {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "no metadata entry in replay archive",
    ))
}

fn apply_metadata_json(replay: &mut Replay, bytes: &[u8]) {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return;
    };

    if let Some(duration) = value.get("duration").and_then(|v| v.as_u64()) {
        replay.duration_ms = Some(duration);
    }
    if let Some(date) = value.get("date").and_then(|v| v.as_i64()) {
        // ReplayMod stores milliseconds since epoch.
        replay.recorded_at =
            Some(if date > 10_000_000_000 { date / 1000 } else { date });
    }
    if let Some(version) = value
        .get("mcversion")
        .or_else(|| value.get("gameVersion"))
        .and_then(|v| v.as_str())
    {
        replay.minecraft_version = Some(version.to_string());
    }
    if let Some(server_name) = value
        .get("serverName")
        .or_else(|| value.get("customServerName"))
        .and_then(|v| v.as_str())
    {
        if !server_name.is_empty() {
            replay.server_name = Some(server_name.to_string());
        }
    }
    if let Some(singleplayer) =
        value.get("singleplayer").and_then(|v| v.as_bool())
    {
        replay.singleplayer = Some(singleplayer);
    }
    if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
        if !name.is_empty() {
            replay.name = name.to_string();
        }
    }
}

pub async fn delete_replay(
    instance: &Path,
    kind: ReplayKind,
    file_name: &str,
) -> Result<()> {
    let path = safe_join(&kind.folder(instance), file_name)?;
    io::remove_file(path).await?;
    Ok(())
}

pub async fn rename_replay(
    instance: &Path,
    kind: ReplayKind,
    file_name: &str,
    new_file_name: &str,
) -> Result<()> {
    let folder = kind.folder(instance);
    let from = safe_join(&folder, file_name)?;
    let to = safe_join(&folder, new_file_name)?;
    io::rename_or_move(from, to).await?;
    Ok(())
}

/// Copies an externally-picked replay file into the instance. The target
/// folder is inferred from the extension (`.mcpr` -> ReplayMod, anything
/// else -> Flashback) since that's the only reliable signal we have.
pub async fn import_replay(
    instance: &Path,
    source_path: &str,
) -> Result<()> {
    let source = PathBuf::from(source_path);
    let Some(file_name) = source.file_name().and_then(|f| f.to_str()) else {
        return Err(ErrorKind::InputError(
            "Invalid replay file path".to_string(),
        )
        .into());
    };

    let kind = if file_name.to_ascii_lowercase().ends_with(".mcpr") {
        ReplayKind::ReplayMod
    } else {
        ReplayKind::Flashback
    };

    let folder = kind.folder(instance);
    io::create_dir_all(&folder).await?;
    let dest = folder.join(file_name);
    io::copy(&source, &dest).await?;
    Ok(())
}

pub fn replay_path(
    instance: &Path,
    kind: ReplayKind,
    file_name: &str,
) -> Result<PathBuf> {
    safe_join(&kind.folder(instance), file_name)
}

pub fn replay_folder(instance: &Path, kind: ReplayKind) -> PathBuf {
    kind.folder(instance)
}

fn safe_join(dir: &Path, file_name: &str) -> Result<PathBuf> {
    if file_name.is_empty()
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name == ".."
    {
        return Err(
            ErrorKind::InputError("Invalid replay file name".to_string())
                .into(),
        );
    }
    Ok(dir.join(file_name))
}
