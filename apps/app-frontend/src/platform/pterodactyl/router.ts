/**
 * Modrinth Studios addition: maps Archon/Kyros requests made by the hosting
 * UI onto Pterodactyl for `ptero-…` servers. Anything not handled here
 * throws a clear "not supported on Pterodactyl" error instead of silently
 * hitting Modrinth's servers with a fake id.
 */
import { type Archon, isLocalFile, ModrinthApiError, type RequestContext } from '@modrinth/api-client'

import { listActivity } from './activity'
import {
	backupDownloadUrl,
	createBackup,
	deleteBackup,
	listBackupsQueue,
	patchBackupOptions,
	restoreBackup,
	setBackupLocked,
} from './backups'
import { pteroRequest, pteroUploadLocal, pteroWrite } from './bridge'
import {
	addAddons,
	deleteAddons,
	getAddons,
	labrinth,
	setAddonEnabled,
	updateAddons,
} from './content'
import {
	compress,
	createFolder,
	decompress,
	deleteFiles,
	listDir,
	listDirectory,
	moveFile,
	normalizePath,
	readBlob,
	readText,
	signedDownloadUrl,
	stat,
	writeBlob,
	writeText,
} from './files'
import {
	parsePteroServerId,
	PTERO_FAKE_JWT,
	PTERO_WORLD_ID,
	type PteroServerRef,
	pteroNodeHost,
} from './ids'
import {
	contentDirectory,
	fetchServerInfo,
	invalidatePteroServerInfo,
	javaVersionFromImage,
	type PteroAllocation,
	type PteroServerInfo,
	toArchonServerV0,
	toArchonServerV1,
} from './server'
import {
	changeGameVersion,
	gameVersionPreview,
	installContent,
	installUploadedModpack,
	modpackUpdateId,
	unlinkModpack,
	updateModpack,
	wipeServer,
} from './install'
import { updateStudioSettings } from './settings'
import { createPteroSyncStream, emitPteroSyncEvent } from './socket'

export class PteroUnsupportedError extends ModrinthApiError {
	constructor(feature: string) {
		super(`${feature} isn't available for Pterodactyl servers in Modrinth Studio yet.`, {
			statusCode: 501,
		})
	}
}

function notFound(message: string) {
	return new ModrinthApiError(message, { statusCode: 404 })
}

type Route = {
	method: string
	segments: string[]
	params: Record<string, unknown>
	body: unknown
	query: URLSearchParams
	context: RequestContext
	ref: PteroServerRef
	info: () => Promise<PteroServerInfo>
}

function splitPath(path: string): { segments: string[]; query: URLSearchParams } {
	const [pathname, search = ''] = path.split('?')
	return {
		segments: pathname.split('/').filter(Boolean).map(decodeURIComponent),
		query: new URLSearchParams(search),
	}
}

function serverPath(ref: PteroServerRef, rest: string) {
	return `/servers/${ref.identifier}${rest}`
}

async function bodyText(body: unknown): Promise<string> {
	if (typeof body === 'string') return body
	if (body instanceof Blob) return body.text()
	if (body instanceof ArrayBuffer) return new TextDecoder().decode(body)
	return JSON.stringify(body ?? '')
}

function afterMutation(ref: PteroServerRef) {
	invalidatePteroServerInfo(ref.serverId)
}

// --- subuser roles -----------------------------------------------------------

const VIEWER_PERMS = [
	'websocket.connect',
	'control.start',
	'control.stop',
	'control.restart',
	'file.read',
	'file.read-content',
	'backup.read',
	'allocation.read',
	'startup.read',
	'schedule.read',
	'activity.read',
]
const EDITOR_ONLY = [
	'control.console',
	'file.create',
	'file.update',
	'file.delete',
	'file.archive',
	'file.sftp',
	'backup.create',
	'backup.delete',
	'backup.download',
	'backup.restore',
	'allocation.create',
	'allocation.update',
	'allocation.delete',
	'startup.update',
	'startup.docker-image',
	'schedule.create',
	'schedule.update',
	'schedule.delete',
	'settings.rename',
	'settings.reinstall',
]

// --- server.properties -----------------------------------------------------

const KNOWN_PROPERTIES = [
	'allow-cheats',
	'allow-flight',
	'difficulty',
	'enforce-whitelist',
	'force-gamemode',
	'gamemode',
	'generate-structures',
	'generator-settings',
	'hardcore',
	'level-seed',
	'level-type',
	'max-players',
	'max-tick-time',
	'motd',
	'pause-when-empty-seconds',
	'player-idle-timeout',
	'require-resource-pack',
	'resource-pack',
	'resource-pack-id',
	'resource-pack-sha1',
	'simulation-distance',
	'spawn-protection',
	'sync-chunk-writes',
	'view-distance',
	'white-list',
]

