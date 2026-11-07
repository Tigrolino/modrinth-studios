/**
 * Modrinth Studios addition: modpack installs, loader switching and game
 * version changes for Pterodactyl servers.
 *
 * Loaders are installed in one of two ways:
 *  - the egg already is that loader → update its version variables and let
 *    Pterodactyl reinstall (the egg's own install script does the work);
 *  - otherwise, for loaders that ship a single runnable server jar (vanilla,
 *    Paper, Purpur, Fabric) Studio drops that jar in as SERVER_JARFILE and
 *    records an override so the UI knows what's really installed.
 * Forge / NeoForge / Quilt need their installer to run, so switching to them
 * needs an egg of that type (changeable by a panel admin).
 */
import type { Archon } from '@modrinth/api-client'
import { listen } from '@tauri-apps/api/event'
import { fetch as tauriFetch } from '@tauri-apps/plugin-http'

import { type MrpackSummary, pteroInstallMrpack, pteroRequest } from './bridge'
import { deleteFiles, listDir, pullRemoteFile, readText, writeText } from './files'
import { PTERO_WORLD_ID } from './ids'
import {
	compatibleLoaders,
	getAddons,
	labrinth,
	type LabrinthProject,
	type LabrinthVersion,
	setAddonEnabled,
	updateAddons,
	type ManifestEntry,
	type ManifestModpack,
	readManifest,
	updateManifest,
} from './content'
import {
	fetchServerInfo,
	invalidatePteroServerInfo,
	LOADER_OVERRIDE_PATH,
	type LoaderOverride,
	type PteroFlavor,
	type PteroServerInfo,
	type PteroVariable,
} from './server'
import { emitPteroSyncEvent, setPteroInstallProgress } from './socket'

type ProgressKey = Archon.Websocket.v0.InstallProgressKey

// --- progress ----------------------------------------------------------------

class InstallProgress {
	private readonly id = `ptero-install-${Date.now().toString(36)}`
	constructor(
		private readonly serverId: string,
		private readonly key: ProgressKey,
	) {}

	set(progress: number | null, error: string | null = null) {
		setPteroInstallProgress(this.serverId, [
			{ world_id: PTERO_WORLD_ID, key: this.key, id: this.id, progress, error },
		])
	}

	finish() {
		this.set(100)
		emitPteroSyncEvent(this.serverId, { type: 'protocol.reset' })
		setTimeout(() => setPteroInstallProgress(this.serverId, []), 4000)
	}

	fail(error: unknown) {
		const message =
			error instanceof Error ? error.message : (error as { message?: string })?.message ?? String(error)
		console.error('[pterodactyl] install failed', error)
		this.set(null, message)
		setTimeout(() => setPteroInstallProgress(this.serverId, []), 60_000)
	}
}

/** Runs an install in the background so the request returns immediately, like Archon. */
function runInBackground(progress: InstallProgress, task: () => Promise<void>) {
	progress.set(0)
	void task().then(
		() => progress.finish(),
		(error) => progress.fail(error),
	)
}

// --- loader metadata ---------------------------------------------------------

async function getJson<T>(url: string): Promise<T> {
	const response = await tauriFetch(url, { method: 'GET' })
	if (!response.ok) throw new Error(`Request to ${new URL(url).host} failed (HTTP ${response.status})`)
	return (await response.json()) as T
}

type JarSource = { url: string; loaderVersion: string | null }

async function vanillaJar(mc: string): Promise<JarSource> {
	const manifest = await getJson<{ versions: { id: string; url: string }[] }>(
		'https://piston-meta.mojang.com/mc/game/version_manifest_v2.json',
	)
	const entry = manifest.versions.find((v) => v.id === mc)
	if (!entry) throw new Error(`Minecraft ${mc} doesn't exist`)
	const version = await getJson<{ downloads?: { server?: { url: string } } }>(entry.url)
	const url = version.downloads?.server?.url
	if (!url) throw new Error(`Minecraft ${mc} has no server jar`)
	return { url, loaderVersion: null }
}

