<script setup lang="ts">
/**
 * Modrinth Studios: server analytics (Modrinth Hosting and Pterodactyl).
 *
 * Two sources:
 *  - Recorded: CPU, RAM, network and player count sampled every minute while
 *    Studio has this server open (see features/server-analytics/collector.ts).
 *  - Logs: player sessions, joins, chat, deaths, restarts, lag... parsed from
 *    the server's own log files, so it covers time Studio wasn't open.
 */
import {
	ChartSplineIcon,
	ClockIcon,
	CpuIcon,
	GlobeIcon,
	MemoryStickIcon,
	RefreshCwIcon,
	SignalIcon,
	TrashIcon,
	UsersIcon,
} from '@modrinth/assets'
import { Button, injectModrinthClient, injectModrinthServerContext } from '@modrinth/ui'
import { computed, onMounted, ref, watch } from 'vue'

import AnalyticsChart from '@/features/server-analytics/AnalyticsChart.vue'
import { clearServerAnalytics, useServerAnalytics } from '@/features/server-analytics/collector'
import { type LogAnalytics, readLogAnalytics } from '@/features/server-analytics/logs'
import { readWorldStats, type WorldStats } from '@/features/server-analytics/world-stats'

const client = injectModrinthClient()
const { serverId, isServerRunning, uptimeSeconds } = injectModrinthServerContext()
const { samples, live } = useServerAnalytics(serverId)

type Range = '1h' | '24h' | '7d' | '30d'
const RANGES: { id: Range; label: string; ms: number; days: number }[] = [
	{ id: '1h', label: 'Last hour', ms: 3_600_000, days: 1 },
	{ id: '24h', label: '24 hours', ms: 86_400_000, days: 1 },
	{ id: '7d', label: '7 days', ms: 7 * 86_400_000, days: 7 },
	{ id: '30d', label: '30 days', ms: 30 * 86_400_000, days: 30 },
]
const range = ref<Range>('24h')
const rangeInfo = computed(() => RANGES.find((r) => r.id === range.value)!)

// --- formatting ---------------------------------------------------------------

function formatBytes(bytes: number | null | undefined, digits = 1) {
	if (bytes === null || bytes === undefined || Number.isNaN(bytes)) return '—'
	const units = ['B', 'KB', 'MB', 'GB', 'TB']
	let v = Math.abs(bytes)
	let u = 0
	while (v >= 1024 && u < units.length - 1) {
		v /= 1024
		u++
	}
	return `${(bytes < 0 ? -v : v).toFixed(u === 0 ? 0 : digits)} ${units[u]}`
}
const formatRate = (bps: number | null | undefined) =>
	bps === null || bps === undefined ? '—' : `${formatBytes(bps)}/s`
function formatDuration(ms: number) {
	if (!ms || ms < 0) return '0m'
	const m = Math.floor(ms / 60_000)
	const d = Math.floor(m / 1440)
	const h = Math.floor((m % 1440) / 60)
	const mm = m % 60
	if (d) return `${d}d ${h}h`
	if (h) return `${h}h ${mm}m`
	return `${mm}m`
}
function formatAgo(t: number) {
	const diff = Date.now() - t
	if (diff < 60_000) return 'just now'
	if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`
	if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`
	return `${Math.floor(diff / 86_400_000)}d ago`
}
const fmtDateTime = (t: number) =>
	new Date(t).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
const fmtDay = (t: number) => new Date(t).toLocaleDateString([], { month: 'short', day: 'numeric' })
const fmtTime = (t: number) =>
	new Date(t).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })

// --- recorded samples -------------------------------------------------------------

const bucketed = computed(() => {
	const now = Date.now()
	const rangeFrom = now - Math.min(rangeInfo.value.ms, 7 * 86_400_000)
	// Only draw the part of the range that actually has recordings, so a few
	// minutes of data fill the chart instead of being a speck at the right edge.
	const first = samples.value.find((s) => s.t >= rangeFrom)?.t ?? now
	const from = Math.min(Math.max(rangeFrom, first), now - 10 * 60_000)
	const span = now - from
	const buckets = Math.min(180, Math.max(10, Math.round(span / 60_000)))
	const size = span / buckets
	const out = Array.from({ length: buckets }, (_, i) => ({
		t: from + i * size,
		cpu: [] as number[],
		ram: [] as number[],
		rx: [] as number[],
		tx: [] as number[],
		players: [] as number[],
		ping: [] as number[],
	}))
	for (const s of samples.value) {
		if (s.t < from) continue
		const b = out[Math.min(buckets - 1, Math.floor((s.t - from) / size))]
		if (s.cpu !== null) b.cpu.push(s.cpu)
		if (s.ram !== null) b.ram.push(s.ram)
		if (s.rx !== null) b.rx.push(s.rx)
		if (s.tx !== null) b.tx.push(s.tx)
		if (s.players !== null) b.players.push(s.players)
		if (s.ping !== null) b.ping.push(s.ping)
	}
	const avg = (a: number[]) => (a.length ? a.reduce((x, y) => x + y, 0) / a.length : null)
	const max = (a: number[]) => (a.length ? Math.max(...a) : null)
	return {
		labels: out.map((b) => b.t),
		cpu: out.map((b) => avg(b.cpu)),
		ram: out.map((b) => avg(b.ram)),
		rx: out.map((b) => avg(b.rx)),
		tx: out.map((b) => avg(b.tx)),
		players: out.map((b) => max(b.players)),
		ping: out.map((b) => avg(b.ping)),
	}
})

const recordedInRange = computed(() => {
	const from = Date.now() - Math.min(rangeInfo.value.ms, 7 * 86_400_000)
	return samples.value.filter((s) => s.t >= from)
})

