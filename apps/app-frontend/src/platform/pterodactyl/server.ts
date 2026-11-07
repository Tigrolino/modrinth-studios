/**
 * Modrinth Studios addition: Pterodactyl server info, flavor detection and
 * mapping onto the Archon server shapes the hosting UI expects.
 */
import type { Archon } from '@modrinth/api-client'

import { listPanels, type PteroPanel, pteroRequest } from './bridge'
import {
	makePteroServerId,
	PTERO_FAKE_JWT,
	PTERO_WORLD_ID,
	type PteroServerRef,
	pteroNodeHost,
} from './ids'

export type PteroAllocation = {
	id: number
	ip: string
	ip_alias: string | null
	port: number
	notes: string | null
	is_default: boolean
}

export type PteroVariable = {
	name: string
	description: string
	env_variable: string
	default_value: string
	server_value: string | null
	is_editable: boolean
	rules: string
}

export type PteroServerAttributes = {
	server_owner: boolean
	identifier: string
	uuid: string
	name: string
	node: string
	sftp_details: { ip: string; port: number }
	description: string
	limits: { memory: number; swap: number; disk: number; io: number; cpu: number }
	invocation: string
	docker_image: string
	egg_features: string[] | null
	feature_limits: { databases: number; allocations: number; backups: number }
	status: string | null
	is_suspended: boolean
	is_installing: boolean
	is_transferring: boolean
	relationships?: {
		allocations?: { data: { attributes: PteroAllocation }[] }
		variables?: { data: { attributes: PteroVariable }[] }
		egg?: { attributes: { uuid: string; name: string } }
	}
}

/**
 * What kind of server this is. Decides which Modrinth loader the UI shows,
 * where content goes and whether the Modrinth content browser is offered.
 */
export type PteroFlavor =
	| 'paper'
	| 'purpur'
	| 'spigot'
	| 'proxy'
	| 'fabric'
	| 'quilt'
	| 'forge'
	| 'neoforge'
	| 'vanilla'
	| 'pumpkin'
	| 'unknown'

export type PteroServerInfo = {
	ref: PteroServerRef
	panel: PteroPanel | null
	attributes: PteroServerAttributes
	username: string
	flavor: PteroFlavor
	/** Flavor implied by the egg alone (ignores a Studio loader override). */
	eggFlavor: PteroFlavor
	settings: StudioServerSettings
	mcVersion: string | null
	loaderVersion: string | null
	variables: Record<string, string>
}

const INFO_TTL_MS = 10_000
const infoCache = new Map<string, { at: number; promise: Promise<PteroServerInfo> }>()
let usernameCache: Map<string, Promise<string>> | null = null

export function invalidatePteroServerInfo(serverId: string) {
	infoCache.delete(serverId)
}

function getUsername(panelId: string): Promise<string> {
	usernameCache ??= new Map()
	let promise = usernameCache.get(panelId)
	if (!promise) {
		promise = pteroRequest<{ attributes: { username: string } }>(panelId, 'GET', '/account')
			.then((r) => r.attributes.username)
			.catch(() => '')
		usernameCache.set(panelId, promise)
	}
	return promise
}

function variableMap(attributes: PteroServerAttributes): Record<string, string> {
	const map: Record<string, string> = {}
	for (const { attributes: v } of attributes.relationships?.variables?.data ?? []) {
		map[v.env_variable] = v.server_value ?? v.default_value ?? ''
	}
	return map
}

function pickVariable(vars: Record<string, string>, names: string[]): string | null {
	for (const name of names) {
		const value = vars[name]?.trim()
		if (value && value.toLowerCase() !== 'latest' && value.toLowerCase() !== 'recommended')
			return value
	}
	return null
}