function parseProperties(text: string): Map<string, string> {
	const map = new Map<string, string>()
	for (const line of text.split(/\r?\n/)) {
		if (!line.trim() || line.trimStart().startsWith('#')) continue
		const index = line.indexOf('=')
		if (index === -1) continue
		map.set(line.slice(0, index).trim(), line.slice(index + 1))
	}
	return map
}

async function getProperties(ref: PteroServerRef): Promise<Archon.Content.v1.PropertiesFields> {
	const text = (await readText(ref, '/server.properties')) ?? ''
	const map = parseProperties(text)
	const known: Record<string, string | null> = {}
	const custom: Record<string, string> = {}
	for (const [key, value] of map) {
		if (KNOWN_PROPERTIES.includes(key)) known[key.replace(/-/g, '_')] = value
		else custom[key] = value
	}
	return { known: known as Archon.Content.v1.KnownPropertiesFields, custom }
}

async function patchProperties(ref: PteroServerRef, patch: Archon.Content.v1.PatchPropertiesFields) {
	const original = (await readText(ref, '/server.properties')) ?? ''
	const updates = new Map<string, string | null>()
	for (const [key, value] of Object.entries(patch.known ?? {})) {
		if (value !== undefined) updates.set(key.replace(/_/g, '-'), value)
	}
	for (const [key, value] of Object.entries(patch.custom ?? {})) updates.set(key, value)

	const lines = original ? original.split(/\r?\n/) : []
	const seen = new Set<string>()
	const out: string[] = []
	for (const line of lines) {
		const index = line.indexOf('=')
		const key = index === -1 || line.trimStart().startsWith('#') ? null : line.slice(0, index).trim()
		if (key && updates.has(key)) {
			seen.add(key)
			const value = updates.get(key)
			if (value !== null) out.push(`${key}=${value}`)
			continue
		}
		out.push(line)
	}
	for (const [key, value] of updates) {
		if (!seen.has(key) && value !== null) out.push(`${key}=${value}`)
	}
	await writeText(ref, '/server.properties', out.join('\n').replace(/\n*$/, '\n'))
}

// --- startup ---------------------------------------------------------------

type PteroStartup = {
	data: { attributes: { env_variable: string; server_value: string | null; default_value: string } }[]
	meta: { startup_command: string; raw_startup_command: string; docker_images: Record<string, string> }
}

async function getStartup(info: PteroServerInfo): Promise<Archon.Content.v1.RuntimeOptions> {
	const startup = await pteroRequest<PteroStartup>(
		info.ref.panelId,
		'GET',
		serverPath(info.ref, '/startup'),
	)
	return {
		java_version: javaVersionFromImage(info.attributes.docker_image),
		jre_vendor: 'temurin',
		original_invocation: startup.meta.raw_startup_command,
		startup_command: startup.meta.startup_command,
	}
}

async function patchStartup(info: PteroServerInfo, patch: Archon.Content.v1.PatchRuntimeOptions) {
	const startup = await pteroRequest<PteroStartup>(
		info.ref.panelId,
		'GET',
		serverPath(info.ref, '/startup'),
	)
	if (
		patch.java_version !== undefined &&
		patch.java_version !== null &&
		patch.java_version !== javaVersionFromImage(info.attributes.docker_image)
	) {
		const images = Object.values(startup.meta.docker_images ?? {})
		const image = images.find((img) => javaVersionFromImage(img) === patch.java_version)
		if (!image) {
			throw new ModrinthApiError(
				`This egg has no Java ${patch.java_version} image. Available: ${images.join(', ') || 'none'}`,
				{ statusCode: 400 },
			)
		}
		await pteroRequest(info.ref.panelId, 'PUT', serverPath(info.ref, '/settings/docker-image'), {
			body: { docker_image: image },
		})
	}
	const command = patch.startup_command?.trim()
	if (
		command &&
		command !== startup.meta.startup_command.trim() &&
		command !== startup.meta.raw_startup_command.trim()
	) {
		throw new ModrinthApiError(
			"Pterodactyl only lets panel admins change a server's startup command. Edit the egg's startup variables on the panel instead.",
			{ statusCode: 400 },
		)
	}
	afterMutation(info.ref)
}

// --- network ---------------------------------------------------------------

async function listAllocations(ref: PteroServerRef): Promise<PteroAllocation[]> {
	try {
		const response = await pteroRequest<{ data: { attributes: PteroAllocation }[] }>(
			ref.panelId,
			'GET',
			serverPath(ref, '/network/allocations'),
		)
		return response.data.map((d) => d.attributes)
	} catch {
		// No allocation permission on this key: the server object still lists them (read-only).
		const info = await fetchServerInfo(ref)
		return (info.attributes.relationships?.allocations?.data ?? []).map((d) => d.attributes)
	}
}

