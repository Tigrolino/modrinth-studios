//! Modrinth Studios addition: backs the Settings > Storage page — measures
//! how much disk space each instance actually uses, so instances can be
//! listed by size rather than the person having to go digging through
//! Explorer themselves.
//!
//! Also measures shared folders (`crate::api::shared_profile`) separately —
//! an instance that's a member of one only holds a directory
//! junction/symlink or hard link into that data, not a copy of it, so its
//! own `directory_size()` walk (see that function's doc comment) already
//! skips over it entirely rather than counting it. Without a separate
//! listing, that space would look like it just isn't tracked anywhere; with
//! one, it shows up exactly once — against the shared folder itself — no
//! matter how many instances are using it, instead of once per instance.

use crate::state::State;
use crate::util::link;
use std::path::Path;

/// Modrinth Studios addition: which part of an instance's own folder its
/// space is actually going to — shown as a composition bar on the Storage
/// page instead of every instance just being an opaque total. `other_bytes`
/// is everything left over (configs, logs, the game jar/libraries/assets
/// cache, screenshots, crash reports, instance metadata, etc.) — there's no
/// single upstream folder name for "the base install", so it's whatever
/// doesn't match one of the named buckets below rather than its own walk.
#[derive(serde::Serialize, Debug, Clone, Default)]
pub struct StorageBreakdown {
    pub worlds_bytes: u64,
    pub resourcepacks_bytes: u64,
    pub shaderpacks_bytes: u64,
    pub mods_bytes: u64,
    pub replays_bytes: u64,
    pub other_bytes: u64,
}

impl StorageBreakdown {
    pub fn total(&self) -> u64 {
        self.worlds_bytes
            + self.resourcepacks_bytes
            + self.shaderpacks_bytes
            + self.mods_bytes
            + self.replays_bytes
            + self.other_bytes
    }
}

#[derive(serde::Serialize, Debug, Clone)]
pub struct InstanceStorageUsage {
    pub instance_id: String,
    pub name: String,
    pub size_bytes: u64,
    pub breakdown: StorageBreakdown,
}

/// Modrinth Studios addition: same idea as `StorageBreakdown`, but for a
/// shared folder's own composition instead of an instance's — a shared
/// folder can only ever contain the specific items `api::shared_profile`
/// knows how to share (see its `SHAREABLE_ITEMS` list): `saves`,
/// `resourcepacks`, `config`, `options.txt`, `servers.dat`. `config`,
/// `options.txt`, and `servers.dat` don't get their own bucket here (there's
/// only ever at most a few small config files in there, not worth calling
/// out) — they fold into `other_bytes`, mirroring how `StorageBreakdown`
/// folds an instance's own `config` folder into *its* `other_bytes` too.
#[derive(serde::Serialize, Debug, Clone, Default)]
pub struct SharedFolderBreakdown {
    pub worlds_bytes: u64,
    pub resourcepacks_bytes: u64,
    pub other_bytes: u64,
}

impl SharedFolderBreakdown {
    pub fn total(&self) -> u64 {
        self.worlds_bytes + self.resourcepacks_bytes + self.other_bytes
    }
}

#[derive(serde::Serialize, Debug, Clone)]
pub struct SharedFolderStorageUsage {
    pub shared_profile_id: String,
    pub name: String,
    pub size_bytes: u64,
    pub member_count: i64,
    pub breakdown: SharedFolderBreakdown,
    /// Absolute path to this shared folder's data on disk, so the Storage
    /// page can offer an "open folder" action the same way it does for
    /// instances, without a second round-trip just to resolve the path.
    pub full_path: String,
}

