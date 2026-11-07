/**
 * Modrinth Studios addition: Pterodactyl servers are given synthetic ids of
 * the form `ptero-<panelId>-<identifier>` so the hosting UI can treat them
 * like any other server id. Requests that target them are intercepted by
 * PterodactylFeature (see feature.ts) before they reach Modrinth's backends.
 */

export const PTERO_WORLD_ID = 'default'
const HOST_SUFFIX = '.ptero.studio.invalid'
const ID_PATTERN = /ptero-[a-z0-9]+-[a-z0-9]+/i

export type PteroServerRef = { serverId: string; panelId: string; identifier: string }

export function isPteroServerId(id: string | null | undefined): boolean {
	return !!id && /^ptero-[a-z0-9]+-[a-z0-9]+$/i.test(id)
}

export function makePteroServerId(panelId: string, identifier: string): string {
	return `ptero-${panelId}-${identifier}`
}

export function parsePteroServerId(serverId: string): PteroServerRef {
	const [, panelId, identifier] = serverId.split('-')
	return { serverId, panelId, identifier }
}

/** Fake "node host" handed to the UI where it expects a Kyros node. */
export function pteroNodeHost(serverId: string): string {
	return `${serverId}${HOST_SUFFIX}`
}

/** Finds a Pterodactyl server id in a request URL, if any. */
export function findPteroServerId(url: string): string | null {
	if (!url.includes('ptero-')) return null
	const match = url.match(ID_PATTERN)
	return match && isPteroServerId(match[0]) ? match[0] : null
}

export function isPteroNodeUrl(url: string): boolean {
	return url.includes(HOST_SUFFIX)
}

/**
 * Download "tokens" for Pterodactyl are the signed Wings URL itself, wrapped
 * into a fake node URL by the UI. This pulls the real URL back out.
 */
export function resolvePteroDownloadUrl(url: string): string {
	if (!isPteroNodeUrl(url)) return url
	try {
		const token = new URL(url).searchParams.get('token')
		return token ?? url
	} catch {
		return url
	}
}

/** A JWT-shaped token that never expires, so NodeAuthFeature won't try to refresh it. */
export const PTERO_FAKE_JWT = [
	btoa(JSON.stringify({ alg: 'none', typ: 'JWT' })),
	btoa(JSON.stringify({ exp: 4102444800, studio: 'pterodactyl' })),
	'pterodactyl',
]
	.map((part) => part.replace(/=+$/, '').replace(/\+/g, '-').replace(/\//g, '_'))
	.join('.')
