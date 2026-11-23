<script setup lang="ts">
/**
 * Modrinth Studios: Horizon's Content tab. This is Modrinth's own content page
 * (ContentPageLayout), fed with what's installed from Horizon's registry.
 */
import {
	type ContentItem,
	ContentPageLayout,
	injectNotificationManager,
	provideContentManager,
} from '@modrinth/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'

import { horizonStoreList, horizonStoreRemove } from '@/helpers/studio-clients'
import { showInstanceInFolder } from '@/helpers/utils.js'

const props = defineProps<{ instanceId: string }>()
const { handleError } = injectNotificationManager()
const queryClient = useQueryClient()
const router = useRouter()

const storeQuery = useQuery({
	queryKey: computed(() => ['horizon-store', props.instanceId]),
	queryFn: () => horizonStoreList(props.instanceId),
	staleTime: 60_000,
})

const removing = ref<string | null>(null)

const items = computed<ContentItem[]>(() =>
	(storeQuery.data.value ?? [])
		.filter((p) => p.installed)
		.map(
			(p) =>
				({
					id: p.id,
					file_name: p.id,
					project_type: p.kind,
					projectType: p.kind,
					has_update: false,
					update_version_id: null,
					enabled: true,
					hideToggle: true,
					hideSwitchVersion: true,
					installing: removing.value === p.id,
					project: { id: p.id, slug: p.id, title: p.name, icon_url: p.icon },
					version: { id: p.version, version_number: p.version, file_name: p.id },
					owner: p.authors.length
						? { id: p.authors[0], name: p.authors.join(', '), type: 'user' }
						: undefined,
					external_url: p.source ?? undefined,
				}) as ContentItem,
		),
)

async function refresh() {
	await queryClient.invalidateQueries({ queryKey: ['horizon-store', props.instanceId] })
}

provideContentManager({
	items,
	loading: storeQuery.isPending,
	error: storeQuery.error,
	managedContent: computed(() => null),
	isPackLocked: computed(() => false),
	isBusy: computed(() => removing.value !== null),
	contentTypeLabel: ref('project'),
	uploadActionLabel: 'Open folder',
	emptyDescription: 'Browse Horizon content to add mods, resource packs and shaders.',
	currentGameVersion: computed(() => '26.3'),
	currentLoader: computed(() => 'horizon'),
	deleteItem: async (item: ContentItem) => {
		removing.value = item.id
		try {
			await horizonStoreRemove(props.instanceId, item.id)
			await refresh()
		} catch (e) {
			handleError(e as Error)
		} finally {
			removing.value = null
		}
	},
	canToggleItem: () => false,
	refresh,
	browse: () => router.push(`/horizon-browse/${encodeURIComponent(props.instanceId)}/mod`),
	uploadFiles: () => showInstanceInFolder(props.instanceId),
	hasUpdateSupport: false,
	mapToTableItem: (item: ContentItem) => ({
		...item,
		id: item.id,
		projectLink: item.external_url,
	}),
})
</script>

<template>
	<ContentPageLayout :bottom-padding="false" />
</template>