const recordedTotals = computed(() => {
	const list = recordedInRange.value
	let rx = 0
	let tx = 0
	let cpuSum = 0
	let cpuN = 0
	let ramPeak = 0
	let up = 0
	let playersPeak = 0
	for (const s of list) {
		if (s.rx !== null) rx += s.rx * 60
		if (s.tx !== null) tx += s.tx * 60
		if (s.cpu !== null) {
			cpuSum += s.cpu
			cpuN++
		}
		if (s.ram !== null) ramPeak = Math.max(ramPeak, s.ram)
		if (s.up) up++
		if (s.players !== null) playersPeak = Math.max(playersPeak, s.players)
	}
	return {
		minutes: list.length,
		rx,
		tx,
		cpuAvg: cpuN ? cpuSum / cpuN : null,
		ramPeak: ramPeak || null,
		uptimePct: list.length ? (up / list.length) * 100 : null,
		playersPeak,
	}
})

const recordedSpan = computed(() => {
	const labels = bucketed.value.labels
	return labels.length ? Date.now() - labels[0] : 0
})
const axisFormat = computed(() =>
	recordedSpan.value <= 36 * 3_600_000
		? fmtTime
		: (t: number) =>
				new Date(t).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit' }),
)
const recordingSince = computed(() => {
	const first = recordedInRange.value[0]?.t
	return first ? fmtDateTime(first) : null
})

/** now / avg / max for a series, for chart headers. */
function seriesSummary(values: (number | null)[], fmt: (v: number) => string) {
	const vals = values.filter((v): v is number => v !== null)
	if (!vals.length) return null
	const avg = vals.reduce((a, b) => a + b, 0) / vals.length
	return { avg: fmt(avg), max: fmt(Math.max(...vals)) }
}
const summaries = computed(() => ({
	players: seriesSummary(bucketed.value.players, (v) => String(Math.round(v))),
	cpu: seriesSummary(bucketed.value.cpu, (v) => `${v.toFixed(1)}%`),
	ram: seriesSummary(bucketed.value.ram, (v) => formatBytes(v)),
	rx: seriesSummary(bucketed.value.rx, (v) => formatRate(v)),
	tx: seriesSummary(bucketed.value.tx, (v) => formatRate(v)),
	ping: seriesSummary(bucketed.value.ping, (v) => `${Math.round(v)} ms`),
}))

// --- logs ---------------------------------------------------------------------------

const logs = ref<LogAnalytics | null>(null)
const logsLoading = ref(false)
const logsError = ref<string | null>(null)
const logsProgress = ref<{ done: number; total: number } | null>(null)

async function loadLogs() {
	logsLoading.value = true
	logsError.value = null
	try {
		logs.value = await readLogAnalytics(
			client,
			serverId,
			rangeInfo.value.id === '1h' ? 1 : rangeInfo.value.days,
			isServerRunning.value,
			(done, total) => (logsProgress.value = { done, total }),
		)
		if (rangeInfo.value.id === '1h' && logs.value) {
			// Narrow to the last hour for the summary cards.
			logs.value = { ...logs.value }
		}
	} catch (error) {
		logsError.value = error instanceof Error ? error.message : String(error)
	} finally {
		logsLoading.value = false
		logsProgress.value = null
	}
}

const worldStats = ref<WorldStats | null>(null)
const worldStatsLoading = ref(false)
const worldStatsError = ref<string | null>(null)
async function loadWorldStats() {
	worldStatsLoading.value = true
	worldStatsError.value = null
	try {
		worldStats.value = await readWorldStats(client)
	} catch (error) {
		worldStatsError.value = error instanceof Error ? error.message : String(error)
	} finally {
		worldStatsLoading.value = false
	}
}
const showAllWorldPlayers = ref(false)
const worldPlayerRows = computed(() => {
	const list = worldStats.value?.players ?? []
	return showAllWorldPlayers.value ? list : list.slice(0, 10)
})
const formatKm = (cm: number) =>
	cm >= 100_000
		? `${(cm / 100_000).toLocaleString(undefined, { maximumFractionDigits: cm >= 100_000_000 ? 0 : 1 })} km`
		: `${Math.round(cm / 100).toLocaleString()} m`
const formatCount = (n: number) => n.toLocaleString()

// --- records & fun facts (from world stats) ---------------------------------------
type RecordDef = {
	title: string
	value: (p: NonNullable<typeof worldStats.value>['players'][number]) => number
	format: (v: number) => string
	/** Only players with at least this much playtime count (for per-hour records). */
	minPlaytimeMs?: number
	lowest?: boolean
}
const HOUR = 3_600_000
const RECORDS: RecordDef[] = [
	{ title: 'Most playtime', value: (p) => p.playTimeMs, format: (v) => formatDuration(v) },
	{ title: 'Most deaths', value: (p) => p.deaths, format: formatCount },
	{ title: 'Most mob kills', value: (p) => p.mobKills, format: formatCount },
	{ title: 'Most player kills', value: (p) => p.playerKills, format: formatCount },
	{ title: 'Most blocks mined', value: (p) => p.blocksMined, format: formatCount },
	{ title: 'Furthest travelled', value: (p) => p.distanceCm, format: formatKm },
	{ title: 'Most elytra flight', value: (p) => p.distance.elytra, format: formatKm },
	{ title: 'Most items crafted', value: (p) => p.itemsCrafted, format: formatCount },
	{ title: 'Most damage dealt', value: (p) => p.damageDealt, format: (v) => formatCount(Math.round(v)) },
	{ title: 'Most damage taken', value: (p) => p.damageTaken, format: (v) => formatCount(Math.round(v)) },
	{ title: 'Most jumps', value: (p) => p.jumps, format: formatCount },
	{
		title: 'Fastest miner',
		value: (p) => p.blocksMined / (p.playTimeMs / HOUR),
		format: (v) => `${formatCount(Math.round(v))} blocks/h`,
		minPlaytimeMs: 10 * HOUR,
	},
	{
		title: 'Deadliest',
		value: (p) => p.mobKills / (p.playTimeMs / HOUR),
		format: (v) => `${formatCount(Math.round(v))} kills/h`,
		minPlaytimeMs: 10 * HOUR,
	},
	{
		title: 'Best survivor',
		value: (p) => p.playTimeMs / HOUR / Math.max(1, p.deaths),
		format: (v) => `${v.toFixed(1)} h per death`,
		minPlaytimeMs: 10 * HOUR,
	},
	{
		title: 'Most accident-prone',
		value: (p) => p.deaths / (p.playTimeMs / HOUR),
		format: (v) => `${v.toFixed(2)} deaths/h`,
		minPlaytimeMs: 10 * HOUR,
	},
]