export function detectFlavor(attributes: PteroServerAttributes): PteroFlavor {
	const egg = attributes.relationships?.egg?.attributes.name ?? ''
	const vars = Object.keys(variableMap(attributes)).join(' ')
	const haystack = `${egg} ${attributes.invocation} ${attributes.docker_image} ${vars}`.toLowerCase()
	const has = (...needles: string[]) => needles.some((n) => haystack.includes(n))
	if (has('pumpkin')) return 'pumpkin'
	if (has('velocity', 'bungee', 'waterfall', 'travertine')) return 'proxy'
	if (has('neoforge', 'neo_forge', 'neo forge')) return 'neoforge'
	if (has('forge')) return 'forge'
	if (has('quilt')) return 'quilt'
	if (has('fabric')) return 'fabric'
	if (has('purpur')) return 'purpur'
	if (has('paper', 'folia')) return 'paper'
	if (has('spigot', 'bukkit')) return 'spigot'
	if (has('vanilla')) return 'vanilla'
	return 'unknown'
}

export async function fetchServerInfo(ref: PteroServerRef): Promise<PteroServerInfo> {
	const cached = infoCache.get(ref.serverId)
	if (cached && Date.now() - cached.at < INFO_TTL_MS) return cached.promise

	const promise = (async () => {
		const [response, panels, username] = await Promise.all([
			pteroRequest<{ attributes: PteroServerAttributes }>(
				ref.panelId,
				'GET',
				`/servers/${ref.identifier}?include=egg`,
			),
			listPanels().catch(() => [] as PteroPanel[]),
			getUsername(ref.panelId),
		])
		const attributes = response.attributes
		const variables = variableMap(attributes)
		let eggFlavor = detectFlavor(attributes)
		if (eggFlavor === 'unknown') eggFlavor = await guessFlavorFromFiles(ref)
		const [override, settings] = await Promise.all([
			readLoaderOverride(ref),
			readStudioJson<StudioServerSettings>(ref, STUDIO_SETTINGS_PATH),
		])
		return {
			ref,
			panel: panels.find((p) => p.id === ref.panelId) ?? null,
			attributes,
			username,
			flavor: override?.flavor ?? eggFlavor,
			eggFlavor,
			settings: settings ?? {},
			variables,
			mcVersion: override?.mc_version ?? pickVariable(variables, [
				'MINECRAFT_VERSION',
				'MC_VERSION',
				'VANILLA_VERSION',
				'VERSION',
				'MINECRAFT_VER',
			]),
			loaderVersion: override ? override.loader_version : pickVariable(variables, [
				'FABRIC_VERSION',
				'LOADER_VERSION',
				'QUILT_VERSION',
				'NEOFORGE_VERSION',
				'FORGE_VERSION',
				'BUILD_NUMBER',
				'BUILD',
			]),
		}
	})()
	infoCache.set(ref.serverId, { at: Date.now(), promise })
	promise.catch(() => infoCache.delete(ref.serverId))
	return promise
}

/**
 * When Studio installs a loader jar itself (e.g. Fabric on a Paper egg) it
 * records what it installed here, since the egg no longer describes the server.
 */
export const LOADER_OVERRIDE_PATH = '/.modrinth-studio/loader.json'

export type LoaderOverride = {
	flavor: PteroFlavor
	mc_version: string | null
	loader_version: string | null
}

/** Per-server Studio settings (custom address, backup schedule split, …). */
export const STUDIO_SETTINGS_PATH = '/.modrinth-studio/settings.json'

export type StudioServerSettings = {
	address?: string | null
	backup_split_manual?: number
	backup_split_automatic?: number
	backup_interval?: number
	/** Display names for backups (Pterodactyl can't rename them). */
	backup_names?: Record<string, string>
	/** Backups created by hand from Studio; others are treated as scheduled. */
	manual_backup_ids?: string[]
	/** Activity before this time is hidden ("clear log"). */
	activity_cleared_at?: string | null
}

export async function readStudioJson<T>(ref: PteroServerRef, path: string): Promise<T | null> {
	try {
		const value = await pteroRequest<T | string>(
			ref.panelId,
			'GET',
			`/servers/${ref.identifier}/files/contents?file=${encodeURIComponent(path)}`,
		)
		return typeof value === 'string' ? (JSON.parse(value) as T) : value
	} catch {
		return null
	}
}

