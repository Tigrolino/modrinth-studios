import { describe, expect, it } from 'vitest'

import { formatStorageSize } from '@/composables/use-studio-instance-storage'
import { noteCounts, parseNotes } from '@/pages/instance/client/horizon-notes'
import type { ClientInstall, ClientKind } from '@/platform/app-lib/studio-clients'

import { clientLogo, clientMeta, CLIENTS } from './catalog'
import { wantedLoader } from './loader-sync'

const KINDS: ClientKind[] = ['lunar', 'labymod', 'norisk', 'horizon']

function install(client: ClientKind, version_id: string): ClientInstall {
	return {
		client,
		version_id,
		version_name: version_id,
		variant_name: null,
		game_version: '1.8.9',
		prepared: true,
		checked_at: 0,
	} as ClientInstall
}

describe('client catalog', () => {
	it('describes every supported client exactly once', () => {
		expect(CLIENTS.map((c) => c.kind).sort()).toEqual([...KINDS].sort())
		for (const meta of CLIENTS) {
			expect(meta.name, meta.kind).toBeTruthy()
			expect(meta.tagline, meta.kind).toBeTruthy()
			expect(meta.color, meta.kind).toMatch(/^#|^rgb|^var\(/)
			for (const link of meta.links) expect(link.url, meta.kind).toMatch(/^https:\/\//)
		}
	})

	it('looks clients up by kind', () => {
		for (const kind of KINDS) expect(clientMeta(kind)?.kind).toBe(kind)
		expect(clientMeta(null)).toBeNull()
		expect(clientMeta(undefined)).toBeNull()
	})

	it('has an https logo for every client', () => {
		for (const kind of KINDS) {
			expect(clientLogo(kind)).toMatch(/^https:\/\//)
			expect(clientLogo(clientMeta(kind)!)).toBe(clientLogo(kind))
		}
	})
})

describe('client loader sync', () => {
	it('makes Lunar instances match the Lunar module', () => {
		expect(wantedLoader(install('lunar', '1.21.11|fabric'))).toBe('fabric')
		expect(wantedLoader(install('lunar', '1.8.9|forge'))).toBe('forge')
		expect(wantedLoader(install('lunar', '1.8.9|lunar'))).toBe('vanilla')
	})

	it('leaves other clients and plain instances alone', () => {
		expect(wantedLoader(null)).toBeNull()
		expect(wantedLoader(undefined)).toBeNull()
		expect(wantedLoader(install('norisk', 'pack|1.21.10'))).toBeNull()
		expect(wantedLoader(install('labymod', 'production|1.21.11'))).toBeNull()
	})
})

describe('Horizon release notes', () => {
	const body = [
		'## New',
		'- Added **shaders** support (#120)',
		'- `F3` debug screen',
		'',
		'### Fixed',
		'* Crash when joining servers (#118, fixes #99)',
		'',
		'## Contributors',
		'Thanks to alice, bob.',
		'',
		'---',
		'Download the launcher at horizon.example',
		'- not a change',
	].join('\n')

	it('splits sections, strips markdown and pulls out issue numbers', () => {
		const notes = parseNotes(body)
		expect(notes.sections.map((s) => [s.title, s.tone])).toEqual([
			['New', 'new'],
			['Fixed', 'fixed'],
		])
		expect(notes.sections[0].items).toEqual([
			{ text: 'Added shaders support', refs: ['#120'] },
			{ text: 'F3 debug screen', refs: [] },
		])
		expect(notes.sections[1].items[0]).toEqual({
			text: 'Crash when joining servers',
			refs: ['#118', '#99'],
		})
	})

	it('keeps contributors and drops the footer', () => {
		const notes = parseNotes(body)
		expect(notes.contributors).toBe('alice, bob')
		expect(JSON.stringify(notes)).not.toContain('not a change')
	})

	it('puts loose bullets under "Changes" and skips empty sections', () => {
		const notes = parseNotes('## Empty\n\n- one\n- two')
		expect(notes.sections).toHaveLength(1)
		expect(notes.sections[0].items.map((i) => i.text)).toEqual(['one', 'two'])
		expect(parseNotes('- loose').sections[0].title).toBe('Changes')
		expect(parseNotes('').sections).toEqual([])
	})

	it('summarises the counts per section', () => {
		expect(noteCounts(parseNotes(body))).toEqual([
			{ tone: 'new', label: '2 new' },
			{ tone: 'fixed', label: '1 fixed' },
		])
	})
})

describe('storage sizes', () => {
	it.each([
		[0, '0 B'],
		[-5, '0 B'],
		[Number.NaN, '0 B'],
		[512, '512 B'],
		[1536, '1.5 KB'],
		[10 * 1024 * 1024, '10 MB'],
		[1.4 * 1024 ** 3, '1.4 GB'],
		[3 * 1024 ** 5, '3072 TB'],
	])('%s bytes is %s', (bytes, text) => {
		expect(formatStorageSize(bytes)).toBe(text)
	})
})
