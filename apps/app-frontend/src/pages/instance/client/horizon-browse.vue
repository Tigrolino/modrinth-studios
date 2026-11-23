<script setup lang="ts">
/**
 * Modrinth Studios: "Browse content" for Horizon instances. Horizon can't run
 * Modrinth's Java mods, so this is Modrinth's own browse page (same layout,
 * cards, install header, sort options and category icons) fed with Horizon's
 * registry instead of Modrinth's search. Everything that differs from
 * Modrinth's page does so only because Horizon's registry has no data for it.
 */
import type { Labrinth } from '@modrinth/api-client'
import { CheckIcon, getCategoryIcon, PlusIcon, SpinnerIcon } from '@modrinth/assets'
import {
	Admonition,
	type BrowseInstallContext,
	BrowsePageLayout,
	BrowseSidebar,
	type CardAction,
	commonMessages,
	formatCategory,
	injectNotificationManager,
	provideBrowseManager,
	useVIntl,
} from '@modrinth/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { computed, ref, shallowRef, watch } from 'vue'
import { useRoute } from 'vue-router'

import { get as getInstance, getInstanceIconUrl } from '@/helpers/instance'
import {
	type HorizonProject,
	horizonStoreInstall,
	horizonStoreList,
} from '@/helpers/studio-clients'

const route = useRoute()
const { handleError } = injectNotificationManager()
const { formatMessage } = useVIntl()
const queryClient = useQueryClient()

const instanceId = computed(() => String(route.params.id ?? ''))
const typeParam = computed(() => String(route.params.type || 'mod'))
const KIND: Record<string, HorizonProject['kind']> = {
	mod: 'mod',
	resourcepack: 'resourcepack',
	shader: 'shader',
}
const projectType = ref(typeParam.value)
watch(typeParam, (t) => {
	projectType.value = t
	currentPage.value = 1
	currentFilters.value = []
})

const instanceQuery = useQuery({
	queryKey: computed(() => ['instance', instanceId.value]),
	queryFn: () => getInstance(instanceId.value),
	networkMode: 'always',
})
const storeQuery = useQuery({
	queryKey: computed(() => ['horizon-store', instanceId.value]),
	queryFn: () => horizonStoreList(instanceId.value),
	staleTime: 60_000,
})

// ---- search state, shaped like Modrinth's ----
const query = ref('')
// Modrinth's sort options and default, minus "Followers" (Horizon has none).
const sortTypes = [
	{ name: 'relevance', display: 'Relevance' },
	{ name: 'downloads', display: 'Downloads' },
	{ name: 'newest', display: 'Date published' },
	{ name: 'updated', display: 'Date updated' },
] as const
const currentSort = shallowRef<{ name: string; display: string }>(sortTypes[0])
const maxResults = ref(20)
const currentPage = ref(1)
watch([query, maxResults, currentSort], () => (currentPage.value = 1))

const busy = ref<string | null>(null)
const currentFilters = ref<{ type: string; option: string; negative?: boolean }[]>([])
const hideInstalled = ref(false)
watch([currentFilters, hideInstalled], () => (currentPage.value = 1), { deep: true })

/** Horizon's categories as Modrinth category ids, so they get Modrinth's icons and names. */
function categorySlug(c: string) {
	return c.trim().toLowerCase().replace(/[\s_]+/g, '-')
}
function categoriesOf(p: HorizonProject) {
	return [...new Set((p.categories ?? []).map(categorySlug))]
}

function toHit(p: HorizonProject) {
	return {
		project_id: p.id,
		slug: p.id,
		name: p.name,
		summary: p.summary,
		icon_url: p.icon,
		author: p.authors.join(', ') || 'Unknown',
		author_id: null,
		organization: null,
		organization_id: null,
		downloads: p.downloads,
		// No follows on Horizon: leaving it out hides the heart instead of "0".
		follows: undefined,
		date_created: p.published ?? '',
		date_modified: p.published ?? '',
		categories: categoriesOf(p),
		display_categories: categoriesOf(p),
		loaders: [],
		project_loader_fields: {},
		featured_gallery: null,
		color: null,
		installed: p.installed,
	} as unknown as Labrinth.Search.v3.ResultSearchProject & { installed?: boolean }
}

const filtered = computed(() => {
	const kind = KIND[projectType.value] ?? 'mod'
	const q = query.value.trim().toLowerCase()
	const included = currentFilters.value.filter((f) => f.type === 'category' && !f.negative)
	const excluded = currentFilters.value.filter((f) => f.type === 'category' && f.negative)
	const list = (storeQuery.data.value ?? []).filter((p) => {
		const cats = categoriesOf(p)
		return (
			p.kind === kind &&
			(!hideInstalled.value || !p.installed) &&
			(!included.length || included.some((f) => cats.includes(f.option))) &&
			excluded.every((f) => !cats.includes(f.option)) &&
			(!q || p.name.toLowerCase().includes(q) || p.summary.toLowerCase().includes(q))
		)
	})
	const byDownloads = (a: HorizonProject, b: HorizonProject) => b.downloads - a.downloads
	const byDate = (a: HorizonProject, b: HorizonProject) =>
		(b.published ?? '').localeCompare(a.published ?? '')
	switch (currentSort.value.name) {
		case 'newest':
		case 'updated':
			return [...list].sort(byDate)
		case 'downloads':
			return [...list].sort(byDownloads)
		default: {
			// Relevance: name matches before summary matches, then downloads.
			const rank = (p: HorizonProject) => {
				if (!q) return 0
				const name = p.name.toLowerCase()
				return name.startsWith(q) ? 0 : name.includes(q) ? 1 : 2
			}
			return [...list].sort((a, b) => rank(a) - rank(b) || byDownloads(a, b))
		}
	}
})
const totalHits = computed(() => filtered.value.length)
const pageCount = computed(() => Math.max(1, Math.ceil(totalHits.value / maxResults.value)))
const projectHits = shallowRef<ReturnType<typeof toHit>[]>([])
watch(
	[filtered, currentPage, maxResults, busy],
	() => {
		const start = (currentPage.value - 1) * maxResults.value
		projectHits.value = filtered.value.slice(start, start + maxResults.value).map(toHit)
	},
	{ immediate: true },
)