async function fabricJar(mc: string, loader: string | null): Promise<JarSource> {
	let loaderVersion = loader
	if (!loaderVersion) {
		const loaders = await getJson<{ loader: { version: string; stable: boolean } }[]>(
			`https://meta.fabricmc.net/v2/versions/loader/${encodeURIComponent(mc)}`,
		)
		if (!loaders.length) throw new Error(`Fabric doesn't support Minecraft ${mc}`)
		loaderVersion = (loaders.find((l) => l.loader.stable) ?? loaders[0]).loader.version
	}
	const installers = await getJson<{ version: string; stable: boolean }[]>(
		'https://meta.fabricmc.net/v2/versions/installer',
	)
	const installer = (installers.find((i) => i.stable) ?? installers[0])?.version
	if (!installer) throw new Error('Could not find a Fabric installer version')
	return {
		url: `https://meta.fabricmc.net/v2/versions/loader/${encodeURIComponent(mc)}/${encodeURIComponent(loaderVersion)}/${encodeURIComponent(installer)}/server/jar`,
		loaderVersion,
	}
}

async function paperJar(mc: string, build: string | null): Promise<JarSource> {
	type Build = { id: number; channel: string; downloads: Record<string, { url: string }> }
	const builds = await getJson<Build[]>(
		`https://fill.papermc.io/v3/projects/paper/versions/${encodeURIComponent(mc)}/builds`,
	).catch(() => [] as Build[])
	if (!builds.length) throw new Error(`Paper doesn't support Minecraft ${mc}`)
	builds.sort((a, b) => b.id - a.id)
	const chosen =
		(build && /^\d+$/.test(build) ? builds.find((b) => String(b.id) === build) : null) ??
		builds.find((b) => b.channel.toUpperCase() === 'STABLE') ??
		builds[0]
	const url = chosen.downloads['server:default']?.url ?? Object.values(chosen.downloads)[0]?.url
	if (!url) throw new Error(`Paper build ${chosen.id} has no download`)
	return { url, loaderVersion: String(chosen.id) }
}

async function purpurJar(mc: string, build: string | null): Promise<JarSource> {
	const info = await getJson<{ builds: { latest: string; all: string[] } }>(
		`https://api.purpurmc.org/v2/purpur/${encodeURIComponent(mc)}`,
	).catch(() => null)
	if (!info) throw new Error(`Purpur doesn't support Minecraft ${mc}`)
	const chosen = build && info.builds.all.includes(build) ? build : info.builds.latest
	return {
		url: `https://api.purpurmc.org/v2/purpur/${encodeURIComponent(mc)}/${chosen}/download`,
		loaderVersion: chosen,
	}
}

const JAR_LOADERS: Partial<Record<PteroFlavor, (mc: string, v: string | null) => Promise<JarSource>>> = {
	vanilla: (mc) => vanillaJar(mc),
	fabric: fabricJar,
	paper: paperJar,
	purpur: purpurJar,
}

// --- egg variables -------------------------------------------------------------

const MC_VARIABLES = ['MINECRAFT_VERSION', 'MC_VERSION', 'VANILLA_VERSION', 'VERSION']
const LOADER_VARIABLES: Partial<Record<PteroFlavor, string[]>> = {
	fabric: ['FABRIC_VERSION', 'LOADER_VERSION'],
	quilt: ['QUILT_VERSION', 'LOADER_VERSION'],
	forge: ['FORGE_VERSION'],
	neoforge: ['NEOFORGE_VERSION', 'NEO_VERSION'],
	paper: ['BUILD_NUMBER', 'BUILD'],
	purpur: ['BUILD_NUMBER', 'BUILD'],
}

async function startupVariables(info: PteroServerInfo) {
	const response = await pteroRequest<{
		data: { attributes: PteroVariable }[]
		meta: { raw_startup_command: string }
	}>(info.ref.panelId, 'GET', `/servers/${info.ref.identifier}/startup`)
	return {
		variables: response.data.map((d) => d.attributes),
		rawStartup: response.meta.raw_startup_command,
	}
}

async function setVariable(info: PteroServerInfo, key: string, value: string) {
	await pteroRequest(info.ref.panelId, 'PUT', `/servers/${info.ref.identifier}/startup/variable`, {
		body: { key, value },
	})
}

