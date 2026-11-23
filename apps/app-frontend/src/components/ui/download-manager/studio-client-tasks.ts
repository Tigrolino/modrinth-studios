/**
 * Modrinth Studios: downloads of client installations (Lunar, LabyMod,
 * NoRisk, Horizon) shown in the download manager next to normal installs.
 * Progress comes from the backend's loading events for that instance.
 */
import { reactive, ref } from 'vue'

import { wait_for_install_job } from '@/helpers/install'
import { prepareClient } from '@/helpers/studio-clients'
import type { AppEvents } from '@/providers/app-events'

export interface StudioClientTask {
	id: string
	instanceId: string
	title: string
	iconUrl: string | null
	status: 'running' | 'succeeded' | 'failed'
	progress: number
	message: string
	createdAt: string
	finishedAt?: string
	error?: string
}

export const studioClientTasks = ref(new Map<string, StudioClientTask>())

export function findRunningClientTask(instanceId: string) {
	for (const task of studioClientTasks.value.values()) {
		if (task.instanceId === instanceId && task.status === 'running') return task
	}
	return null
}

/** Downloads the client's files for an instance, tracked in the download manager. */
const preparations = new Map<string, Promise<void>>()

export function runClientPrepare(
	events: AppEvents,
	instanceId: string,
	title: string,
	iconUrl: string | null,
	installJobId?: string,
): Promise<void> {
	const existing = preparations.get(instanceId)
	if (existing) return existing
	const promise = performClientPrepare(events, instanceId, title, iconUrl, installJobId).finally(
		() => preparations.delete(instanceId),
	)
	preparations.set(instanceId, promise)
	return promise
}

async function performClientPrepare(
	events: AppEvents,
	instanceId: string,
	title: string,
	iconUrl: string | null,
	installJobId?: string,
) {
	const task = reactive<StudioClientTask>({
		id: `studio-client-${crypto.randomUUID()}`,
		instanceId,
		title,
		iconUrl,
		status: 'running',
		progress: 0,
		message: installJobId ? 'Waiting for Minecraft to finish installing…' : 'Getting ready…',
		createdAt: new Date().toISOString(),
	})
	studioClientTasks.value.set(task.id, task)

	const unlisten = events.on('loading', (payload) => {
		const event = payload.event as { type?: string; instance_id?: string }
		if (event.type !== 'minecraft_download' || event.instance_id !== instanceId) return
		if (payload.fraction != null) task.progress = Math.min(0.99, payload.fraction)
		if (payload.message) task.message = payload.message
	})

	try {
		if (installJobId) await wait_for_install_job(events, installJobId)
		await prepareClient(instanceId)
		task.status = 'succeeded'
		task.progress = 1
		task.message = 'Ready to play'
	} catch (e) {
		task.status = 'failed'
		task.error = (e as { message?: string })?.message ?? String(e)
		task.message = task.error
		throw e
	} finally {
		task.finishedAt = new Date().toISOString()
		unlisten()
	}
}