const byId = computed(() => new Map((storeQuery.data.value ?? []).map((p) => [p.id, p])))

async function install(id: string) {
	if (busy.value) return
	const targetInstanceId = instanceId.value
	busy.value = id
	try {
		await horizonStoreInstall(targetInstanceId, id)
		await queryClient.invalidateQueries({ queryKey: ['horizon-store', targetInstanceId] })
	} catch (e) {
		handleError(e as Error)
	} finally {
		busy.value = null
	}
}

// Exactly Modrinth's card button: one outlined brand button that turns into
// a disabled "Installed". Removing is done from the Content tab, like Modrinth.
function getCardActions(result: Labrinth.Search.v3.ResultSearchProject): CardAction[] {
	const project = byId.value.get(result.project_id)
	if (!project) return []
	const installing = busy.value === project.id
	return [
		{
			key: 'install',
			label: formatMessage(
				project.installed ? commonMessages.installedLabel : commonMessages.installButton,
			),
			icon: installing ? SpinnerIcon : project.installed ? CheckIcon : PlusIcon,
			iconClass: installing ? 'animate-spin' : undefined,
			disabled: project.installed || !!busy.value,
			color: 'brand',
			type: 'outlined',
			onClick: () => install(project.id),
		},
	]
}

const installContext = computed<BrowseInstallContext | null>(() => {
	const instance = instanceQuery.data.value
	if (!instance) return null
	return {
		name: instance.name,
		loader: 'horizon',
		gameVersion: instance.game_version,
		iconSrc: getInstanceIconUrl(instance.icon_path),
		backUrl: `/instance/${encodeURIComponent(instanceId.value)}/client/content`,
		backLabel: 'Back to instance',
		heading: 'Installing content',
	}
})

const base = computed(() => `/horizon-browse/${encodeURIComponent(instanceId.value)}`)

provideBrowseManager({
	tags: ref({ gameVersions: [], loaders: [], categories: [] }),
	projectType,
	query,
	filters: computed(() => [
		{
			id: 'category',
			formatted_name: 'Category',
			query_param: 'category',
			supported_project_types: ['mod', 'resourcepack', 'shader'],
			supports: ['include', 'exclude'],
			searchable: false,
			display: 'all',
			options: [
				...new Set(
					(storeQuery.data.value ?? [])
						.filter((p) => p.kind === projectType.value)
						.flatMap(categoriesOf),
				),
			]
				.sort()
				.map((id) => ({
					id,
					formatted_name: formatCategory(formatMessage, id),
					icon: getCategoryIcon(id),
					method: 'or' as const,
					value: id,
				})),
		},
	]),
	currentFilters,
	hideInstalled,
	showHideInstalled: computed(() => true),
	toggledGroups: ref([]),
	overriddenProvidedFilterTypes: ref([]),
	serverFilterTypes: computed(() => []),
	serverCurrentFilters: ref([]),
	serverToggledGroups: ref([]),
	effectiveSortTypes: computed(() => sortTypes),
	effectiveCurrentSortType: currentSort,
	loading: storeQuery.isPending,
	refreshing: storeQuery.isFetching,
	projectHits,
	serverHits: shallowRef([]),
	totalHits,
	pageCount,
	maxResults,
	currentPage,
	isServerType: computed(() => false),
	effectiveLayout: computed(() => 'list' as const),
	deprioritizedTags: computed(() => []),
	excludeLoaders: computed(() => true),
	refreshSearch: async () => {
		await queryClient.invalidateQueries({ queryKey: ['horizon-store', instanceId.value] })
	},
	setPage: async (page: number) => {
		currentPage.value = page
	},
	clearSearch: () => (query.value = ''),
	onFilterChange: () => {
		currentPage.value = 1
	},
	linkOverridesAdvancedPrefs: ref(false),
	applySavedAdvancedPrefs: () => {},
	getProjectLink: (result) => byId.value.get(result.project_id)?.source ?? '',
	// Horizon authors aren't Modrinth users, so there's no profile to link to.
	getAuthorLink: () => undefined,
	getServerProjectLink: () => '',
	selectableProjectTypes: computed(() => [
		{ label: 'Mods', href: `${base.value}/mod` },
		{ label: 'Resource Packs', href: `${base.value}/resourcepack` },
		{ label: 'Shaders', href: `${base.value}/shader` },
	]),
	showProjectTypeTabs: computed(() => true),
	variant: 'app',
	getCardActions,
	installContext,
})
</script>

<template>
	<div class="flex flex-col gap-2 p-6">
		<Admonition
			v-if="storeQuery.isError.value"
			type="critical"
			header="Could not load Horizon content"
			:body="String(storeQuery.error.value)"
		/>
		<BrowsePageLayout />
		<Teleport v-if="route.name === 'HorizonBrowse'" to="#sidebar-teleport-target"
			><BrowseSidebar
		/></Teleport>
	</div>
</template>