/// Recursively measures every instance's folder. Skips symlinks entirely —
/// doesn't traverse into them, doesn't count anything toward their target's
/// size — rather than following them: instance folders can contain
/// symlinked content, and following a symlink here could either
/// double-count content shared across instances or, in a pathological
/// case, loop forever on a circular link. A plain recursive size count is
/// the norm for most disk-usage tools anyway.
pub async fn instance_storage_usage()
-> crate::Result<Vec<InstanceStorageUsage>> {
    let state = State::get().await?;
    let instances = crate::state::list_instances(&state.pool).await?;

    // Modrinth Studios addition: walk every instance concurrently rather than
    // one at a time. This was the actual cause of the Storage page taking
    // ~15s to load with 20-30 instances — each folder's walk is I/O-bound
    // (lots of small `read_dir`/`metadata` syscalls, not CPU work), so doing
    // them one after another means the wall-clock time is the *sum* of every
    // instance's walk time. Running them concurrently overlaps that I/O, so
    // the whole thing takes roughly as long as the single slowest instance
    // instead of the sum of all of them. `join_all` (rather than
    // `tokio::spawn`-ing each one) is enough here since there's no CPU-bound
    // work to actually spread across cores — just I/O to overlap — and it
    // avoids the `'static`/`Send` bookkeeping spawning each walk would need.
    let usage = futures::future::join_all(instances.into_iter().map(|metadata| {
        let instances_dir = state.directories.instances_dir();
        async move {
            let instance = metadata.instance;
            let full_path = instances_dir.join(&instance.path);
            let breakdown = categorize_instance_dir(&full_path).await;
            let size_bytes = breakdown.total();
            InstanceStorageUsage {
                instance_id: instance.id,
                name: instance.name,
                size_bytes,
                breakdown,
            }
        }
    }))
    .await;

    Ok(usage)
}

/// Modrinth Studios addition: the system-wide summary bar at the top of the
/// Storage page (styled after Steam's own per-drive storage breakdown).
/// Every field is in bytes and, `total_disk_bytes`/`free_disk_bytes` aside,
/// they're meant to be read as segments of one bar that sums to
/// `total_disk_bytes`: `worlds_bytes` through `replays_bytes` are each
/// category summed across *every* instance, `instances_other_bytes` is
/// whatever's left of instance folders once those categories are pulled out
/// (see `StorageBreakdown::other_bytes`), `shared_folders_bytes` is shared
/// folders' own total (kept separate rather than folded into the categories
/// above — see `shared_folder_storage_usage`'s doc comment for why that data
/// only counts once), and `non_modrinth_bytes` is everything else on the
/// drive Studio's data lives on (every other app/file, the OS itself, etc.)
/// — computed as a remainder rather than measured directly, since actually
/// walking "everything else on this drive" isn't something any of this needs
/// to do.
#[derive(serde::Serialize, Debug, Clone)]
pub struct SystemStorageOverview {
    pub total_disk_bytes: u64,
    pub free_disk_bytes: u64,
    pub instances_other_bytes: u64,
    pub worlds_bytes: u64,
    pub resourcepacks_bytes: u64,
    pub shaderpacks_bytes: u64,
    pub mods_bytes: u64,
    pub replays_bytes: u64,
    pub shared_folders_bytes: u64,
    pub non_modrinth_bytes: u64,
}

/// Finds the disk that holds `path` and returns its `(total, available)`
/// space in bytes — `None` if no mounted disk's mount point is a prefix of
/// the (canonicalized) path, which shouldn't normally happen for a path that
/// exists. Mirrors the exact same lookup `state::dirs`'s own
/// `get_disk_usage` already uses elsewhere in this fork, rather than
/// introducing a second way of doing the same thing.
fn disk_totals(path: &Path) -> Option<(u64, u64)> {
    let path = crate::util::io::canonicalize(path).ok()?;
    let disks = sysinfo::Disks::new_with_refreshed_list();

    for disk in &disks {
        if path.starts_with(disk.mount_point()) {
            return Some((disk.total_space(), disk.available_space()));
        }
    }

    None
}