function flavorFromModloader(loader: string): PteroFlavor {
	switch (loader.toLowerCase().replace(/[-_ ]/g, '')) {
		case 'neoforge':
			return 'neoforge'
		case 'forge':
			return 'forge'
		case 'fabric':
		case 'fabricloader':
			return 'fabric'
		case 'quilt':
		case 'quiltloader':
			return 'quilt'
		case 'paper':
			return 'paper'
		case 'purpur':
			return 'purpur'
		default:
			return 'vanilla'
	}
}

function prettyFlavor(flavor: PteroFlavor) {
	return (
		{ neoforge: 'NeoForge', forge: 'Forge', quilt: 'Quilt', fabric: 'Fabric', paper: 'Paper', purpur: 'Purpur' } as Record<
			string,
			string
		>
	)[flavor] ?? 'Vanilla'
}

type LoaderPlan = () => Promise<void>

/**
 * Works out how to put `flavor` (mc `mc`, loader version `loader`) on the
 * server and returns the function that does it. Throws up front when it's
 * impossible, so modpack installs can fail before touching any files.
 */
async function planLoader(
	info: PteroServerInfo,
	flavor: PteroFlavor,
	mc: string,
	loader: string | null,
): Promise<LoaderPlan> {
	if (info.flavor === 'pumpkin' || info.eggFlavor === 'pumpkin') {
		throw new Error('Pumpkin servers run their own server software; loaders and modpacks don\'t apply.')
	}
	if (info.flavor === 'proxy') throw new Error("Proxy servers can't run a Minecraft loader.")

	const { variables, rawStartup } = await startupVariables(info)
	const editable = (names: string[]) =>
		names.map((n) => variables.find((v) => v.env_variable === n && v.is_editable)).find(Boolean)

	// 1. The egg already is this loader: change its variables and reinstall.
	const sameEgg = info.eggFlavor === flavor || (flavor === 'vanilla' && info.eggFlavor === 'unknown')
	const mcVar = editable(MC_VARIABLES)
	if (sameEgg && mcVar) {
		return async () => {
			await setVariable(info, mcVar.env_variable, mc)
			const loaderVar = editable(LOADER_VARIABLES[flavor] ?? [])
			if (loaderVar) {
				let value = loader ?? 'latest'
				if (flavor === 'forge' && loader && !loader.startsWith(`${mc}-`)) value = `${mc}-${loader}`
				if ((flavor === 'paper' || flavor === 'purpur') && !(loader && /^\d+$/.test(loader)))
					value = 'latest'
				await setVariable(info, loaderVar.env_variable, value)
			}
			await deleteFiles(info.ref, [LOADER_OVERRIDE_PATH]).catch(() => undefined)
			await pteroRequest(info.ref.panelId, 'POST', `/servers/${info.ref.identifier}/settings/reinstall`)
		}
	}

	// 2. Single-jar loaders: drop the jar in as SERVER_JARFILE.
	const resolveJar = JAR_LOADERS[flavor]
	if (resolveJar) {
		const jarVar = variables.find((v) => v.env_variable === 'SERVER_JARFILE')
		if (!jarVar && !/-jar\s/.test(rawStartup)) {
			throw new Error(
				`This egg doesn't start a server jar, so Studio can't put ${prettyFlavor(flavor)} on it. Ask a panel admin to switch the egg.`,
			)
		}
		const jarName = (jarVar?.server_value || jarVar?.default_value || 'server.jar').replace(/^\/+/, '')
		const source = await resolveJar(mc, loader)
		return async () => {
			await pteroRequest(info.ref.panelId, 'POST', `/servers/${info.ref.identifier}/power`, {
				body: { signal: 'kill' },
			}).catch(() => undefined)
			await deleteFiles(info.ref, [`/${jarName}`]).catch(() => undefined)
			const slash = jarName.lastIndexOf('/')
			await pullRemoteFile(
				info.ref,
				source.url,
				slash === -1 ? '/' : `/${jarName.slice(0, slash)}`,
				jarName.slice(slash + 1),
			)
			const override: LoaderOverride = {
				flavor,
				mc_version: mc,
				loader_version: source.loaderVersion,
			}
			await writeText(info.ref, LOADER_OVERRIDE_PATH, JSON.stringify(override, null, 2))
			// Keep the egg's own version variable in sync where we can, so a
			// later panel reinstall doesn't silently downgrade.
			if (mcVar && sameEgg) await setVariable(info, mcVar.env_variable, mc).catch(() => undefined)
		}
	}

	throw new Error(
		`Switching to ${prettyFlavor(flavor)} needs a ${prettyFlavor(flavor)} egg — its installer has to run on the server. Ask a panel admin to change the egg, then install again.`,
	)
}

