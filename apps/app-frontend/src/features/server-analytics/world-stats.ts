/**
 * Modrinth Studios: all-time per-player statistics straight from the world's
 * `stats/<uuid>.json` files (the same numbers as the in-game Statistics screen).
 * These don't depend on chat/death messages being logged.
 */
import type { injectModrinthClient } from '@modrinth/ui'

type Client = ReturnType<typeof injectModrinthClient>

export type WorldPlayerStats = {
	uuid: string
	name: string
	deaths: number
	playTimeMs: number
	mobKills: number
	playerKills: number
	damageDealt: number
	damageTaken: number
	distanceCm: number
	distance: { onFoot: number; swimming: number; elytra: number; vehicles: number; creativeFlight: number }
	blocksMined: number
	itemsCrafted: number
	jumps: number
	lastModified: number
}

export type WorldStats = {
	level: string
	players: WorldPlayerStats[]
	totals: Omit<WorldPlayerStats, 'uuid' | 'name' | 'lastModified'>
}

// Travel stats (cm). "Fallen" is excluded like the in-game totals do, and creative
// flight is tracked separately so it doesn't swamp everything else.
const DISTANCE_GROUPS = {
	onFoot: ['walk_one_cm', 'sprint_one_cm', 'crouch_one_cm', 'walk_on_water_one_cm', 'walk_under_water_one_cm', 'climb_one_cm'],
	swimming: ['swim_one_cm'],
	elytra: ['aviate_one_cm'],
	vehicles: ['boat_one_cm', 'horse_one_cm', 'minecart_one_cm', 'pig_one_cm', 'strider_one_cm', 'happy_ghast_one_cm'],
	creativeFlight: ['fly_one_cm'],
} as const
/** Stats are 32-bit ints; a value pinned at the max has overflowed and is meaningless. */
const INT_MAX = 2_147_483_647

function sum(record: Record<string, number> | undefined) {
	return record ? Object.values(record).reduce((a, b) => a + (Number(b) || 0), 0) : 0
}

async function readText(client: Client, path: string): Promise<string | null> {
	try {
		return await (await client.kyros.files_v0.downloadFile(path)).text()
	} catch {
		return null
	}
}

async function levelName(client: Client): Promise<string> {
	const props = await readText(client, '/server.properties')
	const match = props?.match(/^level-name=(.*)$/m)
	return match?.[1]?.trim() || 'world'
}

async function listJson(client: Client, dir: string) {
	const files: { name: string; modified: number }[] = []
	let page = 1
	for (;;) {
		const res = await client.kyros.files_v0.listDirectory(dir, page, 100)
		const items = res.items ?? []
		files.push(
			...items
				.filter((i) => i.type !== 'directory' && i.name.endsWith('.json'))
				.map((i) => ({ name: i.name, modified: i.modified ?? 0 })),
		)
		if (items.length < 100 || page >= 50) break
		page++
	}
	return files
}

export class WorldStatsError extends Error {}

function isNotFound(error: unknown) {
	const status = (error as { statusCode?: number; status?: number })?.statusCode ??
		(error as { status?: number })?.status
	return status === 404 || /404|not found/i.test(error instanceof Error ? error.message : String(error))
}

/** Finds `<world>/stats`: level-name first, then any top-level folder that has one. */
async function findStatsDir(client: Client, level: string) {
	const tried: string[] = []
	const errors: string[] = []
	const attempt = async (dir: string) => {
		if (tried.includes(dir)) return null
		tried.push(dir)
		try {
			return await listJson(client, dir)
		} catch (error) {
			if (!isNotFound(error)) errors.push(`${dir}: ${error instanceof Error ? error.message : String(error)}`)
			return null
		}
	}
	// Newer Minecraft versions keep player data under <world>/players/.
	for (const dir of [
		`/${level}/players/stats`,
		`/${level}/stats`,
		'/world/players/stats',
		'/world/stats',
	]) {
		const files = await attempt(dir)
		if (files) return { dir, files, tried, errors }
	}
	// Some setups (Paper world-container, multi-world hosts) keep worlds elsewhere.
	try {
		const root = await client.kyros.files_v0.listDirectory('/', 1, 100)
		const dirs = (root.items ?? []).filter((i) => i.type === 'directory').slice(0, 40)
		for (const d of dirs) {
			for (const dir of [`/${d.name}/players/stats`, `/${d.name}/stats`]) {
				const files = await attempt(dir)
				if (files?.length) return { dir, files, tried, errors }
			}
		}
	} catch {
		// ignore
	}
	return { dir: null, files: [] as { name: string; modified: number }[], tried, errors }
}

