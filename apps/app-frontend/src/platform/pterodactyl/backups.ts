/**
 * Modrinth Studios addition: Archon backup queue emulation on top of
 * Pterodactyl backups.
 */
import type { Archon } from '@modrinth/api-client'

import { pteroRequest } from './bridge'
import type { PteroServerRef } from './ids'
import type { PteroServerInfo } from './server'

type PteroBackup = {
	uuid: string
	is_successful: boolean
	is_locked: boolean
	name: string
	ignored_files: string[]
	checksum: string | null
	bytes: number
	created_at: string
	completed_at: string | null
}

/** Restores we started, so the UI can show them as active until Wings reports back. */
const pendingRestores = new Map<string, { backupId: string; startedAt: string }>()

export function markRestoreFinished(serverId: string) {
	pendingRestores.delete(serverId)
}

async function listPteroBackups(ref: PteroServerRef): Promise<PteroBackup[]> {
	const response = await pteroRequest<{ data: { attributes: PteroBackup }[] }>(
		ref.panelId,
		'GET',
		`/servers/${ref.identifier}/backups?per_page=100`,
	)
	return response.data.map((d) => d.attributes)
}

function toStatus(backup: PteroBackup): Archon.BackupsQueue.v1.BackupStatus {
	if (!backup.completed_at) return 'in_progress'
	return backup.is_successful ? 'done' : 'error'
}

export async function listBackupsQueue(
	info: PteroServerInfo,
	withOptions: boolean,
): Promise<
	Archon.BackupsQueue.v1.BackupsQueueResponse | Archon.BackupsQueue.v1.BackupsQueueWithOptionsResponse
> {
	const backups = await listPteroBackups(info.ref)
	const active: Archon.BackupsQueue.v1.ActiveOperation[] = backups
		.filter((b) => !b.completed_at)
		.map((b) => ({
			backup_id: b.uuid,
			operation_type: 'create',
			operation_id: null,
			has_parent: false,
			scheduled_for: b.created_at,
			started_at: b.created_at,
			synthetic_legacy: true,
			user_info: null,
		}))
	const restore = pendingRestores.get(info.ref.serverId)
	if (restore) {
		active.push({
			backup_id: restore.backupId,
			operation_type: 'restore',
			operation_id: null,
			has_parent: false,
			scheduled_for: restore.startedAt,
			started_at: restore.startedAt,
			synthetic_legacy: true,
			user_info: null,
		})
	}

	const schedule = withOptions ? await findAutoBackupSchedule(info.ref).catch(() => null) : null
	const manualIds = new Set(info.settings.manual_backup_ids ?? [])
	const names = info.settings.backup_names ?? {}
	// Backups Studio didn't create by hand are treated as scheduled while an
	// automatic-backup schedule exists.
	const isAuto = (b: PteroBackup) => !!schedule && !manualIds.has(b.uuid)

	const mapped: Archon.BackupsQueue.v1.BackupQueueBackup[] = backups
		.sort((a, b) => b.created_at.localeCompare(a.created_at))
		.map((b) => ({
			id: b.uuid,
			name: names[b.uuid] ?? b.name,
			created_at: b.created_at,
			status: toStatus(b),
			locked: b.is_locked,
			automated: isAuto(b),
			history: [
				{
					operation_type: 'create',
					operation_id: null,
					state: !b.completed_at ? 'ongoing' : b.is_successful ? 'completed' : 'failed',
					scheduled_for: b.created_at,
					started_at: b.created_at,
					completed_at: b.completed_at,
					has_parent: false,
					error: b.completed_at && !b.is_successful ? 'Backup failed on the node' : null,
					should_prompt: false,
					synthetic_legacy: true,
					user_info: null,
				},
			],
		}))

	const base: Archon.BackupsQueue.v1.BackupsQueueResponse = {
		active_operations: active,
		backups: mapped,
	}
	if (!withOptions) return base

	const limit = info.attributes.feature_limits.backups
	const split_automatic = schedule
		? Math.min(limit, Math.max(1, info.settings.backup_split_automatic ?? Math.floor(limit / 2)))
		: 0
	const autoCount = backups.filter(isAuto).length
	return {
		...base,
		options: {
			split_automatic,
			split_manual: Math.max(0, limit - split_automatic),
			limit,
			interval: info.settings.backup_interval ?? 24,
		},
		next_auto_backup_at: schedule?.next_run_at ?? null,
		manual_count: backups.length - autoCount,
		auto_count: autoCount,
	}
}

// --- automatic backups (Pterodactyl schedules) ----------------------------------

