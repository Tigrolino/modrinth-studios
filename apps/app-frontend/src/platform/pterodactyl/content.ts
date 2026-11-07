/**
 * Modrinth Studios addition: Archon content (mods/plugins/datapacks) on a
 * Pterodactyl server.
 *
 * Pterodactyl knows nothing about Modrinth, so Studio keeps a small manifest
 * on the server itself (`/.modrinth-studio/content.json`) recording which
 * Modrinth project/version each file it installed came from. Files that were
 * put there some other way still show up, just without Modrinth metadata.
 */
import type { Archon } from '@modrinth/api-client'

import {
	deleteFiles,
	listDirSafe,
	moveFile,
	normalizePath,
	type PteroFileAttributes,
	pullRemoteFile,
	readText,
	writeText,
} from './files'
import { pteroBytes } from './bridge'
import type { PteroServerRef } from './ids'
import { contentDirectory, modrinthLoader, type PteroFlavor, type PteroServerInfo } from './server'
import { emitPteroSyncEvent, getPteroInstallProgress } from './socket'

const MANIFEST_PATH = '/.modrinth-studio/content.json'
const LABRINTH = 'https://api.modrinth.com/v2'
const DISABLED_SUFFIX = '.disabled'

export type ManifestEntry = {
	project_id: string
	version_id: string
	installed_at: string
	from_modpack?: boolean
}
export type ManifestModpack =
	| { platform: 'modrinth'; project_id: string; version_id: string; installed_at: string }
	| { platform: 'local_file'; filename: string; name: string; installed_at: string }
export type Manifest = {
	version: 1
	files: Record<string, ManifestEntry>
	modpack?: ManifestModpack | null
	/** Cache of sha1 hashes for files Studio didn't install itself (see identify). */
	hashes?: Record<string, { size: number; modified: string; sha1: string; checked: boolean }>
}

type LabrinthFile = {
	url: string
	filename: string
	primary: boolean
	size: number
}
type LabrinthDependency = {
	project_id: string | null
	version_id: string | null
	dependency_type: 'required' | 'optional' | 'incompatible' | 'embedded'
}
export type LabrinthVersion = {
	id: string
	project_id: string
	name: string
	version_number: string
	loaders: string[]
	game_versions: string[]
	files: LabrinthFile[]
	dependencies: LabrinthDependency[]
}
export type LabrinthProject = {
	id: string
	title: string
	icon_url: string | null
	team: string
	project_type: string
	client_side: string
	server_side: string
}

const manifestLocks = new Map<string, Promise<unknown>>()

function withManifestLock<T>(serverId: string, task: () => Promise<T>): Promise<T> {
	const previous = manifestLocks.get(serverId) ?? Promise.resolve()
	const next = previous.catch(() => undefined).then(task)
	manifestLocks.set(serverId, next)
	return next
}

export async function readManifest(ref: PteroServerRef): Promise<Manifest> {
	const text = await readText(ref, MANIFEST_PATH)
	if (!text) return { version: 1, files: {} }
	try {
		const parsed = JSON.parse(text) as Manifest
		return {
			version: 1,
			files: parsed.files ?? {},
			modpack: parsed.modpack ?? null,
			hashes: parsed.hashes ?? {},
		}
	} catch {
		return { version: 1, files: {} }
	}
}

export async function updateManifest(
	ref: PteroServerRef,
	mutate: (manifest: Manifest) => void,
): Promise<void> {
	await withManifestLock(ref.serverId, async () => {
		const manifest = await readManifest(ref)
		mutate(manifest)
		await writeText(ref, MANIFEST_PATH, JSON.stringify(manifest, null, 2))
	})
}

export async function labrinth<T>(path: string): Promise<T> {
	const response = await fetch(`${LABRINTH}${path}`)
	if (!response.ok) throw new Error(`Modrinth API error (HTTP ${response.status}) for ${path}`)
	return (await response.json()) as T
}

export function kindDirectory(flavor: PteroFlavor, kind: Archon.Content.v1.AddonKind): string | null {
	if (kind === 'datapack') return '/world/datapacks'
	if (kind === 'mod' || kind === 'plugin') return contentDirectory(flavor)
	return null
}

