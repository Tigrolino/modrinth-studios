/**
 * Modrinth Studios addition: Pterodactyl activity log → Archon action log.
 */
import type { Archon } from '@modrinth/api-client'

import { pteroRequest } from './bridge'
import { PTERO_WORLD_ID, type PteroServerRef } from './ids'

type PteroActivity = {
	id: string
	event: string
	is_api: boolean
	ip: string | null
	description: string | null
	properties: Record<string, unknown>
	timestamp: string
	relationships?: {
		actor?: { object: string; attributes: { uuid: string; username: string; image?: string } } | null
	}
}

type Mapped = { action: string; metadata?: unknown }

function joinPath(dir: unknown, file: unknown) {
	const d = typeof dir === 'string' ? dir.replace(/\/+$/, '') : ''
	const f = String(file ?? '').replace(/^\/+/, '')
	return `${d}/${f}`.replace(/^\/*/, '/')
}

function portOf(value: unknown): number | null {
	const match = String(value ?? '').match(/:(\d+)$/)
	return match ? Number(match[1]) : null
}

// Readable names for every Pterodactyl activity event that has no Archon equivalent.
const EVENT_LABELS: Record<string, string> = {
	'backup.download': 'Downloaded backup',
	'backup.fail': 'Backup failed',
	'backup.lock': 'Locked backup',
	'backup.unlock': 'Unlocked backup',
	'backup.restore-complete': 'Backup restore finished',
	'database.create': 'Created database',
	'database.rotate-password': 'Rotated database password',
	'database.delete': 'Deleted database',
	'file.compress': 'Compressed files',
	'file.decompress': 'Extracted archive',
	'file.download': 'Downloaded file',
	'file.read': 'Opened file',
	'file.create-directory': 'Created folder',
	'file.upload': 'Started file upload',
	'file.chmod': 'Changed file permissions',
	'allocation.notes': 'Renamed port',
	'allocation.primary': 'Changed primary port',
	'schedule.create': 'Created schedule',
	'schedule.update': 'Updated schedule',
	'schedule.execute': 'Ran schedule',
	'schedule.delete': 'Deleted schedule',
	'task.create': 'Added schedule task',
	'task.update': 'Updated schedule task',
	'task.delete': 'Removed schedule task',
	'settings.description': 'Changed server description',
	'settings.reinstall': 'Reinstalled server',
	'startup.edit': 'Changed startup variable',
	'startup.image': 'Changed Docker image',
	'subuser.create': 'Added user',
	'subuser.update': 'Updated user permissions',
	'subuser.delete': 'Removed user',
	'sftp.write': 'Edited files over SFTP',
	'sftp.create': 'Created files over SFTP',
	'sftp.create-directory': 'Created folder over SFTP',
	'sftp.rename': 'Renamed files over SFTP',
	'sftp.delete': 'Deleted files over SFTP',
	'console.command': 'Ran console command',
	'power.start': 'Started server',
	'power.stop': 'Stopped server',
	'power.restart': 'Restarted server',
	'power.kill': 'Killed server',
}

/** Events hidden from the log unless explicitly asked for (very noisy). */
export const HIDDEN_BY_DEFAULT = new Set(['server:file.read'])

function describe(p: Record<string, unknown>): string | null {
	const files = Array.isArray(p.files)
		? (p.files as unknown[]).map((f) =>
				typeof f === 'object' && f ? `${(f as { from?: string }).from} → ${(f as { to?: string }).to}` : String(f),
			)
		: null
	const parts = [
		typeof p.file === 'string' ? p.file : null,
		files?.length ? (files.length > 3 ? `${files.slice(0, 3).join(', ')} +${files.length - 3}` : files.join(', ')) : null,
		typeof p.name === 'string' ? p.name : null,
		typeof p.email === 'string' ? p.email : null,
		typeof p.variable === 'string' ? `${p.variable}: ${p.old ?? ''} → ${p.new ?? ''}` : null,
		typeof p.old === 'string' && typeof p.new === 'string' && typeof p.variable !== 'string'
			? `${p.old} → ${p.new}`
			: null,
		typeof p.allocation === 'string' ? p.allocation : null,
		typeof p.directory === 'string' && !files && typeof p.file !== 'string' ? p.directory : null,
	].filter((v): v is string => !!v)
	return parts.length ? parts.join(' · ') : null
}

function studioEvent(activity: PteroActivity): Mapped {
	const key = activity.event.replace(/^server:/, '')
	const label =
		EVENT_LABELS[key] ??
		key
			.split(/[.:-]/)
			.filter(Boolean)
			.map((w, i) => (i === 0 ? w[0].toUpperCase() + w.slice(1) : w))
			.join(' ')
	return {
		action: key === 'file.read' ? 'studio_file_read' : 'studio_event',
		metadata: {
			label,
			detail: describe(activity.properties ?? {}) ?? activity.description ?? null,
			raw: activity.event,
		},
	}
}

function mapEvent(activity: PteroActivity): Mapped[] {
	const p = activity.properties ?? {}
	const event = activity.event.replace(/^server:/, '')
	switch (event) {
		case 'power.start':
			return [{ action: 'server_started' }]
		case 'power.stop':
			return [{ action: 'server_stopped' }]
		case 'power.restart':
			return [{ action: 'server_restarted' }]
		case 'power.kill':
			return [{ action: 'server_killed' }]
		case 'console.command':
			return [{ action: 'console_command_executed', metadata: { command: String(p.command ?? '') } }]
		case 'file.write':
			return [{ action: 'file_edited', metadata: { path: joinPath('', p.file) } }]
		case 'file.uploaded':
			return [{ action: 'file_uploaded', metadata: { path: joinPath(p.directory, p.file) } }]
		case 'file.pull':
			return [
				{ action: 'file_uploaded', metadata: { path: joinPath(p.directory, p.filename ?? p.url) } },
			]
		case 'file.delete':
			return ((p.files as string[] | undefined) ?? []).map((file) => ({
				action: 'file_deleted',
				metadata: { path: joinPath(p.directory, file) },
			}))
		case 'file.rename':
			return ((p.files as { from: string; to: string }[] | undefined) ?? []).map((f) => ({
				action: 'file_renamed',
				metadata: { from: joinPath(p.directory, f.from), to: joinPath(p.directory, f.to) },
			}))
		case 'backup.start':
		case 'backup.complete':
			return [{ action: 'backup_created', metadata: { id: String(p.name ?? activity.id) } }]
		case 'backup.restore':
		case 'backup.restore-complete':
			return [{ action: 'backup_restored', metadata: { id: String(p.name ?? activity.id) } }]
		case 'backup.delete':
			return [{ action: 'backup_deleted', metadata: { id: String(p.name ?? activity.id) } }]
		case 'allocation.create':
			return [{ action: 'port_allocation_added', metadata: { port: portOf(p.allocation) } }]
		case 'allocation.delete':
			return [{ action: 'port_allocation_removed', metadata: { port: portOf(p.allocation) } }]
		case 'settings.rename':
			return [{ action: 'changed_server_name', metadata: { name: String(p.new ?? '') } }]
		case 'settings.reinstall':
			return [{ action: 'server_reset' }]
		case 'startup.edit':
			if (/VERSION/i.test(String(p.variable ?? ''))) {
				return [{ action: 'game_version_edited', metadata: { new_version: String(p.new ?? '') } }]
			}
			return [studioEvent(activity)]
		case 'startup.image':
			return [studioEvent(activity)]
		case 'sftp.write':
		case 'sftp.create':
			return ((p.files as string[] | undefined) ?? []).map((file) => ({
				action: 'file_edited',
				metadata: { path: joinPath('', file) },
			}))
		case 'sftp.delete':
			return ((p.files as string[] | undefined) ?? []).map((file) => ({
				action: 'file_deleted',
				metadata: { path: joinPath('', file) },
			}))
		default:
			return [studioEvent(activity)]
	}
}

export async function listActivity(
	ref: PteroServerRef,
	query: URLSearchParams,
	clearedAt?: string | null,
): Promise<Archon.Actions.v1.ActionLogResponse> {
	const limit = Math.min(100, Math.max(1, Number(query.get('limit') ?? 50)))
	const offset = Math.max(0, Number(query.get('offset') ?? 0))
	const page = Math.floor(offset / limit) + 1
	const sort = query.get('order') === 'asc' ? 'timestamp' : '-timestamp'

	const response = await pteroRequest<{
		data: { attributes: PteroActivity }[]
		meta?: { pagination?: { current_page: number; total_pages: number } }
	}>(
		ref.panelId,
		'GET',
		`/servers/${ref.identifier}/activity?per_page=${limit}&page=${page}&sort=${sort}&include=actor`,
	)

	let filter: Archon.Actions.v1.ActionLogFilter | null = null
	try {
		filter = query.get('filter') ? JSON.parse(query.get('filter')!) : null
	} catch {
		filter = null
	}

	const users: Record<string, Archon.Actions.v1.UserResp> = {}
	const data: Archon.Actions.v1.ActionEntry[] = []
	let reachedCutoff = false
	// "Opened files" is very noisy; only shown when picked in the action filter.
	const showHidden = (filter?.actions ?? []).includes('studio_file_read')
	for (const { attributes: activity } of response.data) {
		if (!showHidden && HIDDEN_BY_DEFAULT.has(activity.event)) continue
		if (clearedAt && Date.parse(activity.timestamp) < Date.parse(clearedAt)) {
			if (sort === '-timestamp') reachedCutoff = true
			continue
		}
		const actor = activity.relationships?.actor?.attributes
		const userId = actor?.uuid ?? 'system'
		users[userId] = {
			username: actor?.username ?? (activity.is_api ? 'API' : 'System'),
			avatar_url: actor?.image ?? null,
		}
		for (const mapped of mapEvent(activity)) {
			data.push({
				actor: { type: 'user', user_id: userId },
				action: { action: mapped.action, metadata: mapped.metadata },
				server_id: ref.serverId,
				world_id: PTERO_WORLD_ID,
				timestamp: activity.timestamp,
			})
		}
	}

	const filtered = data.filter((entry) => {
		if (filter?.users?.length && !filter.users.includes((entry.actor as { user_id: string }).user_id))
			return false
		if (filter?.actions?.length && !filter.actions.includes(entry.action.action as never)) return false
		return true
	})

	const pagination = response.meta?.pagination
	const hasMore =
		!reachedCutoff && (pagination ? pagination.current_page < pagination.total_pages : false)
	return {
		next_offset: hasMore ? offset + limit : null,
		data: filtered,
		users,
		addons: {},
		versions: {},
	}
}
