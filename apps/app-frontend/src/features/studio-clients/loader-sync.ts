/**
 * Modrinth Studios: keeps a client installation's Modrinth loader in line
 * with what the client runs. Lunar + Fabric is a Fabric instance (so the
 * Content tab and browse filters offer Fabric mods), Lunar + Forge a Forge
 * one; plain Lunar is vanilla.
 */
import { install_existing_instance } from '@/helpers/install'
import { edit } from '@/helpers/instance'
import type { ClientInstall } from '@/helpers/studio-clients'

export function wantedLoader(install: ClientInstall | null | undefined): string | null {
	if (!install || install.client !== 'lunar') return null
	if (install.version_id.endsWith('|fabric')) return 'fabric'
	if (install.version_id.endsWith('|forge')) return 'forge'
	return 'vanilla'
}

const pending = new Set<string>()

/** Switches the instance's loader if it doesn't match the client. */
export async function syncClientLoader(
	instance: { id: string; loader: string; game_version: string },
	install: ClientInstall | null | undefined,
) {
	const loader = wantedLoader(install)
	if (!loader || loader === instance.loader || pending.has(instance.id)) return
	pending.add(instance.id)
	try {
		await edit(instance.id, {
			loader,
			game_version: instance.game_version,
			...(loader === 'vanilla' ? {} : { loader_version: 'stable' }),
		} as Parameters<typeof edit>[1])
		await install_existing_instance(instance.id, false)
	} finally {
		pending.delete(instance.id)
	}
}
