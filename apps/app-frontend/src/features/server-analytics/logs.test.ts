import { describe, expect, it } from 'vitest'

import { type LogEvent, parseLog, parseLogDetailed, summarize } from './logs'

const DAY = new Date(2026, 10, 20) // 20 Nov 2026, local time
const at = (h: number, m = 0, s = 0, day = DAY) => {
	const d = new Date(day)
	d.setHours(h, m, s, 0)
	return d.getTime()
}

const kinds = (events: LogEvent[]) => events.map((e) => e.kind)

describe('server log parsing', () => {
	it('reads joins, leaves, chat, commands and advancements', () => {
		const events = parseLog(
			[
				'[10:00:00] [Server thread/INFO]: Done (3.512s)! For help, type "help"',
				'[10:01:00] [Server thread/INFO]: Steve[/1.2.3.4:5555] logged in with entity id 12 at (0.5, 64.0, 0.5)',
				'[10:01:00] [Server thread/INFO]: Steve joined the game',
				'[10:02:00] [Async Chat Thread - #0/INFO]: <Steve> hello',
				'[10:02:30] [Server thread/INFO]: [Not Secure] <Steve> unsigned hi',
				'[10:03:00] [Server thread/INFO]: Steve issued server command: /home',
				'[10:04:00] [Server thread/INFO]: Steve has made the advancement [Stone Age]',
				'[10:05:00] [Server thread/INFO]: Steve lost connection: Disconnected',
				'[10:05:00] [Server thread/INFO]: Steve left the game',
			].join('\n'),
			DAY,
		)
		expect(kinds(events)).toEqual(['start', 'join', 'chat', 'chat', 'command', 'advancement', 'leave'])
		expect(events[0]).toMatchObject({ kind: 'start', seconds: 3.512, t: at(10) })
		expect(events[5]).toMatchObject({ player: 'Steve', name: 'Stone Age' })
	})

	it('counts one join even when both join lines are logged', () => {
		const events = parseLog(
			[
				'[10:00:00] [Server thread/INFO]: Alex[/1.1.1.1:1] logged in with entity id 1 at (0, 0, 0)',
				'[10:00:00] [Server thread/INFO]: Alex joined the game',
			].join('\n'),
			DAY,
		)
		expect(kinds(events)).toEqual(['join'])
	})

	it('reads deaths, lag spikes, crashes, warnings, errors and stops', () => {
		const events = parseLog(
			[
				'[11:00:00] [Server thread/INFO]: Alex joined the game',
				'[11:00:10] [Server thread/INFO]: Alex was slain by Zombie',
				'[11:00:20] [Server thread/INFO]: Alex fell from a high place',
				"[11:00:30] [Server thread/WARN]: Can't keep up! Is the server overloaded? Running 2500ms or 50 ticks behind",
				'[11:00:40] [Server thread/ERROR]: Something broke',
				'[11:00:50] [Server thread/ERROR]: This crash report has been saved to: ./crash.txt',
				'[11:01:00] [Server thread/INFO]: Stopping the server',
			].join('\n'),
			DAY,
		)
		expect(kinds(events)).toEqual([
			'join',
			'death',
			'death',
			'warn',
			'lag',
			'error',
			'error',
			'crash',
			'stop',
		])
		expect(events.find((e) => e.kind === 'lag')).toMatchObject({ ms: 2500 })
	})

	it('strips colour codes and handles full-date timestamps', () => {
		const events = parseLog(
			[
				'\u001b[32m[11:00:00] [Server thread/INFO]: §aBob§r joined the game\u001b[0m',
				'[20Nov2026 11:05:00.123] [Server thread/INFO] [minecraft/MinecraftServer]: Bob left the game',
				'2026-11-20 11:06:00 [Server thread/INFO]: Done (1.0s)! For help',
			].join('\n'),
			DAY,
		)
		expect(kinds(events)).toEqual(['join', 'leave', 'start'])
		expect(events[1].t).toBe(at(11, 5))
	})

	it('moves to the next day when the log passes midnight', () => {
		const { events, lastT } = parseLogDetailed(
			[
				'[23:59:00] [Server thread/INFO]: Steve joined the game',
				'[00:10:00] [Server thread/INFO]: Steve left the game',
			].join('\n'),
			DAY,
		)
		const nextDay = new Date(DAY.getTime() + 86_400_000)
		expect(events[1].t).toBe(at(0, 10, 0, nextDay))
		expect(lastT).toBe(events[1].t)
	})

	it('ignores leaves of players that never joined and lines without a time', () => {
		const events = parseLog(
			['Steve left the game', '[10:00:00] [Server thread/INFO]: Ghost left the game'].join('\n'),
			DAY,
		)
		expect(events).toEqual([])
	})
})