function allocationError(action: string, error: unknown): never {
	const status = (error as { status?: number })?.status
	throw new ModrinthApiError(
		status === 403
			? `Your panel account or API key isn't allowed to ${action}. Ask a panel admin for the allocation permissions on this server.`
			: status === 400 || status === 422
				? `Couldn't ${action}: the server is at its allocation limit or the panel has automatic allocation creation turned off (Admin → Settings → Advanced). A panel admin can change both.`
				: `Couldn't ${action}: ${error instanceof Error ? error.message : String(error)}`,
		{ statusCode: status ?? 400 },
	)
}

async function allocationByPort(ref: PteroServerRef, port: number): Promise<PteroAllocation> {
	const allocation = (await listAllocations(ref)).find((a) => a.port === port)
	if (!allocation) throw notFound(`No allocation on port ${port}`)
	return allocation
}

// --- icon ------------------------------------------------------------------

async function sha256Hex(blob: Blob): Promise<string> {
	const digest = await crypto.subtle.digest('SHA-256', await blob.arrayBuffer())
	return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

async function readIcon(ref: PteroServerRef): Promise<Blob> {
	const root = await listDir(ref, '/')
	if (!root.some((f) => f.name === 'server-icon.png')) throw notFound('No server icon')
	return readBlob(ref, '/server-icon.png')
}

async function toMinecraftIcon(image: Blob): Promise<Blob> {
	const bitmap = await createImageBitmap(image)
	const canvas = document.createElement('canvas')
	canvas.width = 64
	canvas.height = 64
	canvas.getContext('2d')!.drawImage(bitmap, 0, 0, 64, 64)
	return new Promise((resolve, reject) =>
		canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error('Icon conversion failed'))), 'image/png'),
	)
}

// --- upload sessions -------------------------------------------------------

function uploadSession(id: string, status: 'active' | 'finalized' | 'cancelled') {
	const now = Date.now()
	return {
		upload_id: id,
		status,
		created_at: now,
		updated_at: now,
		last_upload_at: null,
		expires_at: now + 3_600_000,
		entry_count: 0,
		uploaded_byte_count: 0,
	}
}

async function uploadSessionFiles(info: PteroServerInfo, scope: string, formData: FormData) {
	for (const entry of formData.getAll('file')) {
		if (!(entry instanceof Blob)) continue
		const filename = entry instanceof File ? entry.name : 'upload'
		let path: string
		if (scope === 'files') {
			path = normalizePath(filename)
		} else {
			const lower = filename.toLowerCase()
			const dir = lower.endsWith('.zip') ? '/world/datapacks' : (contentDirectory(info.flavor) ?? '/')
			path = normalizePath(`${dir}/${filename.split('/').pop()}`)
		}
		await pteroWrite(info.ref.panelId, info.ref.identifier, path, await entry.arrayBuffer(), 'upload')
	}
}

// --- routing ---------------------------------------------------------------