async function afterLoaderChange(info: PteroServerInfo) {
	invalidatePteroServerInfo(info.ref.serverId)
	const fresh = await fetchServerInfo(info.ref)
	emitPteroSyncEvent(info.ref.serverId, { type: 'protocol.reset' })
	return fresh
}

async function resolveMc(info: PteroServerInfo, requested?: string | null): Promise<string> {
	if (requested) return requested
	if (info.mcVersion) return info.mcVersion
	const manifest = await getJson<{ latest: { release: string } }>(
		'https://piston-meta.mojang.com/mc/game/version_manifest_v2.json',
	)
	return manifest.latest.release
}

// --- modpacks ------------------------------------------------------------------

function loaderFromDependencies(deps: Record<string, string>) {
	const mc = deps.minecraft ?? null
	for (const [key, flavor] of [
		['neoforge', 'neoforge'],
		['forge', 'forge'],
		['quilt-loader', 'quilt'],
		['fabric-loader', 'fabric'],
	] as const) {
		if (deps[key]) return { flavor: flavor as PteroFlavor, mc, loader: deps[key] }
	}
	return { flavor: 'vanilla' as PteroFlavor, mc, loader: null }
}

async function mrpackDependencies(source: { url?: string; versionId?: string }) {
	// For Modrinth versions the loader is in the version metadata, so we can
	// check it before downloading anything.
	if (source.versionId) {
		const version = await labrinth<LabrinthVersion>(`/version/${source.versionId}`)
		const loader = version.loaders.find((l) => l !== 'minecraft') ?? 'vanilla'
		return { flavor: flavorFromModloader(loader), mc: version.game_versions[0] ?? null, loader: null }
	}
	return null
}

function kindFromPath(path: string): Archon.Content.v1.AddonKind | null {
	const normalized = path.replace(/\\/g, '/').replace(/^\/+/, '')
	if (normalized.startsWith('mods/')) return 'mod'
	if (normalized.startsWith('plugins/')) return 'plugin'
	if (/^[^/]+\/datapacks\//.test(normalized)) return 'datapack'
	return null
}

async function recordModpackFiles(
	info: PteroServerInfo,
	summary: MrpackSummary,
	modpack: ManifestModpack,
) {
	const hashes = summary.files.map((f) => f.sha1).filter((h): h is string => !!h)
	let byHash: Record<string, { id: string; project_id: string }> = {}
	if (hashes.length) {
		const response = await fetch('https://api.modrinth.com/v2/version_files', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ hashes, algorithm: 'sha1' }),
		}).catch(() => null)
		if (response?.ok) byHash = (await response.json()) as typeof byHash
	}
	const now = new Date().toISOString()
	await updateManifest(info.ref, (manifest) => {
		for (const [key, entry] of Object.entries(manifest.files)) {
			if (entry.from_modpack) delete manifest.files[key]
		}
		for (const file of summary.files) {
			const kind = kindFromPath(file.path)
			const match = file.sha1 ? byHash[file.sha1] : undefined
			if (!kind || !match) continue
			const filename = file.path.split('/').pop()!
			const entry: ManifestEntry = {
				project_id: match.project_id,
				version_id: match.id,
				installed_at: now,
				from_modpack: true,
			}
			manifest.files[`${kind}:${filename}`] = entry
		}
		manifest.modpack = modpack
	})
}

/** Removes files that came from the previously installed modpack. */
async function removePreviousModpack(info: PteroServerInfo) {
	const manifest = await readManifest(info.ref)
	const paths: string[] = []
	for (const [key, entry] of Object.entries(manifest.files)) {
		if (!entry.from_modpack) continue
		const [kind, ...rest] = key.split(':')
		const filename = rest.join(':')
		const dir = kind === 'plugin' ? '/plugins' : kind === 'datapack' ? '/world/datapacks' : '/mods'
		paths.push(`${dir}/${filename}`, `${dir}/${filename}.disabled`)
	}
	if (paths.length) await deleteFiles(info.ref, paths).catch(() => undefined)
}