export function defaultKind(flavor: PteroFlavor): Archon.Content.v1.AddonKind {
	return contentDirectory(flavor) === '/mods'
		? 'mod'
		: contentDirectory(flavor) === '/plugins'
			? 'plugin'
			: 'datapack'
}

export function manifestKey(kind: Archon.Content.v1.AddonKind, filename: string) {
	return `${kind}:${stripDisabled(filename)}`
}

function stripDisabled(filename: string) {
	return filename.endsWith(DISABLED_SUFFIX) ? filename.slice(0, -DISABLED_SUFFIX.length) : filename
}

function isContentFile(flavor: PteroFlavor, kind: Archon.Content.v1.AddonKind, f: PteroFileAttributes) {
	const name = stripDisabled(f.name.toLowerCase())
	if (kind === 'datapack') return !f.is_file || name.endsWith('.zip')
	if (!f.is_file) return false
	// Pumpkin plugins are native libraries / wasm, not jars.
	if (flavor === 'pumpkin') return !name.startsWith('.')
	return name.endsWith('.jar')
}

/** Loaders to ask Modrinth for, in order of preference. */
export function compatibleLoaders(flavor: PteroFlavor, kind: Archon.Content.v1.AddonKind): string[] {
	if (kind === 'datapack') return ['datapack']
	switch (flavor) {
		case 'paper':
			return ['paper', 'spigot', 'bukkit']
		case 'purpur':
			return ['purpur', 'paper', 'spigot', 'bukkit']
		case 'spigot':
			return ['spigot', 'bukkit']
		case 'proxy':
			return ['velocity', 'bungeecord', 'waterfall']
		case 'quilt':
			return ['quilt', 'fabric']
		case 'fabric':
		case 'forge':
		case 'neoforge':
			return [flavor]
		default:
			return []
	}
}

export async function getAddons(
	info: PteroServerInfo,
	query: URLSearchParams,
): Promise<Archon.Content.v1.Addons> {
	const { ref, flavor } = info
	const base: Archon.Content.v1.Addons = {
		modloader: modrinthLoader(flavor),
		modloader_version: info.loaderVersion,
		game_version: info.mcVersion,
		update_channel: 'release',
		modpack: null,
		files_need_side_type_review: 0,
		addons: [],
	}
	const [manifestForPack, packInfo] = await readManifest(ref).then(
		async (m) => [m, await modpackFields(m)] as const,
	)
	base.modpack = packInfo
	const activeInstall = getPteroInstallProgress(ref.serverId).find(
		(item) => item.progress !== 100 && !item.error,
	)
	if (activeInstall) {
		base.installing = activeInstall.key.type === 'platform' ? 'loader' : 'modpack'
	}
	const failedInstall = getPteroInstallProgress(ref.serverId).find((item) => item.error)
	if (failedInstall?.error) base.error = { message: failedInstall.error }
	if (query.get('addons') === 'false') {
		return { ...base, addons: null }
	}
	void manifestForPack

	const kinds: Archon.Content.v1.AddonKind[] = [defaultKind(flavor)]
	if (!kinds.includes('datapack') && flavor !== 'pumpkin' && flavor !== 'proxy')
		kinds.push('datapack')

	const [manifest, ...listings] = await Promise.all([
		readManifest(ref),
		...kinds.map((kind) => {
			const dir = kindDirectory(flavor, kind)
			return dir ? listDirSafe(ref, dir) : Promise.resolve([] as PteroFileAttributes[])
		}),
	])

	const found: { kind: Archon.Content.v1.AddonKind; file: PteroFileAttributes }[] = []
	kinds.forEach((kind, index) => {
		for (const file of listings[index]) {
			if (isContentFile(flavor, kind, file)) found.push({ kind, file })
		}
	})

	const projectIds = new Set<string>()
	const versionIds = new Set<string>()
	for (const { kind, file } of found) {
		const entry = manifest.files[manifestKey(kind, file.name)]
		if (entry) {
			projectIds.add(entry.project_id)
			versionIds.add(entry.version_id)
		}
	}

	const [projects, versions, teams] = await fetchMetadata([...projectIds], [...versionIds])
	const updates =
		query.get('updates') === 'true' ? await findUpdates(info, manifest, found) : new Map()

	const disabledFilter = query.get('disabled')
	const addons: Archon.Content.v1.Addon[] = found
		.map(({ kind, file }) => {
			const disabled = file.name.endsWith(DISABLED_SUFFIX)
			const filename = stripDisabled(file.name)
			const entry = manifest.files[manifestKey(kind, filename)]
			const project = entry ? projects.get(entry.project_id) : undefined
			const version = entry ? versions.get(entry.version_id) : undefined
			const owner = project ? teams.get(project.team) : undefined
			return {
				id: `${kind}:${filename}`,
				filename,
				filesize: file.size,
				btime: file.created_at,
				disabled,
				disabled_server: disabled,
				disabled_player: false,
				side_toggle_unlocked: false,
				manifest: null,
				kind,
				from_modpack: entry?.from_modpack ?? false,
				status: 'installed' as const,
				pack_client_retained: false,
				pack_client_depends: false,
				needs_side_type_review: false,
				has_update: updates.get(manifestKey(kind, filename)) ?? null,
				name: project?.title ?? filename.replace(/\.(jar|zip)$/i, ''),
				project_id: entry?.project_id ?? null,
				version: version
					? { id: version.id, name: version.version_number ?? version.name }
					: entry
						? { id: entry.version_id, name: null }
						: null,
				owner: owner ?? null,
				icon_url: project?.icon_url ?? null,
			}
		})
		.filter((addon) =>
			disabledFilter === null ? true : String(addon.disabled) === disabledFilter,
		)
		.filter((addon) => {
			const fromModpack = query.get('from_modpack')
			return fromModpack === null ? true : String(addon.from_modpack) === fromModpack
		})
		.sort((a, b) => (a.name ?? a.filename).localeCompare(b.name ?? b.filename))

	void identifyUnknownFiles(info, found, manifest)
	return { ...base, addons }
}

