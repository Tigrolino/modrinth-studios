/**
 * Modrinth Studios addition: per-server Studio settings stored on the
 * server itself (`/.modrinth-studio/settings.json`).
 */
import { writeText } from './files'
import type { PteroServerRef } from './ids'
import {
	invalidatePteroServerInfo,
	readStudioJson,
	STUDIO_SETTINGS_PATH,
	type StudioServerSettings,
} from './server'

export async function updateStudioSettings(
	ref: PteroServerRef,
	mutate: (settings: StudioServerSettings) => void,
): Promise<StudioServerSettings> {
	const settings = (await readStudioJson<StudioServerSettings>(ref, STUDIO_SETTINGS_PATH)) ?? {}
	mutate(settings)
	await writeText(ref, STUDIO_SETTINGS_PATH, JSON.stringify(settings, null, 2))
	invalidatePteroServerInfo(ref.serverId)
	return settings
}