const AUTO_SCHEDULE_NAME = 'Modrinth Studio automatic backups'

type PteroSchedule = {
	id: number
	name: string
	cron: { minute: string; hour: string; day_of_month: string; month: string; day_of_week: string }
	is_active: boolean
	next_run_at: string | null
}

async function findAutoBackupSchedule(ref: PteroServerRef): Promise<PteroSchedule | null> {
	const response = await pteroRequest<{ data: { attributes: PteroSchedule }[] }>(
		ref.panelId,
		'GET',
		`/servers/${ref.identifier}/schedules`,
	)
	return response.data.map((d) => d.attributes).find((s) => s.name === AUTO_SCHEDULE_NAME) ?? null
}

/** Cron fields for "every `hours` hours". */
function cronFor(hours: number) {
	const h = Math.max(1, Math.round(hours))
	if (h < 24) return { minute: '0', hour: h === 1 ? '*' : `*/${h}`, day_of_month: '*' }
	const days = Math.max(1, Math.round(h / 24))
	return { minute: '0', hour: '3', day_of_month: days === 1 ? '*' : `*/${days}` }
}

export async function patchBackupOptions(
	info: PteroServerInfo,
	patch: Archon.BackupsQueue.v1.PatchWorldBackupOptions,
	saveSettings: (s: { split_automatic?: number; interval?: number }) => Promise<void>,
): Promise<Archon.BackupsQueue.v1.PatchWorldBackupOptionsResponse> {
	const { ref } = info
	const existing = await findAutoBackupSchedule(ref)
	const enable = (patch.split_automatic ?? (existing ? 1 : 0)) > 0
	const interval = patch.interval ?? info.settings.backup_interval ?? 24

	if (!enable) {
		if (existing) {
			await pteroRequest(ref.panelId, 'DELETE', `/servers/${ref.identifier}/schedules/${existing.id}`)
		}
		await saveSettings({ split_automatic: 0, interval })
		return { applied: true }
	}

	const body = {
		name: AUTO_SCHEDULE_NAME,
		...cronFor(interval),
		month: '*',
		day_of_week: '*',
		is_active: true,
		only_when_online: false,
	}
	if (existing) {
		await pteroRequest(ref.panelId, 'POST', `/servers/${ref.identifier}/schedules/${existing.id}`, {
			body,
		})
	} else {
		const created = await pteroRequest<{ attributes: PteroSchedule }>(
			ref.panelId,
			'POST',
			`/servers/${ref.identifier}/schedules`,
			{ body },
		)
		await pteroRequest(
			ref.panelId,
			'POST',
			`/servers/${ref.identifier}/schedules/${created.attributes.id}/tasks`,
			{ body: { action: 'backup', payload: '', time_offset: 0, continue_on_failure: false } },
		)
	}
	await saveSettings({ split_automatic: patch.split_automatic, interval })
	return { applied: true }
}

export async function createBackup(
	ref: PteroServerRef,
	name: string | undefined,
): Promise<Archon.BackupsQueue.v1.PostBackupQueueResponse> {
	const response = await pteroRequest<{ attributes: PteroBackup }>(
		ref.panelId,
		'POST',
		`/servers/${ref.identifier}/backups`,
		{ body: { name: name || undefined } },
	)
	return { id: response.attributes.uuid }
}

export async function deleteBackup(ref: PteroServerRef, backupId: string) {
	await pteroRequest(ref.panelId, 'DELETE', `/servers/${ref.identifier}/backups/${backupId}`)
}

export async function restoreBackup(ref: PteroServerRef, backupId: string) {
	await pteroRequest(
		ref.panelId,
		'POST',
		`/servers/${ref.identifier}/backups/${backupId}/restore`,
		{ body: { truncate: false } },
	)
	pendingRestores.set(ref.serverId, { backupId, startedAt: new Date().toISOString() })
}

export async function setBackupLocked(ref: PteroServerRef, backupId: string, locked: boolean) {
	const backups = await listPteroBackups(ref)
	const backup = backups.find((b) => b.uuid === backupId)
	if (backup && backup.is_locked !== locked) {
		await pteroRequest(ref.panelId, 'POST', `/servers/${ref.identifier}/backups/${backupId}/lock`)
	}
}

export async function backupDownloadUrl(ref: PteroServerRef, backupId: string): Promise<string> {
	const response = await pteroRequest<{ attributes: { url: string } }>(
		ref.panelId,
		'GET',
		`/servers/${ref.identifier}/backups/${backupId}/download`,
	)
	return response.attributes.url
}