async function modpackFields(manifest: Manifest): Promise<Archon.Content.v1.ModpackFields | null> {
	const pack = manifest.modpack
	if (!pack) return null
	if (pack.platform === 'local_file') {
		return {
			spec: { platform: 'local_file', filename: pack.filename, name: pack.name, description: null },
			has_update: null,
			title: pack.name,
			description: null,
			icon_url: null,
			owner: null,
			version_number: null,
			date_published: pack.installed_at,
			downloads: null,
			followers: null,
		}
	}
	const [project, version] = await Promise.all([
		labrinth<LabrinthProject & { description: string; downloads: number; followers: number }>(
			`/project/${pack.project_id}`,
		).catch(() => null),
		labrinth<LabrinthVersion & { date_published: string }>(`/version/${pack.version_id}`).catch(
			() => null,
		),
	])
	return {
		spec: { platform: 'modrinth', project_id: pack.project_id, version_id: pack.version_id },
		has_update: null,
		title: project?.title ?? null,
		description: project?.description ?? null,
		icon_url: project?.icon_url ?? null,
		owner: null,
		version_number: version?.version_number ?? null,
		date_published: version?.date_published ?? pack.installed_at,
		downloads: project?.downloads ?? null,
		followers: project?.followers ?? null,
	}
}

// --- identifying files Studio didn't install ---------------------------------

const identifying = new Set<string>()
const MAX_IDENTIFY_BYTES = 64 * 1024 * 1024