async function readLoaderOverride(ref: PteroServerRef): Promise<LoaderOverride | null> {
	const parsed = await readStudioJson<LoaderOverride>(ref, LOADER_OVERRIDE_PATH)
	return parsed?.flavor ? parsed : null
}

async function guessFlavorFromFiles(ref: PteroServerRef): Promise<PteroFlavor> {
	try {
		const list = await pteroRequest<{ data: { attributes: { name: string } }[] }>(
			ref.panelId,
			'GET',
			`/servers/${ref.identifier}/files/list?directory=${encodeURIComponent('/')}`,
		)
		const names = new Set(list.data.map((f) => f.attributes.name.toLowerCase()))
		if ([...names].some((n) => n.startsWith('pumpkin'))) return 'pumpkin'
		if (names.has('fabric-server-launch.jar') || names.has('.fabric')) return 'fabric'
		if (names.has('quilt-server-launch.jar')) return 'quilt'
		if (names.has('libraries') && names.has('mods') && names.has('run.sh')) return 'forge'
		if (names.has('plugins')) return 'paper'
		if (names.has('mods')) return 'fabric'
		return 'vanilla'
	} catch {
		return 'vanilla'
	}
}

/** Loader name used by Archon's v0 server object (drives the UI's content type). */
export function archonLoader(flavor: PteroFlavor): Archon.Servers.v0.Loader | null {
	switch (flavor) {
		case 'paper':
		case 'spigot':
		case 'proxy':
		case 'pumpkin':
			return 'Paper'
		case 'purpur':
			return 'Purpur'
		case 'fabric':
			return 'Fabric'
		case 'quilt':
			return 'Quilt'
		case 'forge':
			return 'Forge'
		case 'neoforge':
			return 'NeoForge'
		default:
			return 'Vanilla'
	}
}

/** Loader string used in content responses / Modrinth version filters. */
export function modrinthLoader(flavor: PteroFlavor): string {
	switch (flavor) {
		case 'neoforge':
			return 'neoforge'
		case 'spigot':
			return 'spigot'
		case 'proxy':
			return 'velocity'
		case 'pumpkin':
			return 'pumpkin'
		case 'unknown':
			return 'vanilla'
		default:
			return flavor
	}
}

export function contentDirectory(flavor: PteroFlavor): '/mods' | '/plugins' | null {
	switch (flavor) {
		case 'fabric':
		case 'quilt':
		case 'forge':
		case 'neoforge':
			return '/mods'
		case 'paper':
		case 'purpur':
		case 'spigot':
		case 'proxy':
		case 'pumpkin':
			return '/plugins'
		default:
			return null
	}
}

export function primaryAllocation(info: PteroServerInfo): PteroAllocation | null {
	const allocations = info.attributes.relationships?.allocations?.data ?? []
	return (
		allocations.find((a) => a.attributes.is_default)?.attributes ??
		allocations[0]?.attributes ??
		null
	)
}

