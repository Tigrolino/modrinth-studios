/**
 * Modrinth Studios: records server analytics while a server is open in the
 * Hosting tab (any of its pages). Works for Modrinth Hosting and Pterodactyl
 * alike, because both feed the same Archon websocket events.
 *
 * - Resource usage (CPU, RAM, network in/out) from the `stats` socket event.
 * - Player count + latency from a Minecraft status ping every minute.
 *
 * Samples are kept per minute for 7 days in localStorage (per server).
 */
import type { Archon } from '@modrinth/api-client'
import { getHostingServerAddress, type injectModrinthClient } from '@modrinth/ui'
import { onScopeDispose, reactive, ref, type Ref, watch } from 'vue'

import { get_server_status } from '@/helpers/worlds'

export type AnalyticsSample = {
	/** Minute start (ms). */
	t: number
	cpu: number | null
	ram: number | null
	ramTotal: number | null
	/** Average bytes/second during the minute. */
	rx: number | null
	tx: number | null
	players: number | null
	maxPlayers: number | null
	ping: number | null
	/** 1 when the server was running during the minute. */
	up: 0 | 1
}

export type LiveAnalytics = {
	cpu: number | null
	ram: number | null
	ramTotal: number | null
	storage: number | null
	storageTotal: number | null
	rxRate: number | null
	txRate: number | null
	rxTotal: number | null
	txTotal: number | null
	players: number | null
	maxPlayers: number | null
	playerNames: string[]
	ping: number | null
	version: string | null
	motd: string | null
	pingError: string | null
	address: string | null
	lastPingAt: number | null
}

const RETENTION_MS = 7 * 24 * 60 * 60 * 1000
const MINUTE = 60_000
const STORAGE_PREFIX = 'studio-analytics:v1:'

type ServerAnalytics = {
	samples: Ref<AnalyticsSample[]>
	live: LiveAnalytics
	users: number
}

// Kept on globalThis so hot reloads in dev don't split the collector and the page.
const registry: Map<string, ServerAnalytics> = ((
	globalThis as { __studioAnalytics?: Map<string, ServerAnalytics> }
).__studioAnalytics ??= new Map())

function load(serverId: string): AnalyticsSample[] {
	try {
		const raw = localStorage.getItem(STORAGE_PREFIX + serverId)
		if (!raw) return []
		const parsed = JSON.parse(raw) as AnalyticsSample[]
		const cutoff = Date.now() - RETENTION_MS
		return Array.isArray(parsed) ? parsed.filter((s) => s.t >= cutoff) : []
	} catch {
		return []
	}
}

function save(serverId: string, samples: AnalyticsSample[]) {
	try {
		localStorage.setItem(STORAGE_PREFIX + serverId, JSON.stringify(samples))
	} catch {
		// Storage full or unavailable: keep the in-memory copy only.
	}
}

export function clearServerAnalytics(serverId: string) {
	try {
		localStorage.removeItem(STORAGE_PREFIX + serverId)
	} catch {
		// ignore
	}
	const entry = registry.get(serverId)
	if (entry) entry.samples.value = []
}

function emptyLive(): LiveAnalytics {
	return {
		cpu: null,
		ram: null,
		ramTotal: null,
		storage: null,
		storageTotal: null,
		rxRate: null,
		txRate: null,
		rxTotal: null,
		txTotal: null,
		players: null,
		maxPlayers: null,
		playerNames: [],
		ping: null,
		version: null,
		motd: null,
		pingError: null,
		address: null,
		lastPingAt: null,
	}
}

function entryFor(serverId: string): ServerAnalytics {
	let entry = registry.get(serverId)
	if (!entry) {
		entry = { samples: ref(load(serverId)), live: reactive(emptyLive()), users: 0 }
		registry.set(serverId, entry)
	}
	return entry
}

/** Read-only access for the analytics page. */
export function useServerAnalytics(serverId: string) {
	const entry = entryFor(serverId)
	return { samples: entry.samples, live: entry.live }
}

function motdText(description: unknown): string | null {
	if (!description) return null
	if (typeof description === 'string') return description.replace(/§./g, '')
	const parts: string[] = []
	const walk = (node: unknown) => {
		if (!node) return
		if (typeof node === 'string') {
			parts.push(node)
			return
		}
		const n = node as { text?: string; extra?: unknown[] }
		if (n.text) parts.push(n.text)
		n.extra?.forEach(walk)
	}
	walk(description)
	return parts.join('').replace(/§./g, '') || null
}