async function sha1Hex(buffer: ArrayBuffer): Promise<string> {
	const digest = await crypto.subtle.digest('SHA-1', buffer)
	return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

/**
 * Hashes jars that have no manifest entry and asks Modrinth what they are,
 * so mods/plugins added by hand (or before Studio) get names, icons and
 * update checks too. Runs in the background; results are cached in the
 * manifest so each file is only hashed once.
 */
async function identifyUnknownFiles(
	info: PteroServerInfo,
	found: { kind: Archon.Content.v1.AddonKind; file: PteroFileAttributes }[],
	manifest: Manifest,
) {
	if (identifying.has(info.ref.serverId) || info.flavor === 'pumpkin') return
	const cache = manifest.hashes ?? {}
	const pending = found.filter(({ kind, file }) => {
		if (!file.is_file || file.size > MAX_IDENTIFY_BYTES) return false
		const key = manifestKey(kind, file.name)
		if (manifest.files[key]) return false
		const cached = cache[key]
		return !(cached && cached.size === file.size && cached.modified === file.modified_at)
	})
	if (pending.length === 0) return
	identifying.add(info.ref.serverId)
	try {
		const hashed: { key: string; kind: Archon.Content.v1.AddonKind; file: PteroFileAttributes; sha1: string }[] =
			[]
		const queue = [...pending]
		const worker = async () => {
			while (queue.length) {
				const { kind, file } = queue.shift()!
				const dir = kindDirectory(info.flavor, kind)
				if (!dir) continue
				try {
					const bytes = await pteroBytes(
						info.ref.panelId,
						`/servers/${info.ref.identifier}/files/contents?file=${encodeURIComponent(`${dir}/${file.name}`)}`,
					)
					hashed.push({ key: manifestKey(kind, file.name), kind, file, sha1: await sha1Hex(bytes) })
				} catch {
					// skip unreadable files
				}
			}
		}
		await Promise.all(Array.from({ length: 3 }, worker))
		if (hashed.length === 0) return

		const response = await fetch(`${LABRINTH}/version_files`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ hashes: hashed.map((h) => h.sha1), algorithm: 'sha1' }),
		})
		const matches = response.ok
			? ((await response.json()) as Record<string, { id: string; project_id: string }>)
			: {}

		let identified = 0
		await updateManifest(info.ref, (m) => {
			m.hashes ??= {}
			for (const h of hashed) {
				m.hashes[h.key] = {
					size: h.file.size,
					modified: h.file.modified_at,
					sha1: h.sha1,
					checked: true,
				}
				const match = matches[h.sha1]
				if (match && !m.files[h.key]) {
					m.files[h.key] = {
						project_id: match.project_id,
						version_id: match.id,
						installed_at: h.file.created_at,
					}
					identified++
				}
			}
		})
		if (identified > 0) emitPteroSyncEvent(info.ref.serverId, { type: 'protocol.reset' })
	} catch (error) {
		console.warn('[pterodactyl] Content identification failed', error)
	} finally {
		identifying.delete(info.ref.serverId)
	}
}

async function fetchMetadata(
	projectIds: string[],
	versionIds: string[],
): Promise<
	[
		Map<string, LabrinthProject>,
		Map<string, LabrinthVersion>,
		Map<string, Archon.Content.v1.ContentOwner>,
	]
> {
	const projects = new Map<string, LabrinthProject>()
	const versions = new Map<string, LabrinthVersion>()
	const teams = new Map<string, Archon.Content.v1.ContentOwner>()
	try {
		const [projectList, versionList] = await Promise.all([
			projectIds.length
				? labrinth<LabrinthProject[]>(`/projects?ids=${encodeURIComponent(JSON.stringify(projectIds))}`)
				: Promise.resolve([] as LabrinthProject[]),
			versionIds.length
				? labrinth<LabrinthVersion[]>(`/versions?ids=${encodeURIComponent(JSON.stringify(versionIds))}`)
				: Promise.resolve([] as LabrinthVersion[]),
		])
		for (const p of projectList) projects.set(p.id, p)
		for (const v of versionList) versions.set(v.id, v)
		const teamIds = [...new Set(projectList.map((p) => p.team))]
		if (teamIds.length) {
			const teamMembers = await labrinth<
				{ team_id: string; role: string; user: { id: string; username: string; avatar_url: string | null } }[][]
			>(`/teams?ids=${encodeURIComponent(JSON.stringify(teamIds))}`).catch(() => [])
			for (const members of teamMembers) {
				const owner = members.find((m) => m.role === 'Owner') ?? members[0]
				if (owner) {
					teams.set(owner.team_id, {
						id: owner.user.id,
						name: owner.user.username,
						type: 'user',
						icon_url: owner.user.avatar_url,
					})
				}
			}
		}
	} catch (error) {
		console.warn('[pterodactyl] Could not load Modrinth metadata for content', error)
	}
	return [projects, versions, teams]
}

