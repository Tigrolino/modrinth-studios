/**
 * Modrinth Studios: Horizon's GitHub release notes, split into their sections
 * ("### New", "### Fixed", ...) so they can be shown as tidy lists instead of
 * a raw markdown dump.
 */
export type NoteSection = {
	title: string
	tone: 'new' | 'fixed' | 'changed' | 'other'
	items: { text: string; refs: string[] }[]
}

export type ParsedNotes = {
	sections: NoteSection[]
	contributors: string | null
}

function toneOf(title: string): NoteSection['tone'] {
	const t = title.toLowerCase()
	if (/new|add|feature/.test(t)) return 'new'
	if (/fix|bug/.test(t)) return 'fixed'
	if (/change|improve|perf|update/.test(t)) return 'changed'
	return 'other'
}

export function parseNotes(body: string): ParsedNotes {
	// Everything after the "---" footer, and the "Your launcher updates itself"
	// install blurb, is the same on every release.
	const cut = body.split(/\n-{3,}\n/)[0].split(/\n\s*Your launcher updates itself/)[0]
	const sections: NoteSection[] = []
	let contributors: string | null = null
	let current: NoteSection | null = null
	for (const raw of cut.split('\n')) {
		const line = raw.trim()
		const heading = line.match(/^#{2,4}\s+(.*)$/)
		if (heading) {
			const title = heading[1].trim()
			if (/contributor/i.test(title)) {
				current = null
				contributors = ''
				continue
			}
			current = { title, tone: toneOf(title), items: [] }
			sections.push(current)
			continue
		}
		if (contributors !== null && current === null) {
			if (line) contributors = `${contributors} ${line}`.trim()
			continue
		}
		const bullet = line.match(/^[-*]\s+(.*)$/)
		if (bullet) {
			if (!current) {
				current = { title: 'Changes', tone: 'other', items: [] }
				sections.push(current)
			}
			let text = bullet[1]
			const refs: string[] = []
			// "(#17975, fixes #17950)" -> pull the PR/issue numbers out of the text.
			text = text.replace(/\s*\(([^)]*#\d+[^)]*)\)\s*$/, (_, inner: string) => {
				refs.push(...(inner.match(/#\d+/g) ?? []))
				return ''
			})
			text = text.replace(/`([^`]+)`/g, '$1').replace(/\*\*([^*]+)\*\*/g, '$1')
			current.items.push({ text, refs })
		}
	}
	return {
		sections: sections.filter((s) => s.items.length),
		contributors: contributors?.replace(/^Thanks to\s*/i, '').replace(/\.$/, '') || null,
	}
}

export function noteCounts(notes: ParsedNotes) {
	return notes.sections.map((s) => ({
		tone: s.tone,
		label: `${s.items.length} ${s.title.toLowerCase()}`,
	}))
}

export const TONE_CLASSES: Record<NoteSection['tone'], string> = {
	new: 'bg-brand-highlight text-brand',
	fixed: 'bg-highlight-blue text-blue',
	changed: 'bg-highlight-orange text-orange',
	other: 'bg-surface-4 text-secondary',
}