const records = computed(() => {
	const players = worldStats.value?.players ?? []
	return RECORDS.map((r) => {
		const eligible = players.filter((p) => p.playTimeMs >= (r.minPlaytimeMs ?? 0))
		let best: (typeof players)[number] | null = null
		let bestValue = r.lowest ? Infinity : -Infinity
		for (const p of eligible) {
			const v = r.value(p)
			if (!Number.isFinite(v)) continue
			if (r.lowest ? v < bestValue : v > bestValue) {
				best = p
				bestValue = v
			}
		}
		return best && bestValue > 0 ? { title: r.title, player: best, value: r.format(bestValue) } : null
	}).filter((r): r is NonNullable<typeof r> => r !== null)
})

type FunFact = { value: string; label: string; detail?: string; progress?: number; color: string }
const funFacts = computed<FunFact[]>(() => {
	const t = worldStats.value?.totals
	const count = worldStats.value?.players.length ?? 0
	if (!t) return []
	const km = t.distanceCm / 100_000
	const flightKm = t.distance.creativeFlight / 100_000
	const hours = t.playTimeMs / HOUR
	const facts: FunFact[] = []
	if (t.playTimeMs > 0) {
		const years = t.playTimeMs / (365.25 * 24 * HOUR)
		facts.push(
			years >= 1
				? { value: years.toFixed(1), label: 'years played', detail: 'all players combined', color: 'var(--color-brand)' }
				: { value: (hours / 24).toFixed(0), label: 'days played', detail: 'all players combined', color: 'var(--color-brand)' },
		)
	}
	if (km > 0) {
		const laps = km / 40_075
		facts.push({
			value: laps >= 1 ? `${laps.toFixed(1)}×` : `${(laps * 100).toFixed(0)}%`,
			label: 'around the Earth',
			detail: 'walking, flying, sailing',
			progress: laps >= 1 ? laps % 1 : laps,
			color: 'var(--color-blue)',
		})
		const moon = (km + flightKm) / 384_400
		facts.push({
			value: `${(moon * 100).toFixed(0)}%`,
			label: 'of the way to the Moon',
			detail: 'with creative flight',
			progress: Math.min(1, moon),
			color: 'var(--color-purple)',
		})
	}
	if (t.blocksMined > 0) {
		facts.push({
			value: formatCount(Math.round(t.blocksMined / 98_304)),
			label: 'chunks dug out',
			detail: 'build limit to bedrock',
			color: 'var(--color-orange)',
		})
	}
	if (t.jumps > 0) {
		facts.push({
			value: `${Math.round((t.jumps * 1.25) / 8_849)}×`,
			label: 'Mount Everest',
			detail: 'stacked up in jumps',
			color: 'var(--color-green)',
		})
	}
	if (t.deaths > 0 && t.playTimeMs > 0) {
		facts.push({
			value: formatDuration(t.playTimeMs / t.deaths),
			label: 'between deaths',
			detail: 'on average',
			color: 'var(--color-red)',
		})
	}
	if (t.mobKills > 0 && hours > 0) {
		facts.push({
			value: formatCount(Math.round(t.mobKills / hours)),
			label: 'mobs per hour',
			detail: 'server-wide',
			color: 'var(--color-red)',
		})
	}
	if (t.itemsCrafted > 0 && count) {
		facts.push({
			value: formatCount(Math.round(t.itemsCrafted / count)),
			label: 'crafts per player',
			color: 'var(--color-orange)',
		})
	}
	return facts
})

onMounted(() => {
	void loadLogs()
	void loadWorldStats()
})
watch(range, loadLogs)

const dailyLabels = computed(() =>
	(logs.value?.days ?? []).map((d) => new Date(`${d.day}T12:00:00`).getTime()),
)

const DOW = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']
const heatMax = computed(() => Math.max(1, ...(logs.value?.heatmap.flat() ?? [0])))

const busiestHour = computed(() => {
	const h = logs.value?.heatmap
	if (!h) return null
	let best = { d: 0, h: 0, v: 0 }
	h.forEach((row, d) =>
		row.forEach((v, hr) => {
			if (v > best.v) best = { d, h: hr, v }
		}),
	)
	return best.v ? `${DOW[best.d]} ${String(best.h).padStart(2, '0')}:00` : null
})

const avgStartup = computed(() => {
	const s = logs.value?.starts ?? []
	return s.length ? s.reduce((a, b) => a + b.seconds, 0) / s.length : null
})

const showAllPlayers = ref(false)
const playerRows = computed(() => {
	const list = logs.value?.players ?? []
	return showAllPlayers.value ? list : list.slice(0, 10)
})

function clearRecorded() {
	if (window.confirm('Clear the recorded CPU, RAM, network and player history for this server?')) {
		clearServerAnalytics(serverId)
	}
}