/** Address to ping: the hosting address, or the panel host when Wings binds 0.0.0.0. */
function pingAddress(server: Archon.Servers.v0.Server | undefined): string | null {
	if (!server) return null
	const net = server.net
	const ptero = (server as { studio_ptero?: { panel_url?: string | null } }).studio_ptero
	if (ptero && net?.ip && ['0.0.0.0', '::', '127.0.0.1'].includes(net.ip) && !net.domain) {
		try {
			const host = new URL(ptero.panel_url ?? '').hostname
			if (host) return net.port && net.port !== 25565 ? `${host}:${net.port}` : host
		} catch {
			// fall through
		}
	}
	return getHostingServerAddress(net) || null
}

/** Addresses to try in order: the public address, then the node's direct ip:port. */
function pingCandidates(server: Archon.Servers.v0.Server | undefined): string[] {
	const out: string[] = []
	const primary = pingAddress(server)
	if (primary) out.push(primary)
	const net = server?.net
	if (net?.ip && net.port && !['0.0.0.0', '::', '127.0.0.1'].includes(net.ip)) {
		const host = net.ip.includes(':') && !net.ip.startsWith('[') ? `[${net.ip}]` : net.ip
		const direct = `${host}:${net.port}`
		if (!out.includes(direct)) out.push(direct)
	}
	return out
}

/**
 * Starts recording for `serverId` for as long as the calling scope lives.
 * Call it from the server layout so every tab of the server contributes.
 */