async function routeArchon(route: Route): Promise<unknown> {
	const { method, ref, body } = route
	// segments: servers/<id>/...
	const rest = route.segments.slice(2)
	const at = (i: number) => rest[i]
	const version = route.context.options.version

	if (rest.length === 0 && method === 'GET') {
		const info = await route.info()
		return version === 1 ? toArchonServerV1(info) : toArchonServerV0(info)
	}

	switch (at(0)) {
		case 'fs':
			return { url: `https://${pteroNodeHost(ref.serverId)}/modrinth/v0/fs`, token: PTERO_FAKE_JWT }
		case 'ws':
			return { url: `ptero://${ref.serverId}`, token: PTERO_FAKE_JWT }
		case 'power': {
			const action = String((body as { action?: string })?.action ?? '').toLowerCase()
			await pteroRequest(ref.panelId, 'POST', serverPath(ref, '/power'), {
				body: { signal: action },
			})
			return undefined
		}
		case 'name': {
			await pteroRequest(ref.panelId, 'POST', serverPath(ref, '/settings/rename'), {
				body: { name: (body as { name: string }).name },
			})
			afterMutation(ref)
			emitPteroSyncEvent(ref.serverId, {
				type: 'server.patch',
				name: (body as { name: string }).name,
				subdomain: (await route.info()).settings.address ?? '',
			})
			return undefined
		}
		case 'allocations': {
			if (method === 'GET' && !at(1)) {
				return (await listAllocations(ref))
					.filter((a) => !a.is_default)
					.map((a) => ({ port: a.port, name: a.notes ?? '' }))
			}
			if (method === 'POST' && !at(1)) {
				const created = await pteroRequest<{ attributes: PteroAllocation }>(
					ref.panelId,
					'POST',
					serverPath(ref, '/network/allocations'),
				).catch((error) => allocationError('add a port', error))
				const name = (body as { name?: string })?.name ?? ''
				if (name) {
					await pteroRequest(
						ref.panelId,
						'POST',
						serverPath(ref, `/network/allocations/${created.attributes.id}`),
						{ body: { notes: name } },
					)
				}
				return { port: created.attributes.port, name }
			}
			const allocation = await allocationByPort(ref, Number(at(1)))
			if (method === 'PUT') {
				await pteroRequest(
					ref.panelId,
					'POST',
					serverPath(ref, `/network/allocations/${allocation.id}`),
					{ body: { notes: (body as { name?: string })?.name ?? '' } },
				).catch((error) => allocationError('rename this port', error))
				return undefined
			}
			if (method === 'DELETE') {
				await pteroRequest(
					ref.panelId,
					'DELETE',
					serverPath(ref, `/network/allocations/${allocation.id}`),
				).catch((error) => allocationError('remove this port', error))
				return undefined
			}
			break
		}
		case 'startup': {
			// legacy v0 startup config
			const info = await route.info()
			if (method === 'GET') {
				const startup = await getStartup(info)
				const java = startup.java_version ?? 21
				return {
					invocation: startup.startup_command ?? '',
					original_invocation: startup.original_invocation ?? '',
					jdk_version: java >= 21 ? 'lts21' : java >= 17 ? 'lts17' : java >= 11 ? 'lts11' : 'lts8',
					jdk_build: 'temurin',
				}
			}
			throw new PteroUnsupportedError('Changing the startup command')
		}
		case 'notices':
		case 'flows':
			return undefined
		case 'reinstall': {
			const request = body as
				| { loader?: string; loader_version?: string; game_version?: string; project_id?: string; version_id?: string }
				| undefined
			const info = await route.info()
			const hard = String(route.params.hard ?? 'false') === 'true'
			if (request?.project_id) {
				const versionId =
					request.version_id ??
					(await labrinth<{ id: string }[]>(`/project/${request.project_id}/version`))[0]?.id
				if (!versionId) throw notFound('This modpack has no versions')
				await installContent(info, {
					content_variant: 'modpack',
					spec: { platform: 'modrinth', project_id: request.project_id, version_id: versionId },
					soft_override: !hard,
				})
				return undefined
			}
			if (request?.loader) {
				await installContent(info, {
					content_variant: 'bare',
					loader: request.loader.toLowerCase().replace('neoforge', 'neo_forge') as Archon.Content.v1.Modloader,
					version: request.loader_version ?? '',
					game_version: request.game_version ?? undefined,
					soft_override: !hard,
				})
				return undefined
			}
			if (hard) await wipeServer(info)
			await pteroRequest(ref.panelId, 'POST', serverPath(ref, '/settings/reinstall'))
			return undefined
		}
		case 'reinstallFromMrpack':
			return { url: pteroNodeHost(ref.serverId), token: PTERO_FAKE_JWT }
		case 'subdomain': {
			// Studio stores a custom address (your own domain) instead of a modrinth.gg subdomain.
			const address = String((body as { subdomain?: string })?.subdomain ?? '')
				.trim()
				.toLowerCase()
			await updateStudioSettings(ref, (settings) => {
				settings.address = address || null
			})
			const info = await route.info()
			emitPteroSyncEvent(ref.serverId, {
				type: 'server.patch',
				name: info.attributes.name,
				subdomain: address,
			})
			return undefined
		}
		case 'sftp':
			throw new PteroUnsupportedError('Resetting SFTP credentials (use your panel password)')
		case 'upgrade-stock':
			throw new PteroUnsupportedError('Upgrading')
		case 'users': {
			const userId = at(1)
			if (method === 'GET' && !userId) {
				const response = await pteroRequest<{
					data: {
						attributes: {
							uuid: string
							username: string
							image: string
							created_at: string
							permissions: string[]
						}
					}[]
				}>(ref.panelId, 'GET', serverPath(ref, '/users')).catch(() => ({ data: [] }))
				return response.data.map(({ attributes }) => ({
					user: { id: attributes.uuid, username: attributes.username, avatar_url: attributes.image },
					added_on: attributes.created_at,
					last_invite_sent: null,
					permissions: attributes.permissions.some((p) => EDITOR_ONLY.includes(p))
						? 'BASE_READ|POWER_ACTIONS|EXEC_COMMANDS|FILES_WRITE|SETUP|BACKUPS|ADVANCED'
						: 'BASE_READ|POWER_ACTIONS',
				})) satisfies Archon.ServerUsers.v1.ServerUser[]
			}
			if (method === 'PATCH' && userId) {
				let role = body as string
				if (typeof role === 'string') {
					try {
						role = JSON.parse(role)
					} catch {
						// already plain
					}
				}
				await pteroRequest(ref.panelId, 'POST', serverPath(ref, `/users/${userId}`), {
					body: { permissions: role === 'Editor' ? [...VIEWER_PERMS, ...EDITOR_ONLY] : VIEWER_PERMS },
				})
				return undefined
			}
			if (method === 'DELETE' && userId) {
				await pteroRequest(ref.panelId, 'DELETE', serverPath(ref, `/users/${userId}`))
				return undefined
			}
			if (method === 'POST' && !userId) {
				const request = body as Archon.ServerUsers.v1.AddServerUserRequest
				const email = request.user_id.replace(/^email:/, '')
				if (!email.includes('@')) {
					throw new ModrinthApiError('Enter the email address of their panel account.', {
						statusCode: 400,
					})
				}
				await pteroRequest(ref.panelId, 'POST', serverPath(ref, '/users'), {
					body: {
						email,
						permissions:
							request.role === 'Viewer' ? VIEWER_PERMS : [...VIEWER_PERMS, ...EDITOR_ONLY],
					},
				})
				return undefined
			}
			if (at(2) === 'reinvite') return { sent: false, cooldown_seconds: null }
			throw new PteroUnsupportedError(
				'Inviting Modrinth accounts (add subusers by email on the panel, then manage their role here)',
			)
		}
		case 'action-log':
			if (method === 'DELETE') {
				// Pterodactyl keeps its log; Studio just hides everything before now.
				await updateStudioSettings(ref, (settings) => {
					settings.activity_cleared_at = new Date().toISOString()
				})
				return undefined
			}
			return listActivity(ref, route.query, (await route.info()).settings.activity_cleared_at)
		case 'icon': {
			if (at(1) === 'metadata') {
				const icon = await readIcon(ref)
				return { hash: await sha256Hex(icon), config: null }
			}
			if (method === 'GET') return readIcon(ref)
			if (method === 'DELETE') {
				await deleteFiles(ref, ['/server-icon.png'])
				emitPteroSyncEvent(ref.serverId, { type: 'server.icon.patch', server_icon_hash: null })
				return undefined
			}
			if (method === 'PUT') {
				let image: Blob | null = null
				if (body instanceof Blob) image = body
				else if (body instanceof FormData) {
					const entry = [...body.values()].find((v) => v instanceof Blob)
					image = (entry as Blob | undefined) ?? null
				}
				if (!image) throw new PteroUnsupportedError('Generated icons')
				const icon = await toMinecraftIcon(image)
				await pteroWrite(ref.panelId, ref.identifier, '/server-icon.png', await icon.arrayBuffer(), 'upload')
				const hash = await sha256Hex(icon)
				emitPteroSyncEvent(ref.serverId, { type: 'server.icon.patch', server_icon_hash: hash })
				return { hash, config: null }
			}
			break
		}
		case 'worlds':
			return routeWorld(route, rest.slice(2))
	}
	throw new PteroUnsupportedError(`${method} ${rest.join('/')}`)
}