export async function latestVersion(
	projectId: string,
	loaders: string[],
	gameVersion: string | null,
): Promise<LabrinthVersion | null> {
	const params = new URLSearchParams()
	if (loaders.length) params.set('loaders', JSON.stringify(loaders))
	if (gameVersion) params.set('game_versions', JSON.stringify([gameVersion]))
	let list = await labrinth<LabrinthVersion[]>(`/project/${projectId}/version?${params}`)
	if (list.length === 0 && gameVersion) {
		params.delete('game_versions')
		list = await labrinth<LabrinthVersion[]>(`/project/${projectId}/version?${params}`)
	}
	return list[0] ?? null
}

async function findUpdates(
	info: PteroServerInfo,
	manifest: Manifest,
	found: { kind: Archon.Content.v1.AddonKind; file: PteroFileAttributes }[],
): Promise<Map<string, string>> {
	const updates = new Map<string, string>()
	const checks = found
		.map(({ kind, file }) => ({ key: manifestKey(kind, file.name), kind }))
		.filter(({ key }) => manifest.files[key])
	const queue = [...checks]
	const worker = async () => {
		while (queue.length) {
			const { key, kind } = queue.shift()!
			const entry = manifest.files[key]
			try {
				const latest = await latestVersion(
					entry.project_id,
					compatibleLoaders(info.flavor, kind),
					info.mcVersion,
				)
				if (latest && latest.id !== entry.version_id) updates.set(key, latest.id)
			} catch {
				// ignore single failures
			}
		}
	}
	await Promise.all(Array.from({ length: 4 }, worker))
	return updates
}

async function installVersion(
	info: PteroServerInfo,
	version: LabrinthVersion,
	kind: Archon.Content.v1.AddonKind,
	installed: Set<string>,
	depth: number,
): Promise<void> {
	const dir = kindDirectory(info.flavor, kind)
	if (!dir) throw new Error(`This server type can't hold ${kind}s`)
	const file = version.files.find((f) => f.primary) ?? version.files[0]
	if (!file) throw new Error(`${version.name} has no downloadable file`)

	await pullRemoteFile(info.ref, file.url, dir, file.filename)
	await updateManifest(info.ref, (manifest) => {
		manifest.files[manifestKey(kind, file.filename)] = {
			project_id: version.project_id,
			version_id: version.id,
			installed_at: new Date().toISOString(),
		}
	})
	installed.add(version.project_id)

	if (depth >= 3) return
	for (const dependency of version.dependencies) {
		if (dependency.dependency_type !== 'required') continue
		if (dependency.project_id && installed.has(dependency.project_id)) continue
		try {
			const depVersion = dependency.version_id
				? await labrinth<LabrinthVersion>(`/version/${dependency.version_id}`)
				: dependency.project_id
					? await latestVersion(
							dependency.project_id,
							compatibleLoaders(info.flavor, kind),
							info.mcVersion,
						)
					: null
			if (depVersion && !installed.has(depVersion.project_id)) {
				await installVersion(info, depVersion, kind, installed, depth + 1)
			}
		} catch (error) {
			console.warn('[pterodactyl] Failed to install dependency', dependency, error)
		}
	}
}

async function installedProjects(info: PteroServerInfo): Promise<Set<string>> {
	const manifest = await readManifest(info.ref)
	return new Set(Object.values(manifest.files).map((e) => e.project_id))
}

export async function addAddons(
	info: PteroServerInfo,
	requests: Archon.Content.v1.AddAddonRequest[],
): Promise<void> {
	if (info.flavor === 'pumpkin') {
		throw new Error(
			'Pumpkin plugins are not on Modrinth. Upload them from the Files tab or the content upload button instead.',
		)
	}
	const installed = await installedProjects(info)
	for (const request of requests) {
		let kind = request.kind ?? defaultKind(info.flavor)
		let version = request.version_id
			? await labrinth<LabrinthVersion>(`/version/${request.version_id}`)
			: await latestVersion(
					request.project_id,
					compatibleLoaders(info.flavor, kind),
					info.mcVersion,
				)
		if (!version && !request.kind) {
			kind = 'datapack'
			version = await latestVersion(request.project_id, ['datapack'], info.mcVersion)
		}
		if (!version) throw new Error(`No compatible version found for ${request.project_id}`)
		if (version.loaders.includes('datapack') && version.loaders.length === 1) kind = 'datapack'
		await installVersion(info, version, kind, installed, 0)
	}
}