/**
 * Archon's `soft_override: false` means a full reset: everything on the server
 * is erased (backups are kept by the panel). Studio's own settings and an
 * already-accepted EULA survive.
 */
export async function wipeServer(info: PteroServerInfo) {
	await pteroRequest(info.ref.panelId, 'POST', `/servers/${info.ref.identifier}/power`, {
		body: { signal: 'kill' },
	}).catch(() => undefined)
	const eula = await readText(info.ref, '/eula.txt').catch(() => null)
	const root = await listDir(info.ref, '/')
	const names = root.map((f) => f.name).filter((n) => n !== '.modrinth-studio')
	for (let i = 0; i < names.length; i += 50) {
		await deleteFiles(
			info.ref,
			names.slice(i, i + 50).map((n) => `/${n}`),
		)
	}
	await updateManifest(info.ref, (manifest) => {
		manifest.files = {}
		manifest.modpack = null
		manifest.hashes = {}
	})
	if (eula && /eula\s*=\s*true/i.test(eula)) await writeText(info.ref, '/eula.txt', 'eula=true\n')
}

async function installMrpack(
	info: PteroServerInfo,
	source: { url?: string; bytes?: ArrayBuffer; versionId?: string },
	modpack: (summary: MrpackSummary) => ManifestModpack,
	progress: InstallProgress,
	softOverride: boolean,
) {
	// Fail early if the loader can't go on this server.
	const early = await mrpackDependencies(source)
	if (early) await planLoader(info, early.flavor, await resolveMc(info, early.mc), null)

	if (softOverride) await removePreviousModpack(info)
	else await wipeServer(info)

	const event = `ptero-mrpack-${info.ref.identifier}-${Date.now().toString(36)}`
	const unlisten = await listen<{ done: number; total: number }>(event, ({ payload }) => {
		if (payload.total > 0) progress.set(Math.round((payload.done / payload.total) * 85))
	})
	let summary: MrpackSummary
	try {
		summary = await pteroInstallMrpack(info.ref.panelId, info.ref.identifier, source, event)
	} finally {
		unlisten()
	}

	progress.set(88)
	const deps = loaderFromDependencies(summary.dependencies)
	const mc = await resolveMc(info, deps.mc)
	const plan = await planLoader(info, deps.flavor, mc, deps.loader)
	await plan()
	progress.set(95)
	await recordModpackFiles(info, summary, modpack(summary))
	await afterLoaderChange(info)
}

// --- public entry points -------------------------------------------------------

export async function installContent(
	info: PteroServerInfo,
	request: Archon.Content.v1.InstallWorldContent,
	afterInstall?: () => Promise<void>,
): Promise<void> {
	if (request.content_variant === 'bare') {
		const flavor = flavorFromModloader(request.loader)
		const mc = await resolveMc(info, request.game_version)
		const plan = await planLoader(info, flavor, mc, request.version || null)
		const progress = new InstallProgress(info.ref.serverId, {
			type: 'platform',
			platform: (flavor === 'unknown' ? 'vanilla' : flavor) as Extract<
				ProgressKey,
				{ type: 'platform' }
			>['platform'],
			platform_version: request.version ?? '',
			game_version: mc,
		})
		runInBackground(progress, async () => {
			if (!request.soft_override) await wipeServer(info)
			await plan()
			if (!request.soft_override) {
				await updateManifest(info.ref, (m) => {
					m.modpack = null
				})
			}
			await afterInstall?.()
			await afterLoaderChange(info)
		})
		return
	}

	const spec = request.spec
	if (spec.platform === 'modrinth') {
		const version = await labrinth<LabrinthVersion>(`/version/${spec.version_id}`)
		const file =
			version.files.find((f) => f.primary && f.filename.endsWith('.mrpack')) ??
			version.files.find((f) => f.filename.endsWith('.mrpack'))
		if (!file) throw new Error(`${version.name} has no .mrpack file`)
		const progress = new InstallProgress(info.ref.serverId, {
			type: 'modrinth_modpack',
			project_id: spec.project_id,
			version_id: spec.version_id,
		})
		runInBackground(progress, async () => {
			await installMrpack(
				info,
				{ url: file.url, versionId: spec.version_id },
				() => ({
					platform: 'modrinth',
					project_id: spec.project_id,
					version_id: spec.version_id,
					installed_at: new Date().toISOString(),
				}),
				progress,
				request.soft_override,
			)
			await afterInstall?.()
		})
		return
	}

	throw new Error(
		spec.platform === 'curseforge'
			? "CurseForge modpacks can't be installed on Pterodactyl servers from Studio."
			: 'Upload the .mrpack file to install a local modpack.',
	)
}

