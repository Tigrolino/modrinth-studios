//! Modrinth Studios addition: support for the ReplayMod / Flashback replay
//! recording mods.
//!
//! Neither mod publishes a stable, versioned file format spec, so everything
//! here is intentionally lenient: we read whatever fields happen to be in
//! the zip's metadata JSON and fall back to filesystem info (size, mtime,
//! filename) for anything we can't find. This is a brand-new module (no
//! upstream Modrinth code touched) so it should never conflict with future
//! `git merge`s from upstream.

use crate::state::State;
use crate::util::io;
use crate::{Result, ErrorKind};
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

// Modrinth Studios addition: see the comment where this is used, in
// `list_replays` below.
const REPLAY_METADATA_READ_CONCURRENCY: usize = 16;

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

    // Modrinth Studios addition: a stable string key for
    // `studio_replay_metadata_cache` — kept separate from `serde`'s
    // `snake_case` rename (which would also work, "replay_mod"/"flashback")
    // so the DB's on-disk format never silently changes just because a
    // `#[serde(...)]` attribute above changes for some unrelated (e.g. JSON
    // wire format) reason.
    fn db_key(self) -> &'static str {
        match self {
            ReplayKind::ReplayMod => "replay_mod",
            ReplayKind::Flashback => "flashback",
        }
    }

    fn from_db_key(value: &str) -> Option<Self> {
        match value {
            "replay_mod" => Some(ReplayKind::ReplayMod),
            "flashback" => Some(ReplayKind::Flashback),
            _ => None,
        }
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
    /// Flashback-only. Flashback's `metadata.json` (see its `FlashbackMeta`
    /// class) has no `serverName`/`singleplayer` fields at all — it never
    /// records that distinction — so guessing one from the other fields used
    /// to be flat-out wrong. `world_name` is the actual field it does write
    /// (the recorded world/server's display name), so surface that verbatim
    /// instead of pretending we know singleplayer/multiplayer status.
    pub world_name: Option<String>,
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

pub async fn list_replays(
    instance: &Path,
    instance_id: &str,
) -> Result<Vec<Replay>> {
    // Three-phase: first collect cheap filesystem info for every replay
    // (fast, just directory listing + stat calls), then check
    // `studio_replay_metadata_cache` for each one (one indexed SELECT,
    // regardless of how many replays there are), and only for whatever's
    // left — new replays, or ones whose size/mtime changed since they were
    // last cached — open its zip archive and read the embedded metadata
    // JSON. That last part is slow enough (a few ms to tens of ms each) that
    // doing it for every replay on every single visit to this tab (which is
    // what used to happen — there was no cache at all) made this tab take a
    // very long time for anyone with hundreds/thousands of replays, the same
    // problem the Screenshots tab already solved for itself by keeping an
    // on-disk index instead of re-scanning from scratch every time (see
    // `reconcile_source_screenshots`). `read_zip_metadata_json` is
    // synchronous/blocking, so each one still gets farmed out to tokio's
    // blocking thread pool via `spawn_blocking`, with a bounded number
    // running concurrently at once (see `REPLAY_METADATA_READ_CONCURRENCY`
    // below) rather than either waiting on each other one at a time or all
    // racing at once with no limit — this part is unchanged, it just now
    // runs on a much smaller list most of the time.
    let mut entries = Vec::new();

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

            let replay = Replay {
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
                world_name: None,
            };

            entries.push((path, replay));
        }
    }

    // Modrinth Studios addition: check the cache before opening anything.
    // A hit (same file_size/modified_at as what's on disk right now) means
    // this replay's zip doesn't need to be touched at all this time.
    let state = State::get().await?;
    let cached = load_replay_metadata_cache(&state, instance_id).await?;

    let mut needs_parse = Vec::new();
    for (index, (_, replay)) in entries.iter_mut().enumerate() {
        let hit = cached.get(&(replay.kind, replay.file_name.clone())).filter(
            |cached_row| {
                cached_row.file_size == replay_size_as_i64(replay.size)
                    && cached_row.modified_at == replay.modified
            },
        );
        match hit {
            Some(cached_row) => apply_cached_metadata(replay, cached_row),
            None => needs_parse.push(index),
        }
    }

    // Modrinth Studios addition: cap how many replay archives get opened and
    // read for metadata at once, rather than firing one `spawn_blocking` per
    // replay and letting all of them race each other with no limit. That was
    // fine for a handful of replays, but with hundreds or (as reported)
    // ~1000, it meant opening and inflating ~1000 zip archives at the exact
    // same instant every single time this tab was opened — a burst of disk
    // I/O and CPU contention severe enough on its own to freeze the app for
    // several seconds, entirely on the backend, before any row on the
    // frontend ever gets a chance to render. Bounding concurrency means the
    // reads that *are* running can actually make progress instead of all
    // fighting over the same disk and CPU, while still running many in
    // parallel rather than one at a time. Now that the cache above already
    // filters this down to just new/changed replays, this bound mostly
    // matters again on the very first load (or after adding a big batch of
    // replays at once) rather than every single visit.
    // Collecting owned `PathBuf`s first (rather than mapping `entries.iter()`
    // directly into the `async move` below) sidesteps a real rustc
    // limitation: a closure that both borrows from an outer iterator *and*
    // returns a future capturing that borrow triggers "implementation of
    // `FnOnce` is not general enough" (an HRTB error) once combined with
    // `stream::iter`/`buffer_unordered` — the same reason the equivalent
    // scan in `screenshots/operations.rs` maps over `sources.into_iter()`
    // (owned values) rather than `sources.iter()`.
    let paths: Vec<(usize, PathBuf)> = needs_parse
        .iter()
        .map(|&index| (index, entries[index].0.clone()))
        .collect();

    let metadata_results: Vec<(usize, std::io::Result<Vec<u8>>)> = stream::iter(
        paths.into_iter().map(|(index, path)| async move {
            let result = tokio::task::spawn_blocking(move || {
                read_zip_metadata_json(&path)
            })
            .await
            .unwrap_or_else(|error| {
                Err(std::io::Error::other(format!(
                    "replay metadata read task panicked: {error}"
                )))
            });
            (index, result)
        }),
    )
    .buffer_unordered(REPLAY_METADATA_READ_CONCURRENCY)
    .collect()
    .await;

    for (index, result) in metadata_results {
        if let Ok(bytes) = result {
            apply_metadata_json(&mut entries[index].1, &bytes);
        }
    }

    if let Err(error) = write_replay_metadata_cache(
        &state,
        instance_id,
        &entries,
        &needs_parse,
        &cached,
    )
    .await
    {
        // Modrinth Studios addition: the cache is purely a speed
        // optimization — a write failure (e.g. a locked database at just
        // the wrong moment) shouldn't turn into a hard error for the whole
        // tab. Worst case, this list is fully re-derived again next time.
        tracing::warn!("Failed to update replay metadata cache: {error}");
    }

    let mut replays: Vec<Replay> =
        entries.into_iter().map(|(_, replay)| replay).collect();
    replays.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(replays)
}