function filePathFor(info: PteroServerInfo, kind: Archon.Content.v1.AddonKind, filename: string) {
	const dir = kindDirectory(info.flavor, kind)
	if (!dir) throw new Error(`This server type can't hold ${kind}s`)
	return normalizePath(`${dir}/${stripDisabled(filename)}`)
}

export async function deleteAddons(
	info: PteroServerInfo,
	items: Archon.Content.v1.RemoveAddonRequest[],
): Promise<void> {
	const paths = items.flatMap((item) => {
		const path = filePathFor(info, item.kind, item.filename)
		return [path, `${path}${DISABLED_SUFFIX}`]
	})
	// Delete existing files only; Wings errors on missing ones.
	const byDir = new Map<string, Set<string>>()
	for (const item of items) {
		const dir = kindDirectory(info.flavor, item.kind)!
		if (!byDir.has(dir)) {
			byDir.set(dir, new Set((await listDirSafe(info.ref, dir)).map((f) => f.name)))
		}
	}
	const existing = paths.filter((path) => {
		const dir = path.slice(0, path.lastIndexOf('/')) || '/'
		return byDir.get(dir)?.has(path.slice(path.lastIndexOf('/') + 1))
	})
	if (existing.length) await deleteFiles(info.ref, existing)
	await updateManifest(info.ref, (manifest) => {
		for (const item of items) delete manifest.files[manifestKey(item.kind, item.filename)]
	})
}

export async function setAddonEnabled(
	info: PteroServerInfo,
	items: { kind: Archon.Content.v1.AddonKind; filename: string; enabled: boolean }[],
): Promise<void> {
	for (const item of items) {
		const path = filePathFor(info, item.kind, item.filename)
		const dir = path.slice(0, path.lastIndexOf('/')) || '/'
		const names = new Set((await listDirSafe(info.ref, dir)).map((f) => f.name))
		const base = path.slice(path.lastIndexOf('/') + 1)
		if (item.enabled && names.has(`${base}${DISABLED_SUFFIX}`)) {
			await moveFile(info.ref, `${path}${DISABLED_SUFFIX}`, path)
		} else if (!item.enabled && names.has(base)) {
			await moveFile(info.ref, path, `${path}${DISABLED_SUFFIX}`)
		}
	}
}

export async function updateAddons(
	info: PteroServerInfo,
	items: Archon.Content.v1.UpdateAddonRequest[],
	kindHint?: Archon.Content.v1.AddonKind,
): Promise<void> {
	const manifest = await readManifest(info.ref)
	const installed = new Set(Object.values(manifest.files).map((e) => e.project_id))
	for (const item of items) {
		const kinds: Archon.Content.v1.AddonKind[] = kindHint
			? [kindHint]
			: [defaultKind(info.flavor), 'datapack']
		const kind = kinds.find((k) => manifest.files[manifestKey(k, item.filename)])
		if (!kind) throw new Error(`${item.filename} wasn't installed from Modrinth, so it can't be updated`)
		const entry = manifest.files[manifestKey(kind, item.filename)]
		const version = item.version_id
			? await labrinth<LabrinthVersion>(`/version/${item.version_id}`)
			: await latestVersion(entry.project_id, compatibleLoaders(info.flavor, kind), info.mcVersion)
		if (!version || version.id === entry.version_id) continue
		const wasDisabled = (await listDirSafe(info.ref, kindDirectory(info.flavor, kind)!)).some(
			(f) => f.name === `${item.filename}${DISABLED_SUFFIX}`,
		)
		await deleteAddons(info, [{ kind, filename: item.filename }])
		installed.delete(entry.project_id)
		await installVersion(info, version, kind, installed, 3)
		if (wasDisabled) {
			const file = version.files.find((f) => f.primary) ?? version.files[0]
			await setAddonEnabled(info, [{ kind, filename: file.filename, enabled: false }])
		}
	}
}
