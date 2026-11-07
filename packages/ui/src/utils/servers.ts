import type { Archon } from '@modrinth/api-client'

/** Full address for a hosting domain: Modrinth subdomains get `.modrinth.gg`, custom domains (Studio / Pterodactyl) are used as-is. */
export function formatHostingDomain(domain: string) {
	return domain.includes('.') ? domain : `${domain}.modrinth.gg`
}

export function getHostingServerAddress(
	net: Archon.Servers.v0.Net | null | undefined,
	subdomain?: string,
) {
	const domain = net?.domain || subdomain
	if (domain) return domain.includes('.') ? domain : `${domain}.modrinth.gg`
	if (!net?.ip) return ''
	const host = net.ip.includes(':') && !net.ip.startsWith('[') ? `[${net.ip}]` : net.ip
	return net.port && net.port !== 25565 ? `${host}:${net.port}` : host
}

/** Modrinth Studios: true for self-hosted Pterodactyl servers shown in Hosting. */
export function isPterodactylServerId(serverId: string | null | undefined) {
	return String(serverId ?? '').startsWith('ptero-')
}

/**
 * Modrinth Studios: loaders a Pterodactyl server can switch to. Single-jar loaders
 * (vanilla, Fabric, Paper, Purpur) can be dropped in by Studio; Forge, NeoForge
 * and Quilt only when the egg already is that loader.
 */
export function pterodactylLoaders(server: unknown): string[] {
	const info = (server as { studio_ptero?: { flavor?: string; egg_flavor?: string } } | null)
		?.studio_ptero
	const egg = info?.egg_flavor ?? info?.flavor ?? 'unknown'
	if (['pumpkin', 'proxy', 'spigot'].includes(egg)) return []
	const loaders = ['vanilla', 'fabric', 'paper', 'purpur']
	if (['forge', 'neoforge', 'quilt'].includes(egg)) loaders.push(egg)
	return ['vanilla', 'fabric', 'neoforge', 'forge', 'quilt', 'paper', 'purpur'].filter((l) =>
		loaders.includes(l),
	)
}