describe('server log summary', () => {
	const from = at(0)
	const to = at(23, 59)

	it('builds sessions, playtime, peaks and per-player stats', () => {
		const events: LogEvent[] = [
			{ t: at(10), kind: 'start', seconds: 4 },
			{ t: at(10, 5), kind: 'join', player: 'Steve' },
			{ t: at(10, 10), kind: 'join', player: 'Alex' },
			{ t: at(10, 20), kind: 'chat', player: 'Alex' },
			{ t: at(10, 30), kind: 'death', player: 'Steve' },
			{ t: at(11, 5), kind: 'leave', player: 'Steve' },
			{ t: at(11, 10), kind: 'leave', player: 'Alex' },
		]
		const s = summarize(events, from, to, false, 1)
		expect(s.totals).toMatchObject({
			unique: 2,
			joins: 2,
			peak: 2,
			peakAt: at(10, 10),
			chat: 1,
			deaths: 1,
			restarts: 1,
			playtimeMs: 2 * 60 * 60_000,
			avgSessionMs: 60 * 60_000,
		})
		expect(s.players.map((p) => p.name).sort()).toEqual(['Alex', 'Steve'])
		expect(s.players.find((p) => p.name === 'Steve')).toMatchObject({ deaths: 1, sessions: 1 })
		expect(s.sessions).toHaveLength(2)
		// The heatmap holds player-minutes; two one-hour sessions -> 120.
		expect(s.heatmap.flat().reduce((a, b) => a + b, 0)).toBeCloseTo(120)
	})

	it('ends open sessions at a stop or restart', () => {
		const s = summarize(
			[
				{ t: at(9), kind: 'join', player: 'Steve' },
				{ t: at(9, 30), kind: 'stop' },
				{ t: at(12), kind: 'join', player: 'Steve' },
				{ t: at(12, 45), kind: 'start', seconds: 3 },
			],
			from,
			to,
			false,
			1,
		)
		expect(s.sessions.map((x) => x.end - x.start).sort()).toEqual([30 * 60_000, 45 * 60_000])
	})

	it('keeps a session running while the server is up', () => {
		const s = summarize([{ t: at(10), kind: 'join', player: 'Steve' }], from, to, true, 1)
		expect(s.sessions[0]).toMatchObject({ ongoing: true, end: to })
		expect(s.players[0].online).toBe(true)
	})

	it('only counts activity inside the chosen range, clipping sessions', () => {
		const s = summarize(
			[
				{ t: from - 60 * 60_000, kind: 'join', player: 'Steve' },
				{ t: from - 30 * 60_000, kind: 'chat', player: 'Steve' },
				{ t: from + 30 * 60_000, kind: 'leave', player: 'Steve' },
			],
			from,
			to,
			false,
			1,
		)
		expect(s.totals.joins).toBe(0)
		expect(s.totals.chat).toBe(0)
		expect(s.totals.playtimeMs).toBe(30 * 60_000)
	})

	it('has one entry per day, even for days without activity', () => {
		const weekFrom = at(0, 0, 0, new Date(2026, 10, 14))
		const s = summarize([], weekFrom, to, false, 0)
		expect(s.days.map((d) => d.day)).toEqual([
			'2026-11-14',
			'2026-11-15',
			'2026-11-16',
			'2026-11-17',
			'2026-11-18',
			'2026-11-19',
			'2026-11-20',
		])
		expect(s.totals.unique).toBe(0)
	})
})
