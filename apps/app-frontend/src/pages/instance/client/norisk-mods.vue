<script setup lang="ts">
/**
 * Modrinth Studios: the "NoRisk mods" tab. Modrinth's own content page
 * (ContentPageLayout) with NoRisk's pack shown like a modpack: a managed
 * card on top and its mods underneath, read-only.
 */
import { DownloadIcon } from '@modrinth/assets'
import { Button, type ContentItem, ContentPageLayout, provideContentManager } from '@modrinth/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { computed, ref } from 'vue'
import { injectInstancePage } from '../instance-context'

import { get_project_many } from '@/helpers/cache.js'
import { clientLogo, noriskPackMods } from '@/helpers/studio-clients'
import { showInstanceInFolder } from '@/helpers/utils.js'

const props = defineProps<{
	instanceId: string
	packName: string
	running: boolean
	canUpdate: boolean
	onUpdate: () => void
}>()
const queryClient = useQueryClient()
const { browseContent } = injectInstancePage()
const logo = clientLogo('norisk')

type ModrinthProject = {
	id: string
	title: string
	icon_url: string | null
	slug: string
	team?: string
}

const modsQuery = useQuery({
	queryKey: computed(() => ['norisk-pack-mods', props.instanceId]),
	queryFn: async () => {
		const mods = await noriskPackMods(props.instanceId)
		const ids = mods.map((m) => m.modrinth_id).filter((id): id is string => !!id)
		const projects: ModrinthProject[] = ids.length
			? await get_project_many(ids, 'must_revalidate').catch(() => [])
			: []
		const byId = new Map((projects ?? []).map((p) => [p.id, p]))
		return mods.map((m) => ({
			mod: m,
			project: m.modrinth_id ? byId.get(m.modrinth_id) : undefined,
		}))
	},
	staleTime: 5 * 60_000,
})

const items = computed<ContentItem[]>(() =>
	(modsQuery.data.value ?? []).map(
		({ mod, project }) =>
			({
				id: mod.id,
				file_name: mod.id,
				project_type: 'mod',
				has_update: false,
				update_version_id: null,
				enabled: true,
				locked: true,
				hideToggle: true,
				hideDelete: true,
				hideSwitchVersion: true,
				project: {
					id: project?.id ?? mod.id,
					slug: project?.slug ?? mod.id,
					title: project?.title ?? mod.name,
					// NoRisk's own mods get NoRisk's logo.
					icon_url: project?.icon_url ?? logo,
				},
				owner: project ? undefined : { id: 'norisk', name: 'NoRisk', type: 'organization' },
			}) as ContentItem,
	),
)

provideContentManager({
	items,
	loading: modsQuery.isPending,
	error: modsQuery.error,
	managedContent: computed(() => ({
		disabled: props.running,
		card: {
			kind: 'modpack' as const,
			canSwitchVersion: false,
			manager: { name: 'NoRisk Client', iconUrl: logo, link: 'https://norisk.gg' },
			versionNumber: props.packName,
			summary: [{ type: 'mod' as const, count: items.value.length }],
			installing: props.running,
		},
	})),
	isPackLocked: computed(() => true),
	isBusy: computed(() => props.running),
	contentTypeLabel: ref('mod'),
	itemsHeading: ref('Included mods'),
	uploadActionLabel: 'Open folder',
	emptyHeading: 'No managed mods found',
	emptyDescription: 'Use Update client files to download the mods included with this NoRisk pack.',
	deleteItem: async () => {},
	canDeleteItem: () => false,
	canToggleItem: () => false,
	refresh: async () => {
		await queryClient.invalidateQueries({ queryKey: ['norisk-pack-mods', props.instanceId] })
	},
	// "Browse content" adds your own mods in the normal Content tab.
	browse: () => {
		void browseContent('mod')
	},
	uploadFiles: () => showInstanceInFolder(props.instanceId),
	hasUpdateSupport: false,
	mapToTableItem: (item: ContentItem) => ({
		...item,
		id: item.id,
		projectLink:
			item.project?.slug && item.owner?.id !== 'norisk'
				? `/project/${item.project.slug}`
				: undefined,
	}),
})
</script>

<template>
	<div class="flex flex-col gap-4">
		<div class="flex justify-end">
			<Button :disabled="running || !canUpdate" @click="onUpdate"
				><DownloadIcon />Update client files</Button
			>
		</div>
		<ContentPageLayout :bottom-padding="false" />
	</div>
</template>