async function routeWorld(route: Route, rest: string[]): Promise<unknown> {
	const { method, ref, body, query } = route
	const at = (i: number) => rest[i]

	// World renames have no Pterodactyl equivalent; accept them silently.
	if (rest.length === 0 && method === 'PATCH') return undefined

	switch (at(0)) {
		case 'select-download':
			return { method_type: 'direct_node_download' }
		case 'onboard':
			throw new PteroUnsupportedError('Resetting to onboarding')
		case 'properties':
			if (method === 'GET') return getProperties(ref)
			await patchProperties(ref, body as Archon.Content.v1.PatchPropertiesFields)
			return undefined
		case 'options':
			if (at(1) === 'startup') {
				const info = await route.info()
				if (method === 'GET') return getStartup(info)
				await patchStartup(info, body as Archon.Content.v1.PatchRuntimeOptions)
				return getStartup(await route.info())
			}
			break
		case 'addons': {
			const info = await route.info()
			const action = at(1)
			if (!action && method === 'GET') return getAddons(info, query)
			if (!action && method === 'POST') {
				await addAddons(info, [body as Archon.Content.v1.AddAddonRequest])
				return undefined
			}
			switch (action) {
				case 'install-many':
					await addAddons(info, body as Archon.Content.v1.AddAddonRequest[])
					return undefined
				case 'delete':
					await deleteAddons(info, [body as Archon.Content.v1.RemoveAddonRequest])
					return undefined
				case 'delete-many':
					await deleteAddons(info, (body as { items: Archon.Content.v1.RemoveAddonRequest[] }).items)
					return undefined
				case 'disable':
				case 'enable':
					await setAddonEnabled(info, [
						{ ...(body as Archon.Content.v1.RemoveAddonRequest), enabled: action === 'enable' },
					])
					return undefined
				case 'disable-many':
				case 'enable-many':
					await setAddonEnabled(
						info,
						(body as { items: Archon.Content.v1.RemoveAddonRequest[] }).items.map((item) => ({
							...item,
							enabled: action === 'enable-many',
						})),
					)
					return undefined
				case 'set-enabled-server':
					await setAddonEnabled(info, [body as Archon.Content.v1.SetAddonEnabledRequest])
					return undefined
				case 'set-enabled-player':
				case 'set-locked-side-toggle':
				case 'complete-side-type-review':
					// Client/player sides only exist for shared instances on Modrinth Hosting.
					return undefined
				case 'update': {
					if (method === 'GET') {
						const filename = query.get('filename') ?? ''
						const addons = await getAddons(info, new URLSearchParams({ updates: 'true' }))
						const addon = addons.addons?.find((a) => a.filename === filename)
						if (!addon) throw notFound(`${filename} not found`)
						return addon
					}
					await updateAddons(info, [body as Archon.Content.v1.UpdateAddonRequest])
					return undefined
				}
				case 'update-many':
					await updateAddons(
						info,
						(body as { addons: Archon.Content.v1.UpdateAddonRequest[] }).addons,
					)
					return undefined
			}
			break
		}
		case 'content': {
			const action = at(1)
			const info = await route.info()
			if (!action && method === 'POST') {
				const request = body as Archon.Content.v1.InstallWorldContent
				await installContent(info, request, async () => {
					if (request.properties) await patchProperties(ref, request.properties)
				})
				return undefined
			}
			if (action === 'unlink-modpack') {
				await unlinkModpack(info)
				return undefined
			}
			if (action === 'repair') return undefined
			if (action === 'restore-client-files') return { backup_id: null }
			if (action === 'share') throw new PteroUnsupportedError('Sharing server content as an instance')
			if (action === 'update-game-version') {
				const gameVersion = query.get('game_version') ?? ''
				if (method === 'GET') return gameVersionPreview(info, gameVersion)
				await changeGameVersion(info, gameVersion)
				return undefined
			}
			if (action === 'modpack' && at(2) === 'update') {
				if (method === 'POST') {
					await updateModpack(info)
					return undefined
				}
				const fields = (await getAddons(info, new URLSearchParams())).modpack
				if (!fields) throw notFound('No modpack installed')
				return { ...fields, has_update: await modpackUpdateId(info) }
			}
			break
		}
		case 'backups-queue':
		case 'backups':
			return routeBackups(route, rest.slice(1), at(0) === 'backups')
	}
	throw new PteroUnsupportedError(`${method} worlds/${rest.join('/')}`)
}