/** Kyros `upload-modpack-file`: installs an uploaded .mrpack. */
export async function installUploadedModpack(
	info: PteroServerInfo,
	file: Blob,
	filename: string,
	softOverride: boolean,
	afterInstall?: () => Promise<void>,
): Promise<void> {
	if (!filename.toLowerCase().endsWith('.mrpack')) {
		throw new Error('Only Modrinth .mrpack modpacks can be uploaded to Pterodactyl servers.')
	}
	const bytes = await file.arrayBuffer()
	const progress = new InstallProgress(info.ref.serverId, { type: 'local_modpack', filename })
	runInBackground(progress, async () => {
		await installMrpack(
			info,
			{ bytes },
			(summary) => ({
				platform: 'local_file',
				filename,
				name: summary.name || filename.replace(/\.mrpack$/i, ''),
				installed_at: new Date().toISOString(),
			}),
			progress,
			softOverride,
		)
		await afterInstall?.()
	})
}

async function latestModpackVersion(projectId: string): Promise<LabrinthVersion | null> {
	const versions = await labrinth<LabrinthVersion[]>(`/project/${projectId}/version`)
	return versions[0] ?? null
}

/** Version id of a newer modpack version, or null. */
export async function modpackUpdateId(info: PteroServerInfo): Promise<string | null> {
	const manifest = await readManifest(info.ref)
	const pack = manifest.modpack
	if (pack?.platform !== 'modrinth') return null
	const latest = await latestModpackVersion(pack.project_id).catch(() => null)
	return latest && latest.id !== pack.version_id ? latest.id : null
}

export async function updateModpack(info: PteroServerInfo): Promise<void> {
	const manifest = await readManifest(info.ref)
	const pack = manifest.modpack
	if (pack?.platform !== 'modrinth') throw new Error('No Modrinth modpack is installed')
	const latest = await latestModpackVersion(pack.project_id)
	if (!latest || latest.id === pack.version_id) return
	await installContent(info, {
		content_variant: 'modpack',
		spec: { platform: 'modrinth', project_id: pack.project_id, version_id: latest.id },
		soft_override: true,
	})
}

export async function unlinkModpack(info: PteroServerInfo): Promise<void> {
	await updateManifest(info.ref, (manifest) => {
		manifest.modpack = null
		for (const entry of Object.values(manifest.files)) delete entry.from_modpack
	})
	emitPteroSyncEvent(info.ref.serverId, { type: 'protocol.reset' })
}

type VersionPlan = {
	kind: Archon.Content.v1.AddonKind
	filename: string
	projectId: string
	currentVersionId: string
	next: LabrinthVersion | null
}

/** For every Modrinth-tracked file, the version it would move to on `gameVersion` (null = none). */
async function planGameVersion(info: PteroServerInfo, gameVersion: string): Promise<VersionPlan[]> {
	const manifest = await readManifest(info.ref)
	const entries = Object.entries(manifest.files)
	const plans: VersionPlan[] = []
	let index = 0
	const worker = async () => {
		while (index < entries.length) {
			const [key, entry] = entries[index++]
			const [kind, ...rest] = key.split(':') as [Archon.Content.v1.AddonKind, ...string[]]
			const loaders = compatibleLoaders(info.flavor, kind)
			const params = new URLSearchParams({ game_versions: JSON.stringify([gameVersion]) })
			if (loaders.length) params.set('loaders', JSON.stringify(loaders))
			const versions = await labrinth<LabrinthVersion[]>(
				`/project/${entry.project_id}/version?${params}`,
			).catch(() => [] as LabrinthVersion[])
			plans.push({
				kind,
				filename: rest.join(':'),
				projectId: entry.project_id,
				currentVersionId: entry.version_id,
				next: versions[0] ?? null,
			})
		}
	}
	await Promise.all(Array.from({ length: 6 }, worker))
	return plans
}