fn replay_size_as_i64(size: u64) -> i64 {
    i64::try_from(size).unwrap_or(i64::MAX)
}

/// Modrinth Studios addition: one row of `studio_replay_metadata_cache`.
struct CachedReplayMetadata {
    file_size: i64,
    modified_at: i64,
    duration_ms: Option<i64>,
    recorded_at: Option<i64>,
    minecraft_version: Option<String>,
    server_name: Option<String>,
    singleplayer: Option<bool>,
    world_name: Option<String>,
    display_name: Option<String>,
}

fn apply_cached_metadata(replay: &mut Replay, cached: &CachedReplayMetadata) {
    replay.duration_ms = cached.duration_ms.map(|value| value as u64);
    replay.recorded_at = cached.recorded_at;
    replay.minecraft_version = cached.minecraft_version.clone();
    replay.server_name = cached.server_name.clone();
    replay.singleplayer = cached.singleplayer;
    replay.world_name = cached.world_name.clone();
    if let Some(display_name) = &cached.display_name {
        replay.name = display_name.clone();
    }
}

async fn load_replay_metadata_cache(
    state: &State,
    instance_id: &str,
) -> Result<HashMap<(ReplayKind, String), CachedReplayMetadata>> {
    let rows = sqlx::query(
        "SELECT kind, file_name, file_size, modified_at, duration_ms, recorded_at,
                minecraft_version, server_name, singleplayer, world_name, display_name
         FROM studio_replay_metadata_cache WHERE instance_id = ?",
    )
    .bind(instance_id)
    .fetch_all(&state.pool)
    .await?;

    let mut cache = HashMap::with_capacity(rows.len());
    for row in rows {
        let kind_key: String = row.try_get("kind")?;
        let Some(kind) = ReplayKind::from_db_key(&kind_key) else {
            continue;
        };
        let file_name: String = row.try_get("file_name")?;
        let singleplayer: Option<i64> = row.try_get("singleplayer")?;
        cache.insert(
            (kind, file_name),
            CachedReplayMetadata {
                file_size: row.try_get("file_size")?,
                modified_at: row.try_get("modified_at")?,
                duration_ms: row.try_get("duration_ms")?,
                recorded_at: row.try_get("recorded_at")?,
                minecraft_version: row.try_get("minecraft_version")?,
                server_name: row.try_get("server_name")?,
                singleplayer: singleplayer.map(|value| value != 0),
                world_name: row.try_get("world_name")?,
                display_name: row.try_get("display_name")?,
            },
        );
    }
    Ok(cache)
}