async function routeBackups(route: Route, rest: string[], legacy: boolean): Promise<unknown> {
	const { method, ref, body } = route
	const at = (i: number) => rest[i]
	if (rest.length === 0 && method === 'GET') {
		const queue = await listBackupsQueue(await route.info(), false)
		if (!legacy) return queue
		return queue.backups.map((b) => ({
			id: b.id,
			physical_id: b.id,
			name: b.name,
			created_at: b.created_at,
			automated: false,
			status: b.status,
			interrupted: false,
			ongoing: b.status === 'in_progress',
			locked: b.locked,
		}))
	}
	if (at(0) === 'with-options') {
		const info = await route.info()
		if (method === 'GET') return listBackupsQueue(info, true)
		return patchBackupOptions(
			info,
			body as Archon.BackupsQueue.v1.PatchWorldBackupOptions,
			async ({ split_automatic, interval }) => {
				await updateStudioSettings(ref, (settings) => {
					if (split_automatic !== undefined && split_automatic > 0)
						settings.backup_split_automatic = split_automatic
					if (interval !== undefined) settings.backup_interval = interval
				})
			},
		)
	}
	if (rest.length === 0 && method === 'POST') {
		const result = await createBackup(ref, (body as { name?: string })?.name)
		await updateStudioSettings(ref, (settings) => {
			settings.manual_backup_ids = [...(settings.manual_backup_ids ?? []), result.id].slice(-200)
		}).catch(() => undefined)
		emitPteroSyncEvent(ref.serverId, { type: 'backup.new', id: result.id })
		return result
	}
	if (at(0) === 'history') {
		if (at(3) === 'ack') return undefined
		throw new PteroUnsupportedError('Cancelling backup operations')
	}
	if (at(0) === 'delete-many') {
		for (const id of (body as { backup_ids: string[] }).backup_ids) await deleteBackup(ref, id)
		return undefined
	}
	const backupId = at(0)
	if (backupId && !at(1)) {
		if (method === 'DELETE') {
			await deleteBackup(ref, backupId)
			emitPteroSyncEvent(ref.serverId, {
				type: 'backup.delete',
				world_id: PTERO_WORLD_ID,
				backup_id: backupId,
			})
			return undefined
		}
		if (method === 'PATCH') {
			const patch = body as { name?: string; locked?: boolean }
			if (patch.locked !== undefined) await setBackupLocked(ref, backupId, patch.locked)
			if (patch.name !== undefined) {
				// Pterodactyl can't rename backups; Studio keeps a display name.
				await updateStudioSettings(ref, (settings) => {
					settings.backup_names = { ...(settings.backup_names ?? {}), [backupId]: patch.name! }
				})
			}
			return undefined
		}
		if (method === 'GET') {
			const queue = await listBackupsQueue(await route.info(), false)
			const backup = queue.backups.find((b) => b.id === backupId)
			if (!backup) throw notFound('Backup not found')
			return backup
		}
	}
	if (at(1) === 'restore') {
		await restoreBackup(ref, backupId)
		return undefined
	}
	if (at(1) === 'retry' || at(1) === 'undelete') throw new PteroUnsupportedError('Retrying backups')
	throw new PteroUnsupportedError(`${method} backups/${rest.join('/')}`)
}

