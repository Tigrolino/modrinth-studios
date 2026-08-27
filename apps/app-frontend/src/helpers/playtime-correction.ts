// Modrinth Studios addition: bindings for the `playtime_correction` Tauri
// plugin (packages/app-lib/src/api/playtime_correction.rs +
// apps/app/src/api/playtime_correction.rs). New file — nothing here touches
// upstream Modrinth code.
import { invoke } from '@tauri-apps/api/core'

/** Current correction, in seconds. `0` if none has ever been set. */
export async function getPlaytimeCorrection(instanceId: string): Promise<number> {
	return await invoke('plugin:playtime-correction|playtime_correction_get', { instanceId })
}

/** Sets the correction to an exact value, in seconds — overwrites, not adds. */
export async function setPlaytimeCorrection(instanceId: string, seconds: number): Promise<void> {
	return await invoke('plugin:playtime-correction|playtime_correction_set', {
		instanceId,
		seconds: Math.round(seconds),
	})
}
