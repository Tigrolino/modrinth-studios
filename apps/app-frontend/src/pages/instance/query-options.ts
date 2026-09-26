import { queryOptions } from '@tanstack/vue-query'

import { get_project_v3 } from '@/helpers/cache.js'
import {
	get as getInstance,
	list as listInstances,
	list_instance_screenshots,
	list_screenshot_groups,
	list_synced_screenshots,
} from '@/helpers/instance'
import { loadInstanceContentData } from '@/helpers/instance-content'
import { getPlaytimeCorrection } from '@/helpers/playtime-correction'
import { get_by_instance_id } from '@/helpers/process'
import { hasReplays, listReplays } from '@/helpers/replays'
import { refreshWorlds } from '@/helpers/worlds'

export const instanceKeys = {
	all: ['instances'] as const,
	list: () => [...instanceKeys.all, 'list'] as const,
	detail: (instanceId: string) => [...instanceKeys.all, 'summary', instanceId] as const,
	processes: (instanceId: string) => [...instanceKeys.all, 'processes', instanceId] as const,
	content: (instanceId: string) => [...instanceKeys.all, 'content', instanceId] as const,
	contentSync: (instanceId: string) => [...instanceKeys.all, 'content-sync', instanceId] as const,
	contentUpdateCheck: (instanceId: string) =>
		[...instanceKeys.all, 'content-update-check', instanceId] as const,
	rootPath: (instanceId: string) => [...instanceKeys.detail(instanceId), 'root-path'] as const,
	files: (instanceId: string, path: string) =>
		[...instanceKeys.detail(instanceId), 'files', path] as const,
	console: (instanceId: string) => [...instanceKeys.detail(instanceId), 'console'] as const,
	logs: (instanceId: string) => [...instanceKeys.detail(instanceId), 'logs'] as const,
	installedProjectIds: (instanceId: string, source: 'content' | 'worlds') =>
		[...instanceKeys.detail(instanceId), 'installed-project-ids', source] as const,
	linkedContent: (instanceId: string) => ['linkedModpackContent', instanceId] as const,
	worlds: (instanceId: string) => ['worlds', instanceId] as const,
	// Modrinth Studios addition
	replays: (instanceId: string) => ['replays', instanceId] as const,
	hasReplays: (instanceId: string) => ['replays-exist', instanceId] as const,
	playtimeCorrection: (instanceId: string) => ['playtime-correction', instanceId] as const,
	linkedProject: (projectId: string) => ['project', 'v3', projectId] as const,
	sharedEligibility: (userId: string | null | undefined) =>
		['shared-instance-eligibility', userId] as const,
	sharedUpdatePreview: (instanceId: string, userId: string | null | undefined) =>
		[...instanceKeys.detail(instanceId), 'shared-update-preview', userId] as const,
	sharedMembers: (instanceId: string) => ['sharedInstanceUsers', instanceId] as const,
}

export const screenshotKeys = {
	all: ['screenshots'] as const,
	global: () => [...screenshotKeys.all, 'global'] as const,
	instance: (instanceId: string) => [...screenshotKeys.all, 'instance', instanceId] as const,
	groups: () => [...screenshotKeys.all, 'groups'] as const,
}

export function instanceListQueryOptions() {
	return queryOptions({
		queryKey: instanceKeys.list(),
		queryFn: listInstances,
		staleTime: 30_000,
	})
}

export function syncedScreenshotsQueryOptions() {
	return queryOptions({
		queryKey: screenshotKeys.global(),
		queryFn: list_synced_screenshots,
		staleTime: 0,
	})
}

export function instanceScreenshotsQueryOptions(instanceId: string) {
	return queryOptions({
		queryKey: screenshotKeys.instance(instanceId),
		queryFn: () => list_instance_screenshots(instanceId),
		staleTime: 0,
	})
}

export function screenshotGroupsQueryOptions() {
	return queryOptions({
		queryKey: screenshotKeys.groups(),
		queryFn: list_screenshot_groups,
		staleTime: 0,
	})
}

export function instanceDetailQueryOptions(instanceId: string) {
	return queryOptions({
		queryKey: instanceKeys.detail(instanceId),
		networkMode: 'always',
		queryFn: async () => {
			const instance = await getInstance(instanceId)
			if (!instance) throw new Error(`Instance ${instanceId} is not managed`)
			return instance
		},
		staleTime: 30_000,
	})
}

export function instanceProcessesQueryOptions(instanceId: string) {
	return queryOptions({
		queryKey: instanceKeys.processes(instanceId),
		queryFn: async () => {
			const processes = await get_by_instance_id(instanceId)
			return Array.isArray(processes) ? processes : []
		},
		staleTime: 0,
	})
}

export function instanceLinkedProjectQueryOptions(projectId: string) {
	return queryOptions({
		queryKey: instanceKeys.linkedProject(projectId),
		queryFn: () => get_project_v3(projectId, 'must_revalidate'),
		staleTime: 30_000,
	})
}

export function instanceContentQueryOptions(
	instanceId: string,
	onError?: (error: Error) => unknown,
) {
	return queryOptions({
		queryKey: instanceKeys.content(instanceId),
		networkMode: 'always',
		queryFn: () => loadInstanceContentData(instanceId, undefined, onError),
		staleTime: 30_000,
	})
}

export function instanceWorldsQueryOptions(instanceId: string) {
	return queryOptions({
		queryKey: instanceKeys.worlds(instanceId),
		queryFn: () => refreshWorlds(instanceId),
		staleTime: 0,
	})
}

// Modrinth Studios addition
export function instanceHasReplaysQueryOptions(instanceId: string) {
	return queryOptions({
		queryKey: instanceKeys.hasReplays(instanceId),
		queryFn: () => hasReplays(instanceId),
		staleTime: 30_000,
	})
}

export function instanceReplaysQueryOptions(instanceId: string) {
	return queryOptions({
		queryKey: instanceKeys.replays(instanceId),
		queryFn: () => listReplays(instanceId),
		staleTime: 0,
	})
}

// Modrinth Studios addition. A shared vue-query cache entry (rather than each
// component fetching independently) so saving a correction in
// PlaytimeCorrectionModal.vue can push the new value straight into every
// place that shows it (the settings page and the instance header's playtime
// badge) via `setQueryData`, instead of each one only refreshing on its own
// next mount.
export function instancePlaytimeCorrectionQueryOptions(instanceId: string) {
	return queryOptions({
		queryKey: instanceKeys.playtimeCorrection(instanceId),
		queryFn: () => getPlaytimeCorrection(instanceId),
		staleTime: 30_000,
	})
}