async function routeNode(route: Route): Promise<unknown> {
	const { method, ref, params, body, context } = route
	const segments = route.segments
	const version = String(context.options.version ?? '')
	const path = `/${segments.join('/')}`

	// legacy .mrpack reinstall upload
	if (version === 'reinstallMrpackMultiparted') {
		const formData = (context.metadata as { formData?: FormData } | undefined)?.formData
		const file = formData?.get('file')
		if (!(file instanceof Blob)) throw new ModrinthApiError('No modpack file', { statusCode: 400 })
		await installUploadedModpack(
			await route.info(),
			file,
			file instanceof File ? file.name : 'modpack.mrpack',
			String(params.hard ?? 'false') !== 'true',
		)
		return undefined
	}

	// v0 filesystem
	if (path === '/fs/list') {
		return listDirectory(ref, String(params.path ?? '/'), Number(params.page ?? 1), Number(params.page_size ?? 100))
	}
	if (path === '/fs/create') {
		const target = String(params.path ?? '/')
		const metadata = context.metadata as { isUpload?: boolean; file?: unknown } | undefined
		if (metadata?.isUpload && metadata.file) {
			const file = metadata.file
			if (isLocalFile(file)) {
				await pteroUploadLocal(ref.panelId, ref.identifier, normalizePath(target), file.path)
			} else {
				await pteroWrite(ref.panelId, ref.identifier, normalizePath(target), await (file as Blob).arrayBuffer(), 'upload')
			}
			return undefined
		}
		if (params.type === 'directory') await createFolder(ref, target)
		else await writeText(ref, target, '')
		return undefined
	}
	if (path === '/fs/download') return readBlob(ref, String(params.path ?? '/'))
	if (path === '/fs/update') {
		const target = String(params.path ?? '/')
		if (body instanceof Blob) await writeBlob(ref, target, body)
		else await writeText(ref, target, await bodyText(body))
		return undefined
	}
	if (path === '/fs/move') {
		const { source, destination } = body as { source: string; destination: string }
		await moveFile(ref, source, destination)
		return undefined
	}
	if (path === '/fs/delete') {
		await deleteFiles(ref, [String(params.path)])
		return undefined
	}
	if (path === '/fs/unarchive') {
		if (params.dry === true || params.dry === 'true') return { modpack_name: null, conflicting_files: [] }
		await decompress(ref, String(params.src))
		return { modpack_name: null, conflicting_files: [] }
	}
	if (path.startsWith('/fs/ops/')) return undefined
	if (path === '/logs/clear') return undefined

	// v1 world-scoped endpoints: /worlds/<world>/...
	if (segments[0] === 'worlds') {
		const rest = segments.slice(2)
		const sub = `/${rest.join('/')}`
		if (sub === '/files/stat') return stat(ref, (body as { path: string }).path)
		if (sub === '/files/contents-raw/authorize') {
			return { token: await signedDownloadUrl(ref, String(params.path ?? '/')) }
		}
		if (sub === '/files/download-full-zip/authorize') {
			const archive = await compressWorld(ref)
			return { token: await signedDownloadUrl(ref, archive) }
		}
		if (rest[0] === 'backups' && rest[2] === 'download' && rest[3] === 'authorize') {
			return { token: await backupDownloadUrl(ref, rest[1]) }
		}
		if (rest[0] === 'content' && rest[1] === 'embedded-icon') throw notFound('No embedded icon')
		if (rest[1] === 'upload-session') {
			const info = await route.info()
			const scope = rest[0]
			const uploadId = rest[2]
			if (!uploadId) {
				if (method === 'GET') return { session: null }
				return uploadSession(`ptero-${Date.now().toString(36)}`, 'active')
			}
			if (rest[3] === 'files') {
				const metadata = context.metadata as { formData?: FormData } | undefined
				if (metadata?.formData) await uploadSessionFiles(info, scope, metadata.formData)
				return uploadSession(uploadId, 'active')
			}
			if (rest[3] === 'finalize') {
				emitPteroSyncEvent(ref.serverId, { type: 'protocol.reset' })
				return uploadSession(uploadId, 'finalized')
			}
			if (method === 'DELETE') return uploadSession(uploadId, 'cancelled')
		}
		if (rest[0] === 'content' && rest[1] === 'upload-modpack-file') {
			const info = await route.info()
			const formData = (context.metadata as { formData?: FormData } | undefined)?.formData
			const file = formData?.get('file')
			if (!(file instanceof Blob)) {
				throw new PteroUnsupportedError('Re-installing a previously uploaded modpack')
			}
			let properties: Archon.Content.v1.PropertiesFields | null = null
			try {
				const raw = formData?.get('properties')
				properties = typeof raw === 'string' ? JSON.parse(raw) : null
			} catch {
				properties = null
			}
			await installUploadedModpack(
				info,
				file,
				file instanceof File ? file.name : 'modpack.mrpack',
				String(params.soft_override ?? 'false') === 'true',
				async () => {
					if (properties) await patchProperties(ref, properties)
				},
			)
			return { backup_id: null }
		}
		if (rest[0] === 'content' && rest[1] === 'upload-addon-file') {
			const info = await route.info()
			const formData = (context.metadata as { formData?: FormData } | undefined)?.formData
			if (formData) await uploadSessionFiles(info, 'content', formData)
			emitPteroSyncEvent(ref.serverId, { type: 'protocol.reset' })
			return undefined
		}
		if (rest[0] === 'content' && rest[1] === 'flameanvil')
			throw new PteroUnsupportedError('CurseForge modpack import')
	}
	throw new PteroUnsupportedError(`${method} ${version} ${path}`)
}