/// Builds the Storage page's system-wide overview bar. Reuses
/// `instance_storage_usage()`/`shared_folder_storage_usage()` rather than
/// re-walking the filesystem a second way, so this never disagrees with what
/// the per-instance/per-shared-folder lists below it show.
pub async fn system_storage_overview() -> crate::Result<SystemStorageOverview> {
    let state = State::get().await?;

    let instances = instance_storage_usage().await?;
    let shared_folders = shared_folder_storage_usage().await?;

    let mut worlds_bytes = 0u64;
    let mut resourcepacks_bytes = 0u64;
    let mut shaderpacks_bytes = 0u64;
    let mut mods_bytes = 0u64;
    let mut replays_bytes = 0u64;
    let mut instances_other_bytes = 0u64;
    for entry in &instances {
        worlds_bytes += entry.breakdown.worlds_bytes;
        resourcepacks_bytes += entry.breakdown.resourcepacks_bytes;
        shaderpacks_bytes += entry.breakdown.shaderpacks_bytes;
        mods_bytes += entry.breakdown.mods_bytes;
        replays_bytes += entry.breakdown.replays_bytes;
        instances_other_bytes += entry.breakdown.other_bytes;
    }

    let shared_folders_bytes: u64 =
        shared_folders.iter().map(|entry| entry.size_bytes).sum();

    let instances_dir = state.directories.instances_dir();
    let (total_disk_bytes, free_disk_bytes) =
        disk_totals(&instances_dir).unwrap_or((0, 0));

    let modrinth_total = worlds_bytes
        + resourcepacks_bytes
        + shaderpacks_bytes
        + mods_bytes
        + replays_bytes
        + instances_other_bytes
        + shared_folders_bytes;

    // `saturating_sub` rather than plain subtraction: `total_disk_bytes`
    // being 0 (disk lookup failed) or a race between the walk above and the
    // disk's live free-space reading could otherwise underflow here.
    let non_modrinth_bytes = total_disk_bytes
        .saturating_sub(free_disk_bytes)
        .saturating_sub(modrinth_total);

    Ok(SystemStorageOverview {
        total_disk_bytes,
        free_disk_bytes,
        instances_other_bytes,
        worlds_bytes,
        resourcepacks_bytes,
        shaderpacks_bytes,
        mods_bytes,
        replays_bytes,
        shared_folders_bytes,
        non_modrinth_bytes,
    })
}

/// Same measurement as `instance_storage_usage()`, but for a single instance
/// — used by the instance page header's "storage next to playtime" display
/// (see `use-studio-instance-storage.ts`) so opening one instance only ever
/// pays for walking *that* instance's folder, not every instance in the
/// library. Walking the full list just to read off one entry was the actual
/// cause of that header value feeling slow and inconsistent to show up: its
/// timing depended on how many *other*, unrelated instances happened to be
/// in the library. Returns `None` if the instance id doesn't exist (already
/// deleted, or a stale id from a closed page) rather than erroring.
pub async fn instance_storage_usage_single(
    instance_id: &str,
) -> crate::Result<Option<InstanceStorageUsage>> {
    let state = State::get().await?;
    let Some(metadata) =
        crate::state::get_instance(instance_id, &state.pool).await?
    else {
        return Ok(None);
    };

    let instance = metadata.instance;
    let full_path = state.directories.instances_dir().join(&instance.path);
    let breakdown = categorize_instance_dir(&full_path).await;
    let size_bytes = breakdown.total();

    Ok(Some(InstanceStorageUsage {
        instance_id: instance.id,
        name: instance.name,
        size_bytes,
        breakdown,
    }))
}

/// Iterative (not recursive-async) directory walk — sidesteps the awkward
/// `Box::pin` boilerplate a genuinely recursive `async fn` would need here,
/// and has no risk of stack growth for a very deeply nested folder.
async fn directory_size(root: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];

    while let Some(current) = stack.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(&current).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(file_type) = entry.file_type().await else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                // `entry.file_type()` above is the fast path (no extra
                // syscall on either platform) and should already catch a
                // shared folder's directory junction/symlink via
                // `is_symlink()` — Windows reports junctions as reparse
                // points the same way it does real symlinks. This second,
                // slightly more expensive check (see `link::is_link`'s doc
                // comment for exactly what it does differently) is a
                // deliberate backstop rather than the primary check, so a
                // shared saves/config/resourcepacks folder can never get
                // walked into and counted as this instance's own storage
                // even if that fast path ever turns out to miss a case —
                // see `crate::api::instance::storage`'s module doc comment
                // for why double-counting shared data matters here.
                if link::is_link(entry.path()).await {
                    continue;
                }
                stack.push(entry.path());
            } else if let Ok(metadata) = entry.metadata().await {
                total += metadata.len();
            }
        }
    }

    total
}

