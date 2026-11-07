/**
 * Modrinth Studios: player & server history from the server's own log files
 * (`/logs/latest.log` and the rotated `YYYY-MM-DD-N.log.gz` files). This works
 * for any server, Modrinth Hosting or Pterodactyl, and goes back as far as the
 * logs do, independent of whether Studio was open at the time.
 */
import type { injectModrinthClient } from '@modrinth/ui'

type Client = ReturnType<typeof injectModrinthClient>

export type LogEvent =
	| { t: number; kind: 'join'; player: string }
	| { t: number; kind: 'leave'; player: string }
	| { t: number; kind: 'chat'; player: string }
	| { t: number; kind: 'command'; player: string }
	| { t: number; kind: 'death'; player: string }
	| { t: number; kind: 'advancement'; player: string; name: string }
	| { t: number; kind: 'start'; seconds: number }
	| { t: number; kind: 'stop' }
	| { t: number; kind: 'lag'; ms: number }
	| { t: number; kind: 'error' }
	| { t: number; kind: 'warn' }
	| { t: number; kind: 'crash' }

export type Session = { player: string; start: number; end: number; ongoing: boolean }

export type PlayerStats = {
	name: string
	sessions: number
	playtimeMs: number
	firstSeen: number
	lastSeen: number
	chat: number
	commands: number
	deaths: number
	advancements: number
	online: boolean
}

export type DayStats = {
	day: string
	unique: number
	joins: number
	peak: number
	playtimeMs: number
	chat: number
	deaths: number
	lagSpikes: number
	errors: number
	restarts: number
}

export type LogAnalytics = {
	from: number
	to: number
	filesRead: number
	players: PlayerStats[]
	days: DayStats[]
	sessions: Session[]
	/** [dayOfWeek 0=Mon][hour] -> player-minutes */
	heatmap: number[][]
	starts: { t: number; seconds: number }[]
	advancements: { t: number; player: string; name: string }[]
	totals: {
		unique: number
		joins: number
		playtimeMs: number
		avgSessionMs: number
		peak: number
		peakAt: number | null
		chat: number
		commands: number
		deaths: number
		advancements: number
		restarts: number
		crashes: number
		lagSpikes: number
		lagMs: number
		errors: number
		warnings: number
	}
}

type LogFile = { name: string; path: string; size: number; modified: number; date: Date | null }

const fileCache = new Map<string, LogEvent[]>()

const MONTHS: Record<string, number> = {
	jan: 0,
	feb: 1,
	mar: 2,
	apr: 3,
	may: 4,
	jun: 5,
	jul: 6,
	aug: 7,
	sep: 8,
	oct: 9,
	nov: 10,
	dec: 11,
}

const DEATH_PATTERNS = [
	' was slain by',
	' was shot by',
	' was killed',
	' drowned',
	' fell ',
	' hit the ground too hard',
	' burned to death',
	' went up in flames',
	' walked into fire',
	' tried to swim in lava',
	' discovered the floor was lava',
	' was blown up',
	' blew up',
	' starved to death',
	' suffocated in a wall',
	' was squished',
	' was squashed',
	' froze to death',
	' withered away',
	' was pricked to death',
	' experienced kinetic energy',
	' was impaled',
	' was fireballed',
	' was struck by lightning',
	' fell out of the world',
	' was doomed to fall',
	' died',
	' was stung to death',
	' was obliterated',
	' was poked to death',
	' was skewered',
]

const NAME = String.raw`\.?[A-Za-z0-9_]{1,16}`
const RE = {
	join: new RegExp(String.raw`(?:^|\s)(${NAME}) joined the game`),
	leave: new RegExp(String.raw`(?:^|\s)(${NAME}) left the game`),
	// Always logged by the server, even when join/leave messages are changed or hidden by mods/plugins.
	loggedIn: new RegExp(String.raw`^(${NAME})\[[^\]]*\] logged in with entity id`),
	lostConnection: new RegExp(String.raw`^(${NAME}) lost connection: `),
	chat: new RegExp(String.raw`^(?:\[Not Secure\] )?<(${NAME})> `),
	command: new RegExp(String.raw`^(${NAME}) issued server command: `),
	advancement: new RegExp(
		String.raw`(?:^|[\s>\]])(${NAME}) has (?:made the advancement|completed the challenge|reached the goal|just earned the achievement) \[(.+?)\]`,
	),
	done: /^Done \(([\d.]+)s\)!/,
	stop: /^Stopping (?:the )?server/,
	lag: /Can't keep up!.*?Running (\d+)ms or (\d+) ticks behind/,
	crash: /This crash report has been saved to|Encountered an unexpected exception/,
}

