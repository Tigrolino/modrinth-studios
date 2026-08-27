// Modrinth Studios addition: bindings for the `replays` Tauri plugin
// (packages/app-lib/src/api/replays.rs + apps/app/src/api/replays.rs).
// New file — nothing here touches upstream Modrinth code.
import { invoke } from '@tauri-apps/api/core'

export type ReplayKind = 'replay_mod' | 'flashback'

export interface Replay {
	kind: ReplayKind
	folder: string
	fileName: string
	name: string
	size: number
	/** Unix seconds */
	modified: number
	durationMs?: number | null
	/** Unix seconds */
	recordedAt?: number | null
	minecraftVersion?: string | null
	serverName?: string | null
	singleplayer?: boolean | null
	/** Flashback-only — see the comment on `Replay::world_name` in replays.rs. */
	worldName?: string | null
}

export async function hasReplays(instanceId: string): Promise<boolean> {
	return await invoke('plugin:replays|replays_has_any', { instanceId })
}

export async function listReplays(instanceId: string): Promise<Replay[]> {
	return await invoke('plugin:replays|replays_list', { instanceId })
}

export async function deleteReplay(
	instanceId: string,
	kind: ReplayKind,
	fileName: string,
): Promise<void> {
	return await invoke('plugin:replays|replays_delete', { instanceId, kind, fileName })
}

export async function renameReplay(
	instanceId: string,
	kind: ReplayKind,
	fileName: string,
	newFileName: string,
): Promise<void> {
	return await invoke('plugin:replays|replays_rename', {
		instanceId,
		kind,
		fileName,
		newFileName,
	})
}

export async function importReplay(instanceId: string, sourcePath: string): Promise<void> {
	return await invoke('plugin:replays|replays_import', { instanceId, sourcePath })
}

/**
 * Lazily fetches a replay's embedded thumbnail as a `data:` URL. Not
 * included on `Replay` from `listReplays` — see the comment on
 * `get_replay_thumbnail` in replays.rs for why this is a separate call.
 */
export async function getReplayThumbnail(
	instanceId: string,
	kind: ReplayKind,
	fileName: string,
): Promise<string | null> {
	return await invoke('plugin:replays|replays_thumbnail', { instanceId, kind, fileName })
}