/// Walks only the top level of an instance's folder, bucketing each
/// recognized subfolder's *full recursive* size (via `directory_size` above)
/// into the matching `StorageBreakdown` field — mods/resourcepacks/
/// shaderpacks/saves are all standard, unambiguous folder names Minecraft and
/// every mod loader already agree on, and replays live in either
/// `replay_recordings` (ReplayMod) or `flashback` (Flashback — see
/// `crate::api::replays::ReplayKind`). Everything else at the top level
/// (config, logs, screenshots, crash-reports, the instance's own metadata,
/// the game jar/libraries/assets cache, etc.) has no single name worth
/// calling out on its own, so it's folded into `other_bytes` instead of
/// getting its own bucket. Matches the same symlink/junction skip that
/// `directory_size` uses, for the same reason (never count a shared folder's
/// data as if it were this instance's own).
async fn categorize_instance_dir(root: &Path) -> StorageBreakdown {
    let mut breakdown = StorageBreakdown::default();

    let Ok(mut entries) = tokio::fs::read_dir(root).await else {
        return breakdown;
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            let path = entry.path();
            if link::is_link(&path).await {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let size = directory_size(&path).await;
            match name.as_str() {
                "saves" => breakdown.worlds_bytes += size,
                "resourcepacks" => breakdown.resourcepacks_bytes += size,
                "shaderpacks" => breakdown.shaderpacks_bytes += size,
                "mods" => breakdown.mods_bytes += size,
                "replay_recordings" | "flashback" => {
                    breakdown.replays_bytes += size;
                }
                _ => breakdown.other_bytes += size,
            }
        } else if let Ok(metadata) = entry.metadata().await {
            breakdown.other_bytes += metadata.len();
        }
    }

    breakdown
}

/// Same idea as `categorize_instance_dir`, but for a shared folder's root —
/// see `SharedFolderBreakdown`'s doc comment for exactly which top-level
/// items map to which bucket. No symlink/junction check needed here (unlike
/// `categorize_instance_dir`): a shared folder's own directory is the real
/// data itself, never a link to something else.
async fn categorize_shared_folder_dir(root: &Path) -> SharedFolderBreakdown {
    let mut breakdown = SharedFolderBreakdown::default();

    let Ok(mut entries) = tokio::fs::read_dir(root).await else {
        return breakdown;
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };

        if file_type.is_dir() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let size = directory_size(&entry.path()).await;
            match name.as_str() {
                "saves" => breakdown.worlds_bytes += size,
                "resourcepacks" => breakdown.resourcepacks_bytes += size,
                _ => breakdown.other_bytes += size,
            }
        } else if let Ok(metadata) = entry.metadata().await {
            breakdown.other_bytes += metadata.len();
        }
    }

    breakdown
}

/// Measures how much disk space every shared folder's *actual* data takes up
/// — see the module doc comment for why this needs to be tracked separately
/// from any instance's own number rather than folded into one.
pub async fn shared_folder_storage_usage()
-> crate::Result<Vec<SharedFolderStorageUsage>> {
    let state = State::get().await?;

    let profiles: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, name FROM studio_shared_profiles ORDER BY name COLLATE NOCASE",
    )
    .fetch_all(&state.pool)
    .await?;

    let usage = futures::future::join_all(profiles.into_iter().map(
        |(id, name)| {
            let shared_profiles_dir = state.directories.shared_profiles_dir();
            let pool = state.pool.clone();
            async move {
                let full_path = shared_profiles_dir.join(&id);
                let breakdown = categorize_shared_folder_dir(&full_path).await;
                let size_bytes = breakdown.total();
                let member_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM studio_instance_shared_profiles WHERE shared_profile_id = ?",
                )
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap_or(0);

                SharedFolderStorageUsage {
                    shared_profile_id: id,
                    name,
                    size_bytes,
                    member_count,
                    breakdown,
                    full_path: full_path.to_string_lossy().into_owned(),
                }
            }
        },
    ))
    .await;

    Ok(usage)
}