export async function gameVersionPreview(
	info: PteroServerInfo,
	gameVersion: string,
): Promise<Archon.Content.v1.UpdateGameVersionPreview> {
	const plans = await planGameVersion(info, gameVersion)
	const changing = plans.filter((p) => !p.next || p.next.id !== p.currentVersionId)
	const projectIds = [...new Set(changing.map((p) => p.projectId))]
	const versionIds = [...new Set(changing.map((p) => p.currentVersionId))]
	const [projects, versions] = await Promise.all([
		projectIds.length
			? labrinth<(LabrinthProject & { slug: string })[]>(
					`/projects?ids=${encodeURIComponent(JSON.stringify(projectIds))}`,
				).catch(() => [])
			: [],
		versionIds.length
			? labrinth<LabrinthVersion[]>(
					`/versions?ids=${encodeURIComponent(JSON.stringify(versionIds))}`,
				).catch(() => [])
			: [],
	])
	const projectById = new Map(projects.map((p) => [p.id, p]))
	const versionById = new Map(versions.map((v) => [v.id, v]))

	const addon_changes: Archon.Content.v1.AddonDiff[] = changing.map((plan) => {
		const project = projectById.get(plan.projectId)
		const current = versionById.get(plan.currentVersionId)
		const base = {
			current_version: current ? { id: current.id, version_number: current.version_number } : null,
			new_version: plan.next ? { id: plan.next.id, version_number: plan.next.version_number } : null,
			file_name: plan.filename,
			project_id: plan.projectId,
			project: project
				? { id: project.id, title: project.title, icon_url: project.icon_url, slug: project.slug }
				: null,
		}
		return plan.next
			? {
					...base,
					type: 'updated' as const,
					current_version_id: plan.currentVersionId,
					new_version_id: plan.next.id,
				}
			: { ...base, type: 'removed' as const }
	})

	const listed = await getUntrackedCount(info).catch(() => 0)
	return {
		addon_changes,
		new_game_version: gameVersion,
		new_loader_version: info.loaderVersion ?? '',
		has_unknown_content: listed > 0,
	}
}

/** Content files that Studio can't match to a Modrinth project. */
async function getUntrackedCount(info: PteroServerInfo): Promise<number> {
	const addons = await getAddons(info, new URLSearchParams())
	return (addons.addons ?? []).filter((a) => !a.project_id).length
}

export async function changeGameVersion(info: PteroServerInfo, gameVersion: string): Promise<void> {
	const flavor = info.flavor === 'unknown' || info.flavor === 'spigot' ? info.eggFlavor : info.flavor
	if (flavor === 'spigot') {
		throw new Error('Spigot builds have to be compiled; change MINECRAFT_VERSION on the panel and reinstall.')
	}
	const plans = await planGameVersion(info, gameVersion)
	const keepLoader = flavor === 'fabric' || flavor === 'quilt' ? info.loaderVersion : null
	const plan = await planLoader(info, flavor, gameVersion, keepLoader)
	const progress = new InstallProgress(info.ref.serverId, {
		type: 'platform',
		platform: (flavor === 'unknown' || flavor === 'proxy' || flavor === 'pumpkin'
			? 'vanilla'
			: flavor) as Extract<ProgressKey, { type: 'platform' }>['platform'],
		platform_version: keepLoader ?? '',
		game_version: gameVersion,
	})
	runInBackground(progress, async () => {
		await plan()
		const fresh = await afterLoaderChange(info)
		progress.set(60)
		// Move mods/plugins to versions for the new game version; disable the
		// ones that have none (they're kept, just switched off).
		const updates = plans.filter((p) => p.next && p.next.id !== p.currentVersionId)
		for (const [i, p] of updates.entries()) {
			await updateAddons(fresh, [{ filename: p.filename, version_id: p.next!.id }], p.kind).catch(
				(error) => console.warn('[pterodactyl] update failed', p.filename, error),
			)
			progress.set(60 + Math.round(((i + 1) / Math.max(1, updates.length)) * 35))
		}
		const incompatible = plans.filter((p) => !p.next)
		if (incompatible.length) {
			await setAddonEnabled(
				fresh,
				incompatible.map((p) => ({ kind: p.kind, filename: p.filename, enabled: false })),
			).catch(() => undefined)
		}
	})
}