export function toArchonServerV0(info: PteroServerInfo): Archon.Servers.v0.Server & {
	studio_ptero: {
		flavor: PteroFlavor
		egg_flavor: PteroFlavor
		panel_url: string | null
		identifier: string
		allocation_limit: number
		backup_limit: number
	}
} {
	const a = info.attributes
	const allocation = primaryAllocation(info)
	const host = info.ref.serverId
	return {
		server_id: host,
		name: a.name,
		owner_id: 'pterodactyl',
		net: {
			ip: allocation ? (allocation.ip_alias ?? allocation.ip) : null,
			port: allocation?.port ?? 25565,
			domain: info.settings.address ?? '',
		},
		game: 'Minecraft',
		backup_quota: a.feature_limits.backups,
		used_backup_quota: 0,
		status: a.is_installing ? 'installing' : a.is_suspended ? 'suspended' : 'available',
		suspension_reason: a.is_suspended ? 'other' : null,
		loader: archonLoader(info.flavor),
		loader_version: info.loaderVersion,
		mc_version: info.mcVersion,
		upstream: null,
		sftp_username: info.username ? `${info.username}.${a.identifier}` : a.identifier,
		sftp_password: '',
		sftp_host: `${a.sftp_details.ip}:${a.sftp_details.port}`,
		datacenter: info.panel?.name ?? 'Pterodactyl',
		notices: [],
		node: { token: PTERO_FAKE_JWT, instance: pteroNodeHost(host) },
		flows: { intro: false },
		is_medal: false,
		// -1 parses as "every permission bit"; Pterodactyl enforces the real
		// subuser permissions server-side and errors surface in the UI.
		current_user_permissions: -1,
		locked_since: null,
		icon: null,
		studio_ptero: {
			flavor: info.flavor,
			egg_flavor: info.eggFlavor,
			panel_url: info.panel?.url ?? null,
			identifier: a.identifier,
			allocation_limit: a.feature_limits.allocations,
			backup_limit: a.feature_limits.backups,
		},
	}
}

export function toArchonServerV1(info: PteroServerInfo): Archon.Servers.v1.ServerFull {
	const a = info.attributes
	const host = pteroNodeHost(info.ref.serverId)
	return {
		id: info.ref.serverId,
		name: a.name,
		subdomain: info.settings.address ?? '',
		specs: {
			cpu: a.limits.cpu ? Math.max(1, Math.round(a.limits.cpu / 100)) : 0,
			memory_mb: a.limits.memory,
			storage_mb: a.limits.disk,
			swap_mb: a.limits.swap,
		},
		sftp_username: info.username ? `${info.username}.${a.identifier}` : a.identifier,
		sftp_password: '',
		tags: ['pterodactyl'],
		drop_client_mods: false,
		location: {
			status: 'assigned',
			location_metadata: {
				region: info.panel?.name ?? 'Pterodactyl',
				region_should_be_user_displayed: false,
				hostname: a.node,
				url_host: host,
				is_decommissioned_node: false,
			},
		},
		icon: null,
		worlds: [
			{
				id: PTERO_WORLD_ID,
				name: 'World',
				update_channel: 'release',
				created_at: new Date(0).toISOString(),
				is_active: true,
				download_method: { method_type: 'direct_node_download' },
				backups: [],
				content: {
					shared_instance_id: null,
					shared_instance_needs_update: false,
					requires_restart: false,
					modloader: modrinthLoader(info.flavor),
					modloader_version: info.loaderVersion ?? '',
					game_version: info.mcVersion ?? '',
					java_version: javaVersionFromImage(a.docker_image),
					invocation: a.invocation,
					original_invocation: a.invocation,
				},
				readiness: { data_synchronized_fetched: true },
			},
		],
	}
}

export function javaVersionFromImage(image: string): number | null {
	const match = image.match(/java[_:-]?(\d+)/i) ?? image.match(/(?:jdk|jre|openjdk)[_:-]?(\d+)/i)
	return match ? Number(match[1]) : null
}

/** Lists every server across all configured panels, already mapped to Archon v0. */
export async function listAllPteroServers(): Promise<
	{ panel: PteroPanel; servers: ReturnType<typeof toArchonServerV0>[]; error: string | null }[]
> {
	const panels = await listPanels()
	return Promise.all(
		panels.map(async (panel) => {
			try {
				const response = await pteroRequest<{ data: { attributes: { identifier: string } }[] }>(
					panel.id,
					'GET',
					'/?per_page=100',
				)
				const servers = await Promise.all(
					response.data.map(async ({ attributes }) => {
						const ref = {
							serverId: makePteroServerId(panel.id, attributes.identifier),
							panelId: panel.id,
							identifier: attributes.identifier,
						}
						return toArchonServerV0(await fetchServerInfo(ref))
					}),
				)
				return { panel, servers, error: null }
			} catch (error) {
				return {
					panel,
					servers: [],
					error: error instanceof Error ? error.message : String(error),
				}
			}
		}),
	)
}