export async function readWorldStats(client: Client): Promise<WorldStats | null> {
	const level = await levelName(client)
	const found = await findStatsDir(client, level)
	if (!found.dir) {
		if (found.errors.length) throw new WorldStatsError(found.errors.join(' · '))
		return { level: `/${level}/stats`, players: [], totals: emptyTotals() }
	}
	const dir = found.dir
	let files = found.files
	if (!files.length) return { level: dir, players: [], totals: emptyTotals() }
	files = files.slice(0, 300)

	const names = new Map<string, string>()
	const cache = await readText(client, '/usercache.json')
	if (cache) {
		try {
			for (const entry of JSON.parse(cache) as { name: string; uuid: string }[]) {
				names.set(entry.uuid.toLowerCase(), entry.name)
			}
		} catch {
			// ignore
		}
	}

	const players: WorldPlayerStats[] = []
	let index = 0
	const worker = async () => {
		while (index < files.length) {
			const file = files[index++]
			const text = await readText(client, `${dir}/${file.name}`)
			if (!text) continue
			try {
				const json = JSON.parse(text) as { stats?: Record<string, Record<string, number>> }
				const s = json.stats ?? {}
				const custom = s['minecraft:custom'] ?? {}
				const c = (key: string) => {
					const v = Number(custom[`minecraft:${key}`] ?? 0)
					return Number.isFinite(v) && v >= 0 && v < INT_MAX ? v : 0
				}
				const group = (keys: readonly string[]) => keys.reduce((a, k) => a + c(k), 0)
				const distance = {
					onFoot: group(DISTANCE_GROUPS.onFoot),
					swimming: group(DISTANCE_GROUPS.swimming),
					elytra: group(DISTANCE_GROUPS.elytra),
					vehicles: group(DISTANCE_GROUPS.vehicles),
					creativeFlight: group(DISTANCE_GROUPS.creativeFlight),
				}
				const uuid = file.name.replace(/\.json$/, '')
				players.push({
					uuid,
					name: names.get(uuid.toLowerCase()) ?? uuid.slice(0, 8),
					deaths: c('deaths'),
					playTimeMs: (c('play_time') || c('play_one_minute')) * 50,
					mobKills: c('mob_kills'),
					playerKills: c('player_kills'),
					damageDealt: c('damage_dealt') / 10,
					damageTaken: c('damage_taken') / 10,
					// Survival travel only; creative flight is shown separately.
					distanceCm: distance.onFoot + distance.swimming + distance.elytra + distance.vehicles,
					distance,
					blocksMined: sum(s['minecraft:mined']),
					itemsCrafted: sum(s['minecraft:crafted']),
					jumps: c('jump'),
					lastModified: (file.modified ?? 0) * (file.modified > 1e12 ? 1 : 1000),
				})
			} catch {
				// skip malformed file
			}
		}
	}
	await Promise.all(Array.from({ length: 4 }, worker))

	players.sort((a, b) => b.playTimeMs - a.playTimeMs)
	const totals = emptyTotals()
	for (const p of players) {
		for (const key of Object.keys(totals) as (keyof typeof totals)[]) {
			if (key === 'distance') continue
			;(totals as unknown as Record<string, number>)[key] += p[key] as number
		}
		for (const k of Object.keys(totals.distance) as (keyof typeof totals.distance)[]) {
			totals.distance[k] += p.distance[k]
		}
	}
	return { level: dir, players, totals }
}

function emptyTotals() {
	return {
		deaths: 0,
		playTimeMs: 0,
		mobKills: 0,
		playerKills: 0,
		damageDealt: 0,
		damageTaken: 0,
		distanceCm: 0,
		distance: { onFoot: 0, swimming: 0, elytra: 0, vehicles: 0, creativeFlight: 0 },
		blocksMined: 0,
		itemsCrafted: 0,
		jumps: 0,
	}
}
