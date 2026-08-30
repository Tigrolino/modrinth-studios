// Modrinth Studios addition: backs the Settings > Storage page and the
// optional per-instance size shown next to playtime (see
// use-studio-appearance.ts's `showInstanceStorageUsage` setting). New file —
// purely additive, doesn't touch any shared/upstream composable.
import { invoke } from '@tauri-apps/api/core'

export interface StorageBreakdown {
	worlds_bytes: number
	resourcepacks_bytes: number
	shaderpacks_bytes: number
	mods_bytes: number
	replays_bytes: number
	other_bytes: number
}

export interface InstanceStorageUsage {
	instance_id: string
	name: string
	size_bytes: number
	breakdown: StorageBreakdown
}

export interface SystemStorageOverview {
	total_disk_bytes: number
	free_disk_bytes: number
	instances_other_bytes: number
	worlds_bytes: number
	resourcepacks_bytes: number
	shaderpacks_bytes: number
	mods_bytes: number
	replays_bytes: number
	shared_folders_bytes: number
	non_modrinth_bytes: number
}

export interface SharedFolderBreakdown {
	worlds_bytes: number
	resourcepacks_bytes: number
	other_bytes: number
}

export interface SharedFolderStorageUsage {
	shared_profile_id: string
	name: string
	size_bytes: number
	member_count: number
	breakdown: SharedFolderBreakdown
	full_path: string
}

/** Every instance's on-disk size, largest first — a full recursive walk of
 * every instance folder on the Rust side (see `instance_storage_usage()` in
 * `packages/app-lib/src/api/instance/storage.rs`), so this is worth caching
 * by the caller rather than calling repeatedly in a tight loop. */
export async function fetchInstanceStorageUsage(): Promise<InstanceStorageUsage[]> {
	const usage = await invoke<InstanceStorageUsage[]>('plugin:studio|studio_instance_storage_usage')
	for (const entry of usage) singleUsageCache.set(entry.instance_id, entry.size_bytes)
	return [...usage].sort((a, b) => b.size_bytes - a.size_bytes)
}

// Modrinth Studios addition: the instance page header's "storage next to
// playtime" display used to call fetchInstanceStorageUsage() above — a full
// walk of *every* instance — just to read off the one it needed, which is
// why it felt slow and inconsistent (its timing depended on how big your
// whole library was, not just the one instance you had open). A small
// in-memory cache, keyed by instance id and shared across every component
// that asks, fixes the "appears late/randomly" complaint two ways: a second
// visit to the same instance in this session shows its size immediately
// (no re-walk), and the Storage settings page's full-list fetch above
// populates this same cache, so opening an instance page after having
// looked at Settings > Storage already also shows instantly. Lives only for
// the running session (module-level, not persisted) — sizes change as you
// play/install things, so there's no "correct" long-lived TTL; a stale
// number for one session is an acceptable tradeoff for not re-walking a
// multi-GB folder on every navigation.
const singleUsageCache = new Map<string, number>()

/** Synchronous cache read — use this to show a previously-seen size
 * immediately while `fetchInstanceStorageUsageSingle()` refreshes it in the
 * background, rather than leaving the display blank until that resolves. */
export function getCachedInstanceStorageUsage(instanceId: string): number | undefined {
	return singleUsageCache.get(instanceId)
}

/** Walks just one instance's folder (see `instance_storage_usage_single()`
 * on the Rust side) instead of the whole library. Returns `null` if the
 * instance no longer exists. Updates the shared cache above as a side
 * effect, so `getCachedInstanceStorageUsage()` reflects the fresh value
 * immediately after this resolves. */
export async function fetchInstanceStorageUsageSingle(instanceId: string): Promise<number | null> {
	const entry = await invoke<InstanceStorageUsage | null>(
		'plugin:studio|studio_instance_storage_usage_single',
		{ instanceId },
	)
	if (entry) {
		singleUsageCache.set(instanceId, entry.size_bytes)
		return entry.size_bytes
	}
	return null
}

/** Every shared folder's actual on-disk size, largest first — kept separate
 * from `fetchInstanceStorageUsage()` above on purpose. A member instance
 * only holds a link into this data, not a copy of it, so it's tracked once
 * here against the shared folder itself rather than once per instance using
 * it (see `shared_folder_storage_usage()` in
 * `packages/app-lib/src/api/instance/storage.rs`). */
export async function fetchSharedFolderStorageUsage(): Promise<SharedFolderStorageUsage[]> {
	const usage = await invoke<SharedFolderStorageUsage[]>(
		'plugin:studio|studio_shared_folder_storage_usage',
	)
	return [...usage].sort((a, b) => b.size_bytes - a.size_bytes)
}

/** The Steam-style overview bar's segments — see
 * `system_storage_overview()` in `packages/app-lib/src/api/instance/storage.rs`
 * for how each one is computed. */
export async function fetchSystemStorageOverview(): Promise<SystemStorageOverview> {
	return await invoke<SystemStorageOverview>('plugin:studio|studio_system_storage_overview')
}

const SIZE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const

/** Formats a byte count the way the rest of the app's file-size displays do
 * (binary/1024-based units, e.g. "1.4 GB") — kept local rather than
 * imported from elsewhere since nothing else in this app currently exposes
 * a shared byte-formatting helper. */
export function formatStorageSize(bytes: number): string {
	if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'

	let value = bytes
	let unitIndex = 0
	while (value >= 1024 && unitIndex < SIZE_UNITS.length - 1) {
		value /= 1024
		unitIndex++
	}

	const decimals = unitIndex === 0 ? 0 : value < 10 ? 1 : 0
	return `${value.toFixed(decimals)} ${SIZE_UNITS[unitIndex]}`
}
