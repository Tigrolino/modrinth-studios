import type { LogLine } from '../types'

/**
 * Collapses log entries that repeat back to back (the same error and stack
 * trace every tick, for example) into the first copy plus one
 * "↳ Repeated N more times" line, which is updated in place as more copies
 * arrive.
 *
 * An entry is a timestamped line plus the lines that continue it (stack
 * frames, "Caused by", wrapped messages). Two entries count as the same when
 * they only differ in timestamps and object ids (`Foo@1a2b3c`); for warnings
 * and errors numbers are ignored too, so a trace that alternates between
 * line numbers still collapses.
 */

const ENTRY_START_RE = /^\[\d{2}:\d{2}:\d{2}\]/
const TIMESTAMP_RE = /\[\d{2}:\d{2}:\d{2}\]\s?/g
const PREFIX_RE = /^\[\d{2}:\d{2}:\d{2}\]\s*\[[^\]]*\]:?\s?/
const STD_STREAM_RE = /^\[(?:STDERR|STDOUT)\]:\s?/
const CONTINUATION_RE = /^(?:\s|at |Caused by:|Suppressed:|\.\.\. \d+ more)/

export function isContinuationLine(text: string): boolean {
	if (!ENTRY_START_RE.test(text)) return true
	// Some launchers (Lunar, Forge's stdout redirect) timestamp every line of
	// a stack trace, so look at the message itself too.
	const message = text.replace(PREFIX_RE, '').replace(STD_STREAM_RE, '')
	return CONTINUATION_RE.test(message)
}

export function normalizeLogText(text: string, loose: boolean): string {
	let out = text
		.replace(TIMESTAMP_RE, '')
		.replace(/@[0-9a-f]{4,}\b/gi, '@#')
		.replace(/0x[0-9a-f]+/gi, '0x#')
	if (loose) out = out.replace(/\d+/g, '#')
	return out.trimEnd()
}

function timeOf(text: string): string | null {
	return text.match(/^\[(\d{2}:\d{2}:\d{2})\]/)?.[1] ?? null
}

function repeatText(count: number, time: string | null): string {
	const times = count === 1 ? 'time' : 'times'
	return `  ↳ Repeated ${count.toLocaleString()} more ${times}${time ? ` (last at ${time})` : ''}`
}

export interface RepeatCollapser {
	/** Feeds one line. `emit` appends a line; `replace` swaps the shown counter line for an updated one. */
	feed(line: LogLine): void
	/** True while a possible repeat is held back waiting for the rest of its lines. */
	hasPending(): boolean
	/** Decides on the held-back entry (call when no more lines arrived for a while). */
	finishPending(): void
	reset(): void
}

export function createRepeatCollapser(
	emit: (line: LogLine) => void,
	replace: (previous: LogLine, next: LogLine) => void,
): RepeatCollapser {
	// The last entry that was shown, normalized, and whether it's a warn/error.
	let shown: string[] = []
	let loose = false
	let counter: LogLine | null = null
	// An entry whose first line matches `shown`, held until it's complete.
	let pending: LogLine[] = []

	const startEntry = (line: LogLine) => {
		loose = line.level === 'error' || line.level === 'warn'
		shown = [normalizeLogText(line.text, loose)]
		counter = null
		emit(line)
	}

	const finishPending = () => {
		if (pending.length === 0) return
		const lines = pending
		pending = []
		const same =
			lines.length === shown.length &&
			lines.every((l, i) => normalizeLogText(l.text, loose) === shown[i])
		if (!same) {
			startEntry(lines[0])
			for (let i = 1; i < lines.length; i++) {
				shown.push(normalizeLogText(lines[i].text, loose))
				emit(lines[i])
			}
			return
		}
		const count = (counter?.repeat ?? 0) + 1
		const next: LogLine = {
			text: repeatText(count, timeOf(lines[0].text)),
			level: lines[0].level,
			repeat: count,
		}
		if (counter) replace(counter, next)
		else emit(next)
		counter = next
	}

	const feed = (line: LogLine) => {
		const continuation = (shown.length > 0 || pending.length > 0) && isContinuationLine(line.text)
		if (continuation) {
			if (pending.length > 0) {
				pending.push(line)
			} else if (counter) {
				// Lines continuing an entry after its counter: show them as they are.
				counter = null
				shown = [normalizeLogText(line.text, loose)]
				emit(line)
			} else {
				shown.push(normalizeLogText(line.text, loose))
				emit(line)
			}
			return
		}
		finishPending()
		if (shown.length > 0 && normalizeLogText(line.text, loose) === shown[0]) {
			pending = [line]
		} else {
			startEntry(line)
		}
	}

	return {
		feed,
		hasPending: () => pending.length > 0,
		finishPending,
		reset: () => {
			shown = []
			loose = false
			counter = null
			pending = []
		},
	}
}