/// Writes a fresh cache row for every replay whose metadata was just
/// (re-)parsed, and prunes rows for replays that no longer exist on disk
/// (deleted, or renamed — a rename produces a new `file_name`, so the old
/// row would otherwise never get revisited by anything). No-ops without
/// touching the database at all when there's nothing to write or prune,
/// which is the common case once everything is already cached.
async fn write_replay_metadata_cache(
    state: &State,
    instance_id: &str,
    entries: &[(PathBuf, Replay)],
    needs_parse: &[usize],
    cached: &HashMap<(ReplayKind, String), CachedReplayMetadata>,
) -> Result<()> {
    let current_keys: HashSet<(ReplayKind, &str)> = entries
        .iter()
        .map(|(_, replay)| (replay.kind, replay.file_name.as_str()))
        .collect();
    let stale: Vec<(ReplayKind, &str)> = cached
        .keys()
        .map(|(kind, file_name)| (*kind, file_name.as_str()))
        .filter(|key| !current_keys.contains(key))
        .collect();

    if needs_parse.is_empty() && stale.is_empty() {
        return Ok(());
    }

    let mut tx = state.pool.begin().await?;
    let now = chrono::Utc::now().timestamp();

    for &index in needs_parse {
        let (_, replay) = &entries[index];
        sqlx::query(
            "INSERT INTO studio_replay_metadata_cache
                (instance_id, kind, file_name, file_size, modified_at, duration_ms,
                 recorded_at, minecraft_version, server_name, singleplayer, world_name,
                 display_name, cached_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(instance_id, kind, file_name) DO UPDATE SET
                file_size = excluded.file_size,
                modified_at = excluded.modified_at,
                duration_ms = excluded.duration_ms,
                recorded_at = excluded.recorded_at,
                minecraft_version = excluded.minecraft_version,
                server_name = excluded.server_name,
                singleplayer = excluded.singleplayer,
                world_name = excluded.world_name,
                display_name = excluded.display_name,
                cached_at = excluded.cached_at",
        )
        .bind(instance_id)
        .bind(replay.kind.db_key())
        .bind(&replay.file_name)
        .bind(replay_size_as_i64(replay.size))
        .bind(replay.modified)
        .bind(replay.duration_ms.map(|value| value as i64))
        .bind(replay.recorded_at)
        .bind(&replay.minecraft_version)
        .bind(&replay.server_name)
        .bind(replay.singleplayer.map(|value| value as i64))
        .bind(&replay.world_name)
        .bind(&replay.name)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    for (kind, file_name) in stale {
        sqlx::query(
            "DELETE FROM studio_replay_metadata_cache
             WHERE instance_id = ? AND kind = ? AND file_name = ?",
        )
        .bind(instance_id)
        .bind(kind.db_key())
        .bind(file_name)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
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

    // `duration`/`date`/`mcversion`/`serverName`/`customServerName`/
    // `singleplayer` below are ReplayMod-only — confirmed against
    // ReplayStudio's `ReplayMetaData.java` (the library ReplayMod itself is
    // built on). Flashback's `metadata.json` (confirmed against its own
    // `FlashbackMeta.java`) never has any of these; it uses `world_name`
    // instead (handled separately below), so these all naturally stay `None`
    // for Flashback replays rather than being guessed.
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

    // Flashback-only fields (see FlashbackMeta.java's toJson()).
    if let Some(world_name) = value.get("world_name").and_then(|v| v.as_str()) {
        if !world_name.is_empty() {
            replay.world_name = Some(world_name.to_string());
        }
    }
    if replay.duration_ms.is_none() {
        if let Some(total_ticks) = value.get("total_ticks").and_then(|v| v.as_u64()) {
            // Flashback stores tick count, not a millisecond duration. This
            // assumes a steady 20 ticks/sec, which is the normal case but
            // won't be exactly right if the recording was ever paused or the
            // game's tick rate was altered mid-recording — an approximation,
            // not the same guarantee ReplayMod's own `duration` field gives.
            replay.duration_ms = Some(total_ticks * 50);
        }
    }
}

/// Reads whichever thumbnail entry the replay's zip actually has, returning
/// raw image bytes plus a best-guess MIME type. Both formats are confirmed
/// against their respective mods' own source:
///   - Flashback (`ReplayExporter.java`) embeds `icon.png` on a
///     best-effort basis (only written if the recorder captured one).
///   - ReplayMod (`AbstractReplayFile.java`) writes `thumb.jpg`; very old
///     replays may instead have a legacy `thumb` entry, optionally prefixed
///     with a short "magic number" header that must be stripped first.
fn read_zip_thumbnail(
    path: &Path,
    kind: ReplayKind,
) -> std::io::Result<Option<(Vec<u8>, &'static str)>> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    })?;

    match kind {
        ReplayKind::Flashback => {
            if let Ok(mut entry) = archive.by_name("icon.png") {
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                return Ok(Some((buf, "image/png")));
            }
        }
        ReplayKind::ReplayMod => {
            if let Ok(mut entry) = archive.by_name("thumb.jpg") {
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                return Ok(Some((buf, "image/jpeg")));
            }
            if let Ok(mut entry) = archive.by_name("thumb") {
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                // Legacy entries may be prefixed with a 7-byte Fibonacci
                // "magic number" header (0,1,1,2,3,5,8) — strip it if present.
                const MAGIC: [u8; 7] = [0, 1, 1, 2, 3, 5, 8];
                if buf.starts_with(&MAGIC) {
                    buf.drain(..MAGIC.len());
                }
                return Ok(Some((buf, "image/jpeg")));
            }
        }
    }

    Ok(None)
}

/// Lazily fetches one replay's thumbnail as a data URL, rather than bundling
/// image bytes into every entry from `list_replays` — with hundreds of
/// replays, eagerly decoding + shipping every thumbnail up front would bring
/// back the exact loading-time problem the metadata-only bulk load was built
/// to avoid. The frontend calls this per-row once it's actually rendered.
pub async fn get_replay_thumbnail(
    instance: &Path,
    kind: ReplayKind,
    file_name: &str,
) -> Result<Option<String>> {
    let path = safe_join(&kind.folder(instance), file_name)?;
    let result =
        tokio::task::spawn_blocking(move || read_zip_thumbnail(&path, kind))
            .await
            .map_err(|e| {
                ErrorKind::OtherError(format!("thumbnail task panicked: {e}"))
            })?
            .ok();

    Ok(result.flatten().map(|(bytes, mime)| {
        use base64::Engine;
        format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    }))
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