function parseFileDate(name: string): Date | null {
	const m = name.match(/^(\d{4})-(\d{2})-(\d{2})/)
	return m ? new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3])) : null
}

/** Timestamp at the start of a line, as ms (date from the file for time-only stamps). */
function lineTime(line: string, base: Date): number | null {
	let m = line.match(/^\[(\d{2}):(\d{2}):(\d{2})/)
	if (m) {
		const d = new Date(base)
		d.setHours(Number(m[1]), Number(m[2]), Number(m[3]), 0)
		return d.getTime()
	}
	m = line.match(/^\[(\d{2})([A-Za-z]{3})(\d{4}) (\d{2}):(\d{2}):(\d{2})/)
	if (m) {
		return new Date(
			Number(m[3]),
			MONTHS[m[2].toLowerCase()] ?? 0,
			Number(m[1]),
			Number(m[4]),
			Number(m[5]),
			Number(m[6]),
		).getTime()
	}
	m = line.match(/^\[?(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2}):(\d{2})/)
	if (m) {
		return new Date(
			Number(m[1]),
			Number(m[2]) - 1,
			Number(m[3]),
			Number(m[4]),
			Number(m[5]),
			Number(m[6]),
		).getTime()
	}
	return null
}

/** Players seen in any parsed log (shared across files). */
const known = new Set<string>()

const DEATH_RE = new RegExp(
	String.raw`^(?:\[[^\]]*\]\s*)*(${NAME})(${DEATH_PATTERNS.map((p) => p.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})`,
)

export function parseLog(text: string, baseDate: Date): LogEvent[] {
	return parseLogDetailed(text, baseDate).events
}

/** Like parseLog, plus the timestamp of the last line (for timezone detection). */
export function parseLogDetailed(text: string, baseDate: Date): { events: LogEvent[]; lastT: number } {
	const events: LogEvent[] = []
	const online = new Set<string>()
	let base = new Date(baseDate)
	base.setHours(0, 0, 0, 0)
	let lastT = 0

	for (const rawLine of text.split(/\r?\n/)) {
		if (!rawLine) continue
		const line = rawLine.replace(/\u001b\[[0-9;]*m/g, '').replace(/§[0-9a-fk-or]/gi, '')
		let t = lineTime(line, base)
		if (t === null) continue
		// Time-only logs: wrap past midnight.
		if (lastT && t < lastT - 60 * 60 * 1000 && /^\[\d{2}:/.test(line)) {
			base = new Date(base.getTime() + 24 * 60 * 60 * 1000)
			t += 24 * 60 * 60 * 1000
		}
		lastT = t

		const sep = line.indexOf(']: ')
		const msg = sep === -1 ? '' : line.slice(sep + 3).trim()
		const header = sep === -1 ? line : line.slice(0, sep)

		if (/\/ERROR\]|\[ERROR\]/.test(header)) events.push({ t, kind: 'error' })
		else if (/\/WARN\]|\[WARN\]/.test(header)) events.push({ t, kind: 'warn' })
		if (!msg) continue

		let m: RegExpMatchArray | null
		if ((m = msg.match(RE.loggedIn)) || (m = msg.match(RE.join))) {
			known.add(m[1])
			if (!online.has(m[1])) {
				online.add(m[1])
				events.push({ t, kind: 'join', player: m[1] })
			}
		} else if ((m = msg.match(RE.lostConnection)) || (m = msg.match(RE.leave))) {
			if (online.has(m[1])) {
				online.delete(m[1])
				events.push({ t, kind: 'leave', player: m[1] })
			}
		} else if ((m = msg.match(RE.chat))) {
			events.push({ t, kind: 'chat', player: m[1] })
		} else if ((m = msg.match(RE.command))) {
			events.push({ t, kind: 'command', player: m[1] })
		} else if ((m = msg.match(RE.advancement))) {
			events.push({ t, kind: 'advancement', player: m[1], name: m[2] })
		} else if ((m = msg.match(RE.done))) {
			events.push({ t, kind: 'start', seconds: Number(m[1]) })
		} else if (RE.stop.test(msg)) {
			online.clear()
			events.push({ t, kind: 'stop' })
		} else if ((m = msg.match(RE.lag))) {
			events.push({ t, kind: 'lag', ms: Number(m[1]) })
		} else if (RE.crash.test(msg)) {
			events.push({ t, kind: 'crash' })
		} else {
			// Death messages: "<player> <vanilla death phrase>". The player must be someone
			// seen on the server (in any log), or at least look like a player name; the
			// online check alone misses players whose session started in an older log.
			const dm = msg.match(DEATH_RE)
			if (dm && (online.has(dm[1]) || known.has(dm[1]) || /^\.?[A-Za-z0-9_]{3,16}$/.test(dm[1]))) {
				events.push({ t, kind: 'death', player: dm[1] })
			}
		}
	}
	return { events, lastT }
}

async function blobText(blob: Blob, gzip: boolean): Promise<string> {
	if (!gzip) return blob.text()
	const stream = blob.stream().pipeThrough(new DecompressionStream('gzip'))
	return new Response(stream).text()
}

async function listLogs(client: Client): Promise<LogFile[]> {
	const files: LogFile[] = []
	let page = 1
	for (;;) {
		const res = await client.kyros.files_v0.listDirectory('/logs', page, 200)
		for (const item of res.items) {
			if (item.type !== 'file') continue
			if (item.name !== 'latest.log' && !/\.log(\.gz)?$/.test(item.name)) continue
			if (/debug/i.test(item.name)) continue
			files.push({
				name: item.name,
				path: `/logs/${item.name}`,
				size: item.size ?? 0,
				modified: (item.modified ?? 0) * (item.modified > 1e12 ? 1 : 1000),
				date: parseFileDate(item.name),
			})
		}
		if (page >= (res.total ?? 1) || res.items.length === 0) break
		page++
	}
	return files
}

/**
 * Reads and parses the logs covering the last `days` days.
 * `onProgress` gets (done, total) as files are read.
 */
export async function readLogAnalytics(
	client: Client,
	serverId: string,
	days: number,
	isRunning: boolean,
	onProgress?: (done: number, total: number) => void,
): Promise<LogAnalytics> {
	const now = Date.now()
	const from = now - days * 24 * 60 * 60 * 1000
	const all = await listLogs(client)
	const relevant = all
		.filter((f) => {
			const when = f.date?.getTime() ?? f.modified
			return when + 24 * 60 * 60 * 1000 >= from
		})
		.sort((a, b) => {
			// Oldest first; latest.log last.
			if (a.name === 'latest.log') return 1
			if (b.name === 'latest.log') return -1
			return a.name.localeCompare(b.name, undefined, { numeric: true })
		})

	const events: LogEvent[] = []
	let done = 0
	onProgress?.(0, relevant.length)

	// Log timestamps are in the *server's* timezone (often UTC in a container),
	// but get parsed as local time. Work out the difference from latest.log:
	// its last line was written at (about) the file's modification time.
	let tzOffset = 0
	const day = 86_400_000
	const latest = relevant.find((f) => f.name === 'latest.log')
	let latestEvents: LogEvent[] = []
	if (latest) {
		try {
			const text = await blobText(await client.kyros.files_v0.downloadFile(latest.path), false)
			const { events: parsed, lastT } = parseLogDetailed(text, new Date(latest.modified || now))
			if (lastT && latest.modified) {
				let raw = (latest.modified - lastT) % day
				if (raw > day / 2) raw -= day
				if (raw < -day / 2) raw += day
				const quarter = 15 * 60_000
				const rounded = Math.round(raw / quarter) * quarter
				// Only trust it when it's clearly a timezone, not just a quiet log.
				if (Math.abs(rounded) <= 14 * 3_600_000 && Math.abs(raw - rounded) < 5 * 60_000) {
					tzOffset = rounded
				}
			}
			latestEvents = parsed.map((e) => ({ ...e, t: e.t + tzOffset }))
			if (latestEvents.length) {
				// If the log spans past midnight the stamps already wrapped; shift so
				// the last event isn't in the future.
				const last = latestEvents[latestEvents.length - 1].t
				const shift = last > now + 60_000 ? Math.ceil((last - now) / day) * day : 0
				if (shift) latestEvents = latestEvents.map((e) => ({ ...e, t: e.t - shift }))
			}
		} catch (error) {
			console.warn('[analytics] could not read', latest.path, error)
		}
		onProgress?.(++done, relevant.length)
	}

	for (const file of relevant) {
		if (file === latest) continue
		const key = `${serverId}:${file.name}:${file.size}:${file.modified}`
		let parsed = fileCache.get(key)
		if (!parsed) {
			try {
				const blob = await client.kyros.files_v0.downloadFile(file.path)
				const text = await blobText(blob, file.name.endsWith('.gz'))
				parsed = parseLog(text, file.date ?? new Date(file.modified))
				fileCache.set(key, parsed)
			} catch (error) {
				console.warn('[analytics] could not read', file.path, error)
				parsed = []
			}
		}
		for (const e of parsed) events.push(tzOffset ? { ...e, t: e.t + tzOffset } : e)
		onProgress?.(++done, relevant.length)
	}
	events.push(...latestEvents)

	return summarize(events, from, now, isRunning, relevant.length)
}

function dayKey(t: number) {
	const d = new Date(t)
	return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

export function summarize(
	raw: LogEvent[],
	from: number,
	to: number,
	isRunning: boolean,
	filesRead: number,
): LogAnalytics {
	const events = [...raw].sort((a, b) => a.t - b.t)
	const sessions: Session[] = []
	const open = new Map<string, number>()
	const players = new Map<string, PlayerStats>()
	const days = new Map<string, DayStats & { set: Set<string> }>()
	const starts: { t: number; seconds: number }[] = []
	const advancements: { t: number; player: string; name: string }[] = []
	let peak = 0
	let peakAt: number | null = null
	const totals = {
		chat: 0,
		commands: 0,
		deaths: 0,
		advancements: 0,
		restarts: 0,
		crashes: 0,
		lagSpikes: 0,
		lagMs: 0,
		errors: 0,
		warnings: 0,
		joins: 0,
	}

	const day = (t: number) => {
		const k = dayKey(t)
		let d = days.get(k)
		if (!d) {
			d = {
				day: k,
				unique: 0,
				joins: 0,
				peak: 0,
				playtimeMs: 0,
				chat: 0,
				deaths: 0,
				lagSpikes: 0,
				errors: 0,
				restarts: 0,
				set: new Set(),
			}
			days.set(k, d)
		}
		return d
	}
	const player = (name: string, t: number) => {
		let p = players.get(name)
		if (!p) {
			p = {
				name,
				sessions: 0,
				playtimeMs: 0,
				firstSeen: t,
				lastSeen: t,
				chat: 0,
				commands: 0,
				deaths: 0,
				advancements: 0,
				online: false,
			}
			players.set(name, p)
		}
		p.lastSeen = Math.max(p.lastSeen, t)
		return p
	}
	const close = (name: string, end: number, ongoing = false) => {
		const start = open.get(name)
		if (start === undefined) return
		open.delete(name)
		sessions.push({ player: name, start, end, ongoing })
	}
	const inRange = (t: number) => t >= from && t <= to

	for (const e of events) {
		switch (e.kind) {
			case 'join':
				if (open.has(e.player)) close(e.player, e.t)
				open.set(e.player, e.t)
				if (inRange(e.t)) {
					totals.joins++
					const d = day(e.t)
					d.joins++
					d.set.add(e.player)
					player(e.player, e.t).sessions++
					if (open.size > peak) {
						peak = open.size
						peakAt = e.t
					}
					d.peak = Math.max(d.peak, open.size)
				}
				break
			case 'leave':
				close(e.player, e.t)
				break
			case 'stop':
			case 'start':
				for (const name of [...open.keys()]) close(name, e.t)
				if (e.kind === 'start') {
					if (inRange(e.t)) {
						starts.push({ t: e.t, seconds: e.seconds })
						totals.restarts++
						day(e.t).restarts++
					}
				}
				break
			default:
				if (!inRange(e.t)) break
				switch (e.kind) {
					case 'chat':
						totals.chat++
						day(e.t).chat++
						player(e.player, e.t).chat++
						break
					case 'command':
						totals.commands++
						player(e.player, e.t).commands++
						break
					case 'death':
						totals.deaths++
						day(e.t).deaths++
						player(e.player, e.t).deaths++
						break
					case 'advancement':
						totals.advancements++
						player(e.player, e.t).advancements++
						advancements.push({ t: e.t, player: e.player, name: e.name })
						break
					case 'lag':
						totals.lagSpikes++
						totals.lagMs += e.ms
						day(e.t).lagSpikes++
						break
					case 'crash':
						totals.crashes++
						break
					case 'error':
						totals.errors++
						day(e.t).errors++
						break
					case 'warn':
						totals.warnings++
						break
				}
		}
	}
	// Still-open sessions: ongoing if the server is up, otherwise end at the last event.
	const lastT = events.length ? events[events.length - 1].t : to
	for (const name of [...open.keys()]) close(name, isRunning ? to : lastT, isRunning)

	// Sessions → playtime, heatmap, per-day playtime (clipped to the range).
	const heatmap = Array.from({ length: 7 }, () => Array<number>(24).fill(0))
	let playtime = 0
	const ranged: Session[] = []
	for (const s of sessions) {
		const start = Math.max(s.start, from)
		const end = Math.min(s.end, to)
		if (end <= start) continue
		ranged.push(s)
		const dur = end - start
		playtime += dur
		const p = player(s.player, start)
		p.playtimeMs += dur
		p.lastSeen = Math.max(p.lastSeen, end)
		if (s.ongoing) p.online = true
		// Walk the session in 5-minute steps for the heatmap and per-day playtime.
		for (let t = start; t < end; t += 5 * 60_000) {
			const step = Math.min(5 * 60_000, end - t)
			const d = new Date(t)
			heatmap[(d.getDay() + 6) % 7][d.getHours()] += step / 60_000
			day(t).playtimeMs += step
		}
	}

	const dayList = [...days.values()]
		.map(({ set, ...d }) => ({ ...d, unique: set.size }))
		.sort((a, b) => a.day.localeCompare(b.day))
	// Fill empty days so charts have a continuous axis.
	const filled: DayStats[] = []
	for (let t = from; t <= to; t += 86_400_000) {
		const k = dayKey(t)
		filled.push(
			dayList.find((d) => d.day === k) ?? {
				day: k,
				unique: 0,
				joins: 0,
				peak: 0,
				playtimeMs: 0,
				chat: 0,
				deaths: 0,
				lagSpikes: 0,
				errors: 0,
				restarts: 0,
			},
		)
	}
	if (filled[filled.length - 1]?.day !== dayKey(to)) {
		const k = dayKey(to)
		filled.push(dayList.find((d) => d.day === k) ?? { ...filled[0], day: k, unique: 0, joins: 0, peak: 0, playtimeMs: 0, chat: 0, deaths: 0, lagSpikes: 0, errors: 0, restarts: 0 })
	}

	const playerList = [...players.values()]
		.filter((p) => p.sessions > 0 || p.playtimeMs > 0)
		.sort((a, b) => b.playtimeMs - a.playtimeMs)

	return {
		from,
		to,
		filesRead,
		players: playerList,
		days: filled,
		sessions: ranged.sort((a, b) => b.start - a.start),
		heatmap,
		starts,
		advancements: advancements.sort((a, b) => b.t - a.t),
		totals: {
			...totals,
			unique: playerList.length,
			playtimeMs: playtime,
			avgSessionMs: ranged.length ? playtime / ranged.length : 0,
			peak,
			peakAt,
		},
	}
}