/** Zips the world folders so the "download world" button has one file to fetch. */
async function compressWorld(ref: PteroServerRef): Promise<string> {
	const properties = parseProperties((await readText(ref, '/server.properties')) ?? '')
	const level = properties.get('level-name')?.trim() || 'world'
	const root = await listDir(ref, '/')
	const worldDirs = root
		.filter((f) => !f.is_file && (f.name === level || f.name.startsWith(`${level}_`)))
		.map((f) => f.name)
	if (worldDirs.length === 0) throw notFound(`No "${level}" folder found to download`)
	const archive = await compress(ref, '/', worldDirs)
	// Clean the archive up a while later so it doesn't eat disk space.
	setTimeout(() => void deleteFiles(ref, [`/${archive.name}`]).catch(() => undefined), 30 * 60_000)
	return `/${archive.name}`
}

/** Zip stream for files.createZip (application/json-seq progress records). */
async function createZipStream(ref: PteroServerRef, body: unknown): Promise<ReadableStream<Uint8Array>> {
	const request = body as
		| { target_type: 'Directory'; path: string }
		| { target_type: 'ManyPaths'; parent: string; include: string[]; target: string }
	if (request.target_type === 'Directory') {
		const { dir, name } = (() => {
			const normalized = normalizePath(request.path)
			const index = normalized.lastIndexOf('/')
			return { dir: normalized.slice(0, index) || '/', name: normalized.slice(index + 1) }
		})()
		await compress(ref, dir, [name])
	} else {
		const created = await compress(ref, request.parent, request.include)
		if (request.target) {
			const parent = normalizePath(request.parent)
			const createdPath = `${parent === '/' ? '' : parent}/${created.name}`
			const targetPath = normalizePath(
				request.target.startsWith('/') ? request.target : `${parent}/${request.target}`,
			)
			if (createdPath !== targetPath) await moveFile(ref, createdPath, targetPath).catch(() => undefined)
		}
	}
	const encoder = new TextEncoder()
	return new ReadableStream({
		start(controller) {
			controller.enqueue(encoder.encode(`\u001e${JSON.stringify({ progress: 1, done: true })}\n`))
			controller.close()
		},
	})
}

/**
 * Entry point used by PterodactylFeature. Returns `{ handled: false }` when
 * the request isn't for a Pterodactyl server.
 */
export async function handlePteroRequest(
	context: RequestContext,
	serverId: string,
	kind: 'archon' | 'node' | 'sync',
): Promise<unknown> {
	const ref = parsePteroServerId(serverId)
	const { segments, query } = splitPath(context.path)
	const params = (context.options.params ?? {}) as Record<string, unknown>
	for (const [key, value] of Object.entries(params)) {
		if (!query.has(key) && value !== undefined && value !== null) query.set(key, String(value))
	}
	const route: Route = {
		method: (context.options.method ?? 'GET').toUpperCase(),
		segments,
		params,
		body: context.options.body,
		query,
		context,
		ref,
		info: () => fetchServerInfo(ref),
	}

	try {
		if (kind === 'sync') return createPteroSyncStream(serverId)
		if (kind === 'node') {
			if (segments.join('/').endsWith('files/zip')) return createZipStream(ref, route.body)
			return await routeNode(route)
		}
		return await routeArchon(route)
	} catch (error) {
		if (error instanceof ModrinthApiError) throw error
		const message = error instanceof Error ? error.message : (error as { message?: string })?.message ?? String(error)
		const status = (error as { status?: number; statusCode?: number })?.status ??
			(error as { statusCode?: number })?.statusCode
		throw new ModrinthApiError(message, { statusCode: status })
	}
}