export function useServerAnalyticsCollector(
	client: ReturnType<typeof injectModrinthClient>,
	serverId: Ref<string>,
	server: Ref<Archon.Servers.v0.Server | undefined>,
) {
	let stop: (() => void) | null = null

	const start = (id: string) => {
		const entry = entryFor(id)
		entry.users++
		const live = entry.live

		// --- current minute accumulator ---
		let minuteStart = Math.floor(Date.now() / MINUTE) * MINUTE
		let cpuSum = 0
		let ramSum = 0
		let statCount = 0
		let rxBytes = 0
		let txBytes = 0
		let lastNet: { t: number; rx: number; tx: number } | null = null
		const netWindow: { t: number; rx: number; tx: number }[] = []
		let running = false
		let dirty = 0

		const flush = (now: number) => {
			const sample: AnalyticsSample = {
				t: minuteStart,
				cpu: statCount ? +(cpuSum / statCount).toFixed(1) : null,
				ram: statCount ? Math.round(ramSum / statCount) : null,
				ramTotal: live.ramTotal,
				rx: statCount ? Math.round(rxBytes / 60) : null,
				tx: statCount ? Math.round(txBytes / 60) : null,
				players: live.lastPingAt && now - live.lastPingAt < 2 * MINUTE ? live.players : null,
				maxPlayers: live.maxPlayers,
				ping: live.lastPingAt && now - live.lastPingAt < 2 * MINUTE ? live.ping : null,
				up: running || statCount > 0 ? 1 : 0,
			}
			if (sample.cpu !== null || sample.players !== null || sample.up) {
				const cutoff = now - RETENTION_MS
				const next = entry.samples.value.filter((s) => s.t >= cutoff && s.t !== sample.t)
				next.push(sample)
				entry.samples.value = next
				dirty++
				if (dirty >= 5) {
					save(id, next)
					dirty = 0
				}
			}
			minuteStart = Math.floor(now / MINUTE) * MINUTE
			cpuSum = ramSum = statCount = rxBytes = txBytes = 0
		}

		const offStats = client.archon.sockets.on(id, 'stats', (data) => {
			const now = Date.now()
			if (now - minuteStart >= MINUTE) flush(now)
			cpuSum += data.cpu_percent
			ramSum += data.ram_usage_bytes
			statCount++
			live.cpu = data.cpu_percent
			live.ram = data.ram_usage_bytes
			live.ramTotal = data.ram_total_bytes || live.ramTotal
			live.storage = data.storage_usage_bytes
			live.storageTotal = data.storage_total_bytes || live.storageTotal
			live.rxTotal = data.net_rx_bytes
			live.txTotal = data.net_tx_bytes
			if (lastNet) {
				const dt = (now - lastNet.t) / 1000
				const drx = data.net_rx_bytes - lastNet.rx
				const dtx = data.net_tx_bytes - lastNet.tx
				// Counters reset when the container restarts.
				if (dt > 0 && drx >= 0 && dtx >= 0) {
					rxBytes += drx
					txBytes += dtx
				} else {
					netWindow.length = 0
				}
			}
			lastNet = { t: now, rx: data.net_rx_bytes, tx: data.net_tx_bytes }
			// Live rate over the last ~15 s: counters update in bursts, so a single
			// delta is often 0 even when there's traffic.
			netWindow.push(lastNet)
			while (netWindow.length > 2 && now - netWindow[0].t > 15_000) netWindow.shift()
			if (netWindow.length >= 2) {
				const first = netWindow[0]
				const span = (now - first.t) / 1000
				if (span > 0) {
					live.rxRate = Math.max(0, (data.net_rx_bytes - first.rx) / span)
					live.txRate = Math.max(0, (data.net_tx_bytes - first.tx) / span)
				}
			}
		})

		const offPower = client.archon.sockets.on(id, 'power-state', (data) => {
			running = data.state === 'running' || data.state === 'starting'
			if (!running) {
				live.rxRate = live.txRate = 0
				lastNet = null
				netWindow.length = 0
			}
		})

		let pinging = false
		let failures = 0
		let working: string | null = null
		const ping = async () => {
			if (pinging) return
			const candidates = pingCandidates(server.value)
			// Prefer whichever address answered last time.
			if (working && candidates.includes(working)) {
				candidates.splice(candidates.indexOf(working), 1)
				candidates.unshift(working)
			}
			live.address = candidates[0] ?? null
			if (!candidates.length) return
			pinging = true
			let lastError: unknown = null
			try {
				for (const address of candidates) {
					// Modern ping (twice: single pings time out now and then), then the
					// legacy protocol like the app's own server list does.
					const protocols = [null, null, { version: 74, legacy: true }] as const
					for (const protocol of protocols) {
						try {
							const status = await get_server_status(address, protocol)
							working = address
							failures = 0
							live.address = address
							live.players = status.players?.online ?? null
							live.maxPlayers = status.players?.max ?? null
							live.playerNames =
								status.players?.sample?.map((p) => p.name).filter(Boolean) ?? []
							live.ping = status.ping ?? null
							live.version = status.version?.name ?? null
							live.motd = motdText(status.description)
							live.pingError = null
							live.lastPingAt = Date.now()
							running = true
							return
						} catch (error) {
							lastError = error
							console.warn('[analytics] ping failed', address, protocol, error)
						}
					}
				}
				// Keep showing the last good values through a blip; give up after 3 misses.
				failures++
				if (failures >= 3) {
					live.players = null
					live.ping = null
					live.playerNames = []
					live.pingError =
						lastError instanceof Error
							? lastError.message
							: typeof lastError === 'object' && lastError && 'message' in lastError
								? String((lastError as { message: unknown }).message)
								: String(lastError)
				}
			} finally {
				pinging = false
				if (failures > 0 && failures < 3) window.setTimeout(() => void ping(), 10_000)
			}
		}

		void ping()
		// Ping again as soon as the server details (address) arrive or change.
		const stopAddressWatch = watch(
			() => pingAddress(server.value),
			(address, previous) => {
				if (address && address !== previous) void ping()
			},
		)
		const pingTimer = window.setInterval(ping, MINUTE)
		const flushTimer = window.setInterval(() => {
			const now = Date.now()
			if (now - minuteStart >= MINUTE) flush(now)
		}, 15_000)

		return () => {
			offStats()
			offPower()
			stopAddressWatch()
			window.clearInterval(pingTimer)
			window.clearInterval(flushTimer)
			flush(Date.now())
			save(id, entry.samples.value)
			entry.users--
		}
	}

	watch(
		serverId,
		(id) => {
			stop?.()
			stop = id ? start(id) : null
		},
		{ immediate: true },
	)

	onScopeDispose(() => {
		stop?.()
		stop = null
	})
}