const memoryPct = computed(() =>
	live.ram !== null && live.ramTotal ? (live.ram / live.ramTotal) * 100 : null,
)
const storagePct = computed(() =>
	live.storage !== null && live.storageTotal ? (live.storage / live.storageTotal) * 100 : null,
)
</script>

<template>
	<div class="flex flex-col gap-6 pb-8">
		<!-- header -->
		<div class="flex flex-wrap items-center justify-between gap-3">
			<div class="flex items-center gap-2">
				<ChartSplineIcon class="size-6 text-brand" />
				<h2 class="m-0 text-2xl font-extrabold text-contrast">Analytics</h2>
			</div>
			<div class="flex flex-wrap items-center gap-2">
				<div class="flex rounded-full bg-button-bg p-1">
					<button
						v-for="r in RANGES"
						:key="r.id"
						class="cursor-pointer rounded-full border-0 px-3 py-1 text-sm font-semibold transition-colors"
						:class="
							range === r.id
								? 'bg-brand text-brand-inverted'
								: 'bg-transparent text-secondary hover:text-contrast'
						"
						@click="range = r.id"
					>
						{{ r.label }}
					</button>
				</div>
				<Button
					:disabled="logsLoading"
					@click="
						() => {
							loadLogs()
							loadWorldStats()
						}
					"
				>
					<RefreshCwIcon :class="logsLoading ? 'animate-spin' : ''" />
					Refresh
				</Button>
			</div>
		</div>

		<!-- live cards -->
		<div class="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-6">
			<div class="stat-card">
				<span class="stat-label"><UsersIcon /> Players online</span>
				<span class="stat-value">
					{{ live.players ?? '—' }}<span v-if="live.maxPlayers" class="stat-sub"> / {{ live.maxPlayers }}</span>
				</span>
				<span
					class="stat-foot truncate"
					:title="live.pingError ?? live.playerNames.join(', ')"
				>
					{{
						live.playerNames.length
							? live.playerNames.join(', ')
							: live.pingError
								? `Not reachable: ${live.pingError}`
								: live.lastPingAt
									? 'Nobody online'
									: 'Checking…'
					}}
				</span>
			</div>
			<div class="stat-card">
				<span class="stat-label"><SignalIcon /> Latency</span>
				<span class="stat-value">{{ live.ping !== null ? `${live.ping} ms` : '—' }}</span>
				<span class="stat-foot truncate" :title="live.address ?? ''">{{ live.address ?? 'No address' }}</span>
			</div>
			<div class="stat-card">
				<span class="stat-label"><CpuIcon /> CPU</span>
				<span class="stat-value">{{ live.cpu !== null ? `${live.cpu.toFixed(1)}%` : '—' }}</span>
				<span class="stat-foot">
					avg {{ recordedTotals.cpuAvg !== null ? `${recordedTotals.cpuAvg.toFixed(1)}%` : '—' }}
				</span>
			</div>
			<div class="stat-card">
				<span class="stat-label"><MemoryStickIcon /> Memory</span>
				<span class="stat-value">{{ formatBytes(live.ram) }}</span>
				<span class="stat-foot">
					{{ memoryPct !== null ? `${memoryPct.toFixed(0)}% of ${formatBytes(live.ramTotal, 0)}` : '—' }}
				</span>
			</div>
			<div class="stat-card">
				<span class="stat-label"><GlobeIcon /> Network</span>
				<span class="stat-value text-base">↓ {{ formatRate(live.rxRate) }}</span>
				<span class="stat-value text-base">↑ {{ formatRate(live.txRate) }}</span>
			</div>
			<div class="stat-card">
				<span class="stat-label"><ClockIcon /> Uptime</span>
				<span class="stat-value">
					{{ isServerRunning ? formatDuration(uptimeSeconds * 1000) : 'Offline' }}
				</span>
				<span class="stat-foot">
					{{ recordedTotals.uptimePct !== null ? `${recordedTotals.uptimePct.toFixed(0)}% up (recorded)` : '—' }}
				</span>
			</div>
		</div>

		<div v-if="live.version || live.motd || storagePct !== null" class="flex flex-wrap gap-x-6 gap-y-1 text-sm text-secondary">
			<span v-if="live.version">Version: <span class="text-contrast">{{ live.version }}</span></span>
			<span v-if="live.motd" class="truncate">MOTD: <span class="text-contrast">{{ live.motd }}</span></span>
			<span v-if="storagePct !== null">
				Disk: <span class="text-contrast">{{ formatBytes(live.storage) }} / {{ formatBytes(live.storageTotal, 0) }}</span>
				({{ storagePct.toFixed(0) }}%)
			</span>
			<span>
				Transferred ({{ rangeInfo.id === '30d' ? '7 days' : rangeInfo.label.toLowerCase() }}):
				<span class="text-contrast">↓ {{ formatBytes(recordedTotals.rx) }} · ↑ {{ formatBytes(recordedTotals.tx) }}</span>
			</span>
		</div>

		<!-- recorded charts -->
		<section class="flex flex-col gap-3">
			<div class="flex flex-wrap items-baseline justify-between gap-2">
				<h3 class="m-0 text-xl font-semibold text-contrast">Performance & players</h3>
				<div class="flex items-center gap-3 text-sm text-secondary">
					<span>
						Recorded every minute while Studio has this server open<template v-if="recordingSince">
							· since {{ recordingSince }} ({{ formatDuration(recordedTotals.minutes * 60_000) }} of data)</template
						><template v-if="rangeInfo.id === '30d'"> · keeps 7 days</template>
					</span>
					<button
						class="flex cursor-pointer items-center gap-1 border-0 bg-transparent p-0 text-secondary hover:text-red"
						@click="clearRecorded"
					>
						<TrashIcon class="size-4" /> Clear
					</button>
				</div>
			</div>
			<div class="grid gap-3 lg:grid-cols-2">
				<div class="chart-card">
					<div class="chart-title">
						<span>Players online <span class="chart-now">{{ live.players ?? '—' }}</span></span>
						<span v-if="summaries.players" class="chart-meta">
							avg {{ summaries.players.avg }} · peak {{ summaries.players.max }}
						</span>
					</div>
					<AnalyticsChart
						integer
						:labels="bucketed.labels"
						:series="[{ name: 'Players', color: 'var(--color-brand)', values: bucketed.players }]"
						:format-value="(v) => String(Math.round(v))"
						:format-label="fmtDateTime"
						:format-axis-label="axisFormat"
					/>
				</div>
				<div class="chart-card">
					<div class="chart-title">
						<span>
							CPU usage
							<span class="chart-now">{{ live.cpu !== null ? `${live.cpu.toFixed(1)}%` : '—' }}</span>
						</span>
						<span v-if="summaries.cpu" class="chart-meta">
							avg {{ summaries.cpu.avg }} · max {{ summaries.cpu.max }}
						</span>
					</div>
					<AnalyticsChart
						:labels="bucketed.labels"
						:series="[{ name: 'CPU', color: 'var(--color-blue)', values: bucketed.cpu }]"
						:format-value="(v) => `${+v.toFixed(v < 10 ? 2 : 0)}%`"
						:format-label="fmtDateTime"
						:format-axis-label="axisFormat"
					/>
				</div>
				<div class="chart-card">
					<div class="chart-title">
						<span>Memory <span class="chart-now">{{ formatBytes(live.ram) }}</span></span>
						<span v-if="summaries.ram" class="chart-meta">
							avg {{ summaries.ram.avg }} · max {{ summaries.ram.max }}
						</span>
					</div>
					<AnalyticsChart
						:labels="bucketed.labels"
						:series="[{ name: 'RAM', color: 'var(--color-purple)', values: bucketed.ram }]"
						:format-value="(v) => formatBytes(v, 1)"
						:format-label="fmtDateTime"
						:format-axis-label="axisFormat"
						:max="live.ramTotal ?? undefined"
					/>
				</div>
				<div class="chart-card">
					<div class="chart-title">
						<span>
							Network
							<span class="chart-now text-green">↓ {{ formatRate(live.rxRate) }}</span>
							<span class="chart-now !ml-3 text-orange">↑ {{ formatRate(live.txRate) }}</span>
						</span>
						<span v-if="summaries.rx" class="chart-meta">
							max ↓ {{ summaries.rx.max }} · ↑ {{ summaries.tx?.max ?? '—' }}
						</span>
					</div>
					<AnalyticsChart
						:labels="bucketed.labels"
						:series="[
							{ name: 'In', color: 'var(--color-green)', values: bucketed.rx },
							{ name: 'Out', color: 'var(--color-orange)', values: bucketed.tx },
						]"
						:format-value="(v) => formatRate(v)"
						:format-label="fmtDateTime"
						:format-axis-label="axisFormat"
					/>
				</div>
				<div class="chart-card lg:col-span-2">
					<div class="chart-title">
						<span>
							Latency
							<span class="chart-now">{{ live.ping !== null ? `${live.ping} ms` : '—' }}</span>
						</span>
						<span v-if="summaries.ping" class="chart-meta">
							avg {{ summaries.ping.avg }} · max {{ summaries.ping.max }}
						</span>
					</div>
					<AnalyticsChart
						:height="120"
						:labels="bucketed.labels"
						:series="[{ name: 'Ping', color: 'var(--color-red)', values: bucketed.ping }]"
						:format-value="(v) => `${Math.round(v)} ms`"
						:format-label="fmtDateTime"
						:format-axis-label="axisFormat"
					/>
				</div>
			</div>
		</section>

		<!-- log analytics -->
		<section class="flex flex-col gap-3">
			<div class="flex flex-wrap items-baseline justify-between gap-2">
				<h3 class="m-0 text-xl font-semibold text-contrast">Player activity</h3>
				<span class="text-sm text-secondary">
					From the server's log files
					<template v-if="logs">· {{ logs.filesRead }} file{{ logs.filesRead === 1 ? '' : 's' }}</template>
					<template v-if="logsProgress"> · reading {{ logsProgress.done }}/{{ logsProgress.total }}</template>
				</span>
			</div>

			<div
				v-if="logsError"
				class="rounded-2xl border border-solid border-red bg-highlight-red p-4 text-sm text-red"
			>
				Couldn't read the server logs: {{ logsError }}
			</div>
			<div v-else-if="!logs && logsLoading" class="h-40 animate-pulse rounded-2xl bg-bg-raised" />

			<template v-if="logs">
				<div class="grid grid-cols-2 gap-3 md:grid-cols-4 xl:grid-cols-6">
					<div class="stat-card">
						<span class="stat-label">Unique players</span>
						<span class="stat-value">{{ logs.totals.unique }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Joins</span>
						<span class="stat-value">{{ logs.totals.joins }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Total playtime</span>
						<span class="stat-value">{{ formatDuration(logs.totals.playtimeMs) }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Avg. session</span>
						<span class="stat-value">{{ formatDuration(logs.totals.avgSessionMs) }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Peak online</span>
						<span class="stat-value">{{ logs.totals.peak }}</span>
						<span v-if="logs.totals.peakAt" class="stat-foot">{{ fmtDateTime(logs.totals.peakAt) }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Busiest time</span>
						<span class="stat-value text-lg">{{ busiestHour ?? '—' }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Chat messages</span>
						<span class="stat-value">{{ logs.totals.chat }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Commands</span>
						<span class="stat-value">{{ logs.totals.commands }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Deaths</span>
						<span class="stat-value">{{ logs.totals.deaths }}</span>
						<span v-if="worldStats" class="stat-foot">
							{{ formatCount(worldStats.totals.deaths) }} all-time (world stats)
						</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Advancements</span>
						<span class="stat-value">{{ logs.totals.advancements }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Restarts</span>
						<span class="stat-value">{{ logs.totals.restarts }}</span>
						<span v-if="avgStartup !== null" class="stat-foot">avg start {{ avgStartup.toFixed(1) }}s</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Lag spikes</span>
						<span class="stat-value">{{ logs.totals.lagSpikes }}</span>
						<span v-if="logs.totals.lagSpikes" class="stat-foot">
							{{ (logs.totals.lagMs / 1000).toFixed(1) }}s behind total
						</span>
					</div>
				</div>

				<div class="grid gap-3 lg:grid-cols-2">
					<div class="chart-card">
						<div class="chart-title">Players per day</div>
						<AnalyticsChart
							type="bar"
							:labels="dailyLabels"
							:series="[
								{ name: 'Unique players', color: 'var(--color-brand)', values: logs.days.map((d) => d.unique) },
								{ name: 'Joins', color: 'var(--color-blue)', values: logs.days.map((d) => d.joins) },
							]"
							:format-value="(v) => String(Math.round(v))"
							:format-label="fmtDay"
							:format-axis-label="fmtDay"
						/>
					</div>
					<div class="chart-card">
						<div class="chart-title">Playtime per day</div>
						<AnalyticsChart
							type="bar"
							:labels="dailyLabels"
							:series="[
								{ name: 'Playtime', color: 'var(--color-purple)', values: logs.days.map((d) => d.playtimeMs / 3_600_000) },
							]"
							:format-value="(v) => `${v.toFixed(1)}h`"
							:format-label="fmtDay"
							:format-axis-label="fmtDay"
						/>
					</div>
					<div class="chart-card">
						<div class="chart-title">Chat & deaths per day</div>
						<AnalyticsChart
							type="bar"
							:labels="dailyLabels"
							:series="[
								{ name: 'Chat', color: 'var(--color-green)', values: logs.days.map((d) => d.chat) },
								{ name: 'Deaths', color: 'var(--color-red)', values: logs.days.map((d) => d.deaths) },
							]"
							:format-value="(v) => String(Math.round(v))"
							:format-label="fmtDay"
							:format-axis-label="fmtDay"
						/>
					</div>
					<div class="chart-card">
						<div class="chart-title">Server health per day</div>
						<AnalyticsChart
							type="bar"
							:labels="dailyLabels"
							:series="[
								{ name: 'Lag spikes', color: 'var(--color-orange)', values: logs.days.map((d) => d.lagSpikes) },
								{ name: 'Restarts', color: 'var(--color-blue)', values: logs.days.map((d) => d.restarts) },
								{ name: 'Errors', color: 'var(--color-red)', values: logs.days.map((d) => d.errors) },
							]"
							:format-value="(v) => String(Math.round(v))"
							:format-label="fmtDay"
							:format-axis-label="fmtDay"
						/>
					</div>
				</div>

				<!-- heatmap -->
				<div class="chart-card">
					<div class="chart-title">
						When players are online
						<span class="text-secondary">player-minutes by weekday and hour</span>
					</div>
					<div class="overflow-x-auto">
						<div class="grid min-w-[640px] gap-[3px]" style="grid-template-columns: 2.5rem repeat(24, 1fr)">
							<span />
							<span v-for="h in 24" :key="`h${h}`" class="text-center text-[11px] text-secondary">
								{{ (h - 1) % 3 === 0 ? String(h - 1).padStart(2, '0') : '' }}
							</span>
							<template v-for="(row, d) in logs.heatmap" :key="`d${d}`">
								<span class="text-[11px] leading-5 text-secondary">{{ DOW[d] }}</span>
								<div
									v-for="(v, h) in row"
									:key="`c${d}-${h}`"
									v-tooltip="`${DOW[d]} ${String(h).padStart(2, '0')}:00 · ${Math.round(v)} player-min`"
									class="h-5 rounded-[4px]"
									:style="{
										background: v
											? `color-mix(in srgb, var(--color-brand) ${Math.round(15 + (v / heatMax) * 85)}%, transparent)`
											: 'var(--color-button-bg)',
									}"
								/>
							</template>
						</div>
					</div>
				</div>

				<!-- players -->
				<div class="chart-card">
					<div class="chart-title">
						Players
						<span class="text-secondary">{{ logs.players.length }} total</span>
					</div>
					<div v-if="!logs.players.length" class="py-4 text-secondary">
						No players joined in this period.
					</div>
					<div v-else class="overflow-x-auto">
						<table class="w-full border-collapse text-sm">
							<thead>
								<tr class="text-left text-secondary">
									<th class="py-2 pr-3 font-semibold">Player</th>
									<th class="py-2 pr-3 font-semibold">Playtime</th>
									<th class="py-2 pr-3 font-semibold">Sessions</th>
									<th class="py-2 pr-3 font-semibold">Last seen</th>
									<th class="py-2 pr-3 font-semibold">First seen</th>
									<th class="py-2 pr-3 font-semibold">Chat</th>
									<th class="py-2 pr-3 font-semibold">Deaths</th>
									<th class="py-2 font-semibold">Advancements</th>
								</tr>
							</thead>
							<tbody>
								<tr
									v-for="p in playerRows"
									:key="p.name"
									class="border-0 border-t border-solid border-surface-4"
								>
									<td class="py-2 pr-3">
										<div class="flex items-center gap-2">
											<img
												:src="`https://mc-heads.net/avatar/${encodeURIComponent(p.name.replace(/^\./, ''))}/24`"
												class="size-6 rounded"
												alt=""
												loading="lazy"
											/>
											<span class="font-semibold text-contrast">{{ p.name }}</span>
											<span
												v-if="p.online || live.playerNames.includes(p.name)"
												class="rounded-full bg-highlight-green px-2 text-xs font-semibold text-green"
											>
												online
											</span>
										</div>
									</td>
									<td class="py-2 pr-3 text-contrast">{{ formatDuration(p.playtimeMs) }}</td>
									<td class="py-2 pr-3">{{ p.sessions }}</td>
									<td class="py-2 pr-3">{{ formatAgo(p.lastSeen) }}</td>
									<td class="py-2 pr-3">{{ fmtDay(p.firstSeen) }}</td>
									<td class="py-2 pr-3">{{ p.chat }}</td>
									<td class="py-2 pr-3">{{ p.deaths }}</td>
									<td class="py-2">{{ p.advancements }}</td>
								</tr>
							</tbody>
						</table>
						<button
							v-if="logs.players.length > 10"
							class="mt-2 cursor-pointer border-0 bg-transparent p-0 text-sm font-semibold text-brand"
							@click="showAllPlayers = !showAllPlayers"
						>
							{{ showAllPlayers ? 'Show fewer' : `Show all ${logs.players.length}` }}
						</button>
					</div>
				</div>

				<div class="grid gap-3 lg:grid-cols-2">
					<div class="chart-card">
						<div class="chart-title">Recent sessions</div>
						<div v-if="!logs.sessions.length" class="py-2 text-secondary">No sessions.</div>
						<ul class="m-0 flex list-none flex-col gap-1 p-0 text-sm">
							<li
								v-for="s in logs.sessions.slice(0, 15)"
								:key="`${s.player}-${s.start}`"
								class="flex items-center justify-between gap-3"
							>
								<span class="truncate font-semibold text-contrast">{{ s.player }}</span>
								<span class="whitespace-nowrap text-secondary">
									{{ fmtDateTime(s.start) }} ·
									<span :class="s.ongoing ? 'text-green' : 'text-contrast'">
										{{ s.ongoing ? 'playing for ' : '' }}{{ formatDuration(s.end - s.start) }}
									</span>
								</span>
							</li>
						</ul>
					</div>
					<div class="chart-card">
						<div class="chart-title">Recent advancements</div>
						<div v-if="!logs.advancements.length" class="py-2 text-secondary">None yet.</div>
						<ul class="m-0 flex list-none flex-col gap-1 p-0 text-sm">
							<li
								v-for="a in logs.advancements.slice(0, 15)"
								:key="`${a.player}-${a.t}-${a.name}`"
								class="flex items-center justify-between gap-3"
							>
								<span class="truncate">
									<span class="font-semibold text-contrast">{{ a.player }}</span>
									<span class="text-secondary"> · {{ a.name }}</span>
								</span>
								<span class="whitespace-nowrap text-secondary">{{ formatAgo(a.t) }}</span>
							</li>
						</ul>
					</div>
				</div>

				<div v-if="logs.starts.length" class="chart-card">
					<div class="chart-title">
						Server starts
						<span class="text-secondary">time until "Done"</span>
					</div>
					<AnalyticsChart
						type="bar"
						:height="140"
						:labels="logs.starts.map((s) => s.t)"
						:series="[{ name: 'Startup', color: 'var(--color-blue)', values: logs.starts.map((s) => s.seconds) }]"
						:format-value="(v) => `${v.toFixed(1)}s`"
						:format-label="fmtDateTime"
						:format-axis-label="fmtDay"
					/>
				</div>
			</template>
		</section>

		<!-- all-time world statistics -->
		<section class="flex flex-col gap-3">
			<div class="flex flex-wrap items-baseline justify-between gap-2">
				<h3 class="m-0 text-xl font-semibold text-contrast">All-time player statistics</h3>
				<span class="text-sm text-secondary">
					From the world's statistics files<template v-if="worldStats"> · {{ worldStats.level }}</template>
				</span>
			</div>
			<div v-if="worldStatsLoading && !worldStats" class="h-32 animate-pulse rounded-2xl bg-bg-raised" />
			<div v-else-if="worldStatsError" class="text-secondary">
				Couldn't read the world's statistics: <code>{{ worldStatsError }}</code>
			</div>
			<div v-else-if="!worldStats || !worldStats.players.length" class="text-secondary">
				No player statistics yet. Minecraft writes them to <code>{{ worldStats?.level ?? 'world/stats' }}</code>
				when players log out or the world autosaves.
			</div>
			<template v-else-if="worldStats">
				<div class="grid grid-cols-2 gap-3 md:grid-cols-4 xl:grid-cols-6">
					<div class="stat-card">
						<span class="stat-label">Players</span>
						<span class="stat-value">{{ worldStats.players.length }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Playtime</span>
						<span class="stat-value">{{ formatDuration(worldStats.totals.playTimeMs) }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Deaths</span>
						<span class="stat-value">{{ formatCount(worldStats.totals.deaths) }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Mob kills</span>
						<span class="stat-value">{{ formatCount(worldStats.totals.mobKills) }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Player kills</span>
						<span class="stat-value">{{ formatCount(worldStats.totals.playerKills) }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Blocks mined</span>
						<span class="stat-value">{{ formatCount(worldStats.totals.blocksMined) }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Distance travelled</span>
						<span class="stat-value">{{ formatKm(worldStats.totals.distanceCm) }}</span>
						<span class="stat-foot">
							on foot {{ formatKm(worldStats.totals.distance.onFoot) }} · elytra
							{{ formatKm(worldStats.totals.distance.elytra) }} · vehicles
							{{ formatKm(worldStats.totals.distance.vehicles) }} · swim
							{{ formatKm(worldStats.totals.distance.swimming) }}
						</span>
						<span v-if="worldStats.totals.distance.creativeFlight" class="stat-foot">
							+ {{ formatKm(worldStats.totals.distance.creativeFlight) }} creative flight
						</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Items crafted</span>
						<span class="stat-value">{{ formatCount(worldStats.totals.itemsCrafted) }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Damage dealt</span>
						<span class="stat-value">{{ formatCount(Math.round(worldStats.totals.damageDealt)) }}</span>
						<span class="stat-foot">health points</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Damage taken</span>
						<span class="stat-value">{{ formatCount(Math.round(worldStats.totals.damageTaken)) }}</span>
					</div>
					<div class="stat-card">
						<span class="stat-label">Jumps</span>
						<span class="stat-value">{{ formatCount(worldStats.totals.jumps) }}</span>
					</div>
				</div>

				<div v-if="records.length" class="flex flex-col gap-3">
					<h4 class="m-0 text-lg font-semibold text-contrast">Records</h4>
					<div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5">
						<div v-for="r in records" :key="r.title" class="stat-card">
							<span class="stat-label">{{ r.title }}</span>
							<div class="flex min-w-0 items-center gap-2">
								<img
									:src="`https://mc-heads.net/avatar/${r.player.uuid}/32`"
									class="size-8 shrink-0 rounded-lg"
									alt=""
									loading="lazy"
								/>
								<div class="flex min-w-0 flex-col">
									<span class="truncate font-bold text-contrast">{{ r.player.name }}</span>
									<span class="truncate text-sm font-semibold text-brand">{{ r.value }}</span>
								</div>
							</div>
						</div>
					</div>
				</div>

				<div v-if="funFacts.length" class="grid grid-cols-2 gap-3 md:grid-cols-4">
					<div
						v-for="fact in funFacts"
						:key="fact.label"
						class="flex flex-col rounded-2xl border border-solid border-surface-4 bg-bg-raised p-4"
					>
						<span class="text-3xl font-extrabold leading-none" :style="{ color: fact.color }">
							{{ fact.value }}
						</span>
						<span class="mt-1.5 font-semibold text-contrast">{{ fact.label }}</span>
						<span v-if="fact.detail" class="text-xs text-secondary">{{ fact.detail }}</span>
						<div
							v-if="fact.progress !== undefined"
							class="mt-3 h-1.5 w-full overflow-hidden rounded-full bg-button-bg"
						>
							<div
								class="h-full rounded-full"
								:style="{ width: `${Math.max(2, fact.progress * 100)}%`, background: fact.color }"
							/>
						</div>
					</div>
				</div>

				<div v-if="worldStats.players.length" class="chart-card">
					<div class="overflow-x-auto">
						<table class="w-full border-collapse text-sm">
							<thead>
								<tr class="text-left text-secondary">
									<th class="py-2 pr-3 font-semibold">Player</th>
									<th class="py-2 pr-3 font-semibold">Playtime</th>
									<th class="py-2 pr-3 font-semibold">Deaths</th>
									<th class="py-2 pr-3 font-semibold">Mob kills</th>
									<th class="py-2 pr-3 font-semibold">Player kills</th>
									<th class="py-2 pr-3 font-semibold">Blocks mined</th>
									<th class="py-2 pr-3 font-semibold">Distance</th>
									<th class="py-2 font-semibold">Jumps</th>
								</tr>
							</thead>
							<tbody>
								<tr
									v-for="p in worldPlayerRows"
									:key="p.uuid"
									class="border-0 border-t border-solid border-surface-4"
								>
									<td class="py-2 pr-3">
										<div class="flex items-center gap-2">
											<img
												:src="`https://mc-heads.net/avatar/${p.uuid}/24`"
												class="size-6 rounded"
												alt=""
												loading="lazy"
											/>
											<span class="font-semibold text-contrast">{{ p.name }}</span>
										</div>
									</td>
									<td class="py-2 pr-3 text-contrast">{{ formatDuration(p.playTimeMs) }}</td>
									<td class="py-2 pr-3">{{ formatCount(p.deaths) }}</td>
									<td class="py-2 pr-3">{{ formatCount(p.mobKills) }}</td>
									<td class="py-2 pr-3">{{ formatCount(p.playerKills) }}</td>
									<td class="py-2 pr-3">{{ formatCount(p.blocksMined) }}</td>
									<td class="py-2 pr-3">{{ formatKm(p.distanceCm) }}</td>
									<td class="py-2">{{ formatCount(p.jumps) }}</td>
								</tr>
							</tbody>
						</table>
						<button
							v-if="worldStats.players.length > 10"
							class="mt-2 cursor-pointer border-0 bg-transparent p-0 text-sm font-semibold text-brand"
							@click="showAllWorldPlayers = !showAllWorldPlayers"
						>
							{{ showAllWorldPlayers ? 'Show fewer' : `Show all ${worldStats.players.length}` }}
						</button>
					</div>
				</div>
			</template>
		</section>
	</div>
</template>

<style scoped>
.stat-card {
	@apply flex min-w-0 flex-col gap-1 rounded-2xl border border-solid border-surface-4 bg-bg-raised p-4;
}
.stat-label {
	@apply flex items-center gap-1.5 text-sm font-semibold text-secondary;
}
.stat-label :deep(svg) {
	@apply size-4;
}
.stat-value {
	@apply text-2xl font-extrabold text-contrast;
}
.stat-sub {
	@apply text-base font-semibold text-secondary;
}
.stat-foot {
	@apply text-xs text-secondary;
}
.chart-card {
	@apply flex flex-col gap-3 rounded-2xl border border-solid border-surface-4 bg-bg-raised p-4;
}
.chart-title {
	@apply flex flex-wrap items-baseline justify-between gap-2 font-semibold text-contrast;
}
.chart-now {
	@apply ml-2 text-lg font-extrabold;
}
.chart-meta {
	@apply text-xs font-medium text-secondary;
}
</style>
