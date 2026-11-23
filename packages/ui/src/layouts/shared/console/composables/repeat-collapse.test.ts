import { describe, expect, it } from 'vitest'

import type { LogLine } from '../types'
import { createRepeatCollapser, isContinuationLine, normalizeLogText } from './repeat-collapse'

function level(text: string): LogLine['level'] {
	if (text.includes('/ERROR')) return 'error'
	if (text.includes('/WARN')) return 'warn'
	if (text.includes('/INFO')) return 'info'
	return null
}

/** Feeds lines through a collapser and returns what the console would show. */
function run(lines: string[]): string[] {
	const out: LogLine[] = []
	const collapser = createRepeatCollapser(
		(line) => out.push(line),
		(previous, next) => {
			out[out.lastIndexOf(previous)] = next
		},
	)
	let parent: LogLine['level'] = null
	for (const text of lines) {
		let l = level(text)
		if (/^\[\d{2}:/.test(text)) parent = l
		else l ??= parent
		collapser.feed({ text, level: l })
	}
	collapser.finishPending()
	return out.map((l) => l.text)
}

const replayError = (i: number) => [
	`[17:51:${String(i % 60).padStart(2, '0')}] [Client thread/ERROR]: Writing packet:`,
	`java.util.concurrent.RejectedExecutionException: Task FutureTask@${(0x6352cd9 + i * 977).toString(16)} rejected`,
	'\tat java.base/java.util.concurrent.ThreadPoolExecutor.reject(Unknown Source)',
	`\tat com.replaymod.recording.handler.RecordingEventHandler.onPlayerTick(RecordingEventHandler.java:${[252, 258, 262][i % 3]})`,
]

describe('continuation lines', () => {
	it('treats stack frames as part of the entry above', () => {
		expect(isContinuationLine('\tat foo.Bar.baz(Bar.java:1)')).toBe(true)
		expect(isContinuationLine('java.lang.RuntimeException: boom')).toBe(true)
		expect(isContinuationLine('[10:00:00] [main/INFO]: Hello')).toBe(false)
	})

	it('spots stack frames that a launcher timestamped line by line', () => {
		expect(isContinuationLine('[17:50:36] [Client thread/INFO]: [STDERR]:      at xaero.Foo.bar(Foo.java:59)')).toBe(true)
		expect(isContinuationLine('[17:50:36] [Client thread/INFO]: [STDERR]: Caused by: java.io.IOException')).toBe(true)
		expect(isContinuationLine('[17:50:36] [Client thread/INFO]: [STDERR]: java.lang.RuntimeException: x')).toBe(false)
	})
})

describe('normalizing log text', () => {
	it('ignores timestamps and object ids', () => {
		expect(normalizeLogText('[10:00:00] Task@6352cd9 x 0x7ff1', false)).toBe(
			normalizeLogText('[11:22:33] Task@1a2b3c4d x 0x0001', false),
		)
	})

	it('ignores numbers only for warnings and errors', () => {
		expect(normalizeLogText('Running 2221ms behind', true)).toBe(normalizeLogText('Running 54ms behind', true))
		expect(normalizeLogText('Spawn area: 28%', false)).not.toBe(normalizeLogText('Spawn area: 62%', false))
	})
})

describe('collapsing repeated entries', () => {
	it('folds 500 copies of an error into one entry and a counter', () => {
		const lines = Array.from({ length: 500 }, (_, i) => replayError(i)).flat()
		const shown = run(lines)
		expect(shown).toHaveLength(5)
		expect(shown.slice(0, 4)).toEqual(replayError(0))
		expect(shown[4]).toBe('  ↳ Repeated 499 more times (last at 17:51:19)')
	})

	it('says "time" for a single repeat', () => {
		const shown = run([...replayError(0), ...replayError(1)])
		expect(shown.at(-1)).toBe('  ↳ Repeated 1 more time (last at 17:51:01)')
	})

	it('collapses warnings that only differ in numbers', () => {
		const shown = run(
			Array.from(
				{ length: 30 },
				(_, i) =>
					`[18:58:${String(i).padStart(2, '0')}] [Chunk Batcher ${(i % 13) + 1}/WARN]: Needed to grow BufferBuilder buffer: Old size 524288 bytes, new size 2621440 bytes.`,
			),
		)
		expect(shown).toHaveLength(2)
		expect(shown[1]).toContain('Repeated 29 more times')
	})

	it('keeps different errors apart, even with the same first line', () => {
		const shown = run([
			'[18:57:36] [Client thread/INFO]: [STDERR]: java.lang.RuntimeException: Online mod data expired!',
			'[18:57:36] [Client thread/INFO]: [STDERR]:      at xaero.patreon.Patreon7.checkPatreon(Patreon7.java:75)',
			'[18:57:37] [Client thread/INFO]: [STDERR]: java.lang.RuntimeException: Online mod data expired!',
			'[18:57:37] [Client thread/INFO]: [STDERR]:      at xaero.common.misc.Internet.checkModVersion(Internet.java:53)',
		])
		expect(shown).toHaveLength(4)
		expect(shown.join('\n')).not.toContain('Repeated')
	})

	it('does not merge info lines whose numbers differ', () => {
		const shown = run([
			'[18:57:52] [Server thread/INFO]: Preparing spawn area: 28%',
			'[18:57:53] [Server thread/INFO]: Preparing spawn area: 62%',
		])
		expect(shown).toHaveLength(2)
	})

	it('starts a new group after something else was logged in between', () => {
		const shown = run([
			...replayError(0),
			...replayError(1),
			'[17:52:00] [Client thread/INFO]: Minimap session finalized.',
			...replayError(2),
		])
		expect(shown).toEqual([
			...replayError(0),
			'  ↳ Repeated 1 more time (last at 17:51:01)',
			'[17:52:00] [Client thread/INFO]: Minimap session finalized.',
			...replayError(2),
		])
	})

	it('shows a would-be repeat in full when its stack turns out different', () => {
		const other = [
			'[17:51:05] [Client thread/ERROR]: Writing packet:',
			'java.lang.IllegalStateException: closed',
		]
		const shown = run([...replayError(0), ...other])
		expect(shown).toEqual([...replayError(0), ...other])
	})

	it('can be reset', () => {
		const out: LogLine[] = []
		const c = createRepeatCollapser(
			(l) => out.push(l),
			() => {},
		)
		c.feed({ text: '[10:00:00] [main/ERROR]: x', level: 'error' })
		c.reset()
		c.feed({ text: '[10:00:01] [main/ERROR]: x', level: 'error' })
		c.finishPending()
		expect(out.map((l) => l.text)).toEqual(['[10:00:00] [main/ERROR]: x', '[10:00:01] [main/ERROR]: x'])
	})
})
