<!--
	Modrinth Studios addition: shows ReplayMod / Flashback recordings found in
	this instance. New file, new route — doesn't modify any existing
	upstream instance tab. Row rendering lives in ReplayItem.vue, built to
	mirror WorldItem.vue's card so this tab looks consistent with Worlds.
-->
<template>
	<RenameReplayModal ref="renameModal" :instance-id="instance.id" @submit="onRenamed" />
	<ConfirmModal
		ref="bulkDeleteModal"
		:title="formatMessage(messages.bulkDeleteTitle)"
		:description="
			formatMessage(messages.bulkDeleteDescription, { count: selectedKeys.size })
		"
		:proceed-label="formatMessage(commonMessages.deleteLabel)"
		:markdown="false"
		@proceed="deleteSelected"
	/>
	<ReadyTransition :pending="replaysReadyPending">
		<div v-if="replays.length > 0" class="flex flex-col gap-4">
			<div class="flex flex-wrap items-center gap-2 justify-between">
				<h2 class="m-0 text-lg font-semibold text-contrast">
					{{ formatMessage(messages.heading, { count: replays.length }) }}
				</h2>
				<Button type="colored" color="brand" size="lg" :disabled="importing" @click="addReplay">
					<PlusIcon class="size-5" />
					{{ formatMessage(importing ? messages.adding : messages.addReplay) }}
				</Button>
			</div>
			<div class="flex flex-wrap items-center gap-2">
				<Input
					v-if="replays.length > 8"
					v-model="searchFilter"
					:icon="SearchIcon"
					type="text"
					autocomplete="off"
					:spellcheck="false"
					input-class="!h-10"
					clearable
					wrapper-class="min-w-[16rem] flex-1"
					:placeholder="formatMessage(messages.searchPlaceholder, { count: replays.length })"
				/>
				<!--
					Modrinth Studios addition: sort/group controls — mirrors the
					Screenshots tab's toolbar.vue (same Combobox usage, same
					sortOptions/groupOptions shape) so browsing hundreds of
					replays has the same tools browsing hundreds of screenshots
					already does.
				-->
				<Combobox
					v-model="sortModel"
					class="w-max"
					:options="sortOptions"
					:show-icon-in-selected="false"
					dropdown-min-width="170px"
				>
					<template #prefix>
						<ArrowUpDownIcon
							class="size-5 text-primary"
							:aria-label="formatMessage(messages.sortBy)"
						/>
					</template>
					<template #selected="{ label }">
						<span>{{ label }}</span>
					</template>
				</Combobox>
				<Combobox
					v-model="groupByModel"
					class="w-max"
					:options="groupOptions"
					:show-icon-in-selected="false"
					dropdown-min-width="170px"
				>
					<template #prefix>
						<LayoutGridIcon
							class="size-5 text-primary"
							:aria-label="formatMessage(messages.groupBy)"
						/>
					</template>
					<template #selected="{ label }">
						<span>{{ label }}</span>
					</template>
				</Combobox>
			</div>
			<!--
				Modrinth Studios addition: groups render in normal document
				flow (a plain v-for, no per-group absolute positioning) —
				exactly how the Screenshots tab lays out its own groups (see
				screenshots-page/index.vue's `virtualizedScreenshotGroups`
				and group.vue). Only the ROWS inside an open group are
				virtualized (`renderedReplays`/`virtualRowsTop` below,
				windowed the same way Screenshots windows its photo grid),
				since one group here can still hold hundreds of replays.
				Keeping groups themselves in normal flow is also what makes
				ReplayGroupSection's open/close animation actually work —
				see its own comment for why the earlier absolutely-positioned
				single-list approach couldn't animate at all.
			-->
			<div ref="listContainer" class="flex w-full flex-col">
				<ReplayGroupSection
					v-for="group in virtualizedReplayGroups"
					:key="group.id"
					:label="group.label"
					:replays="group.replays"
					:rendered-replays="group.renderedReplays"
					:rows-height="group.rowsHeight"
					:virtual-rows-top="group.virtualRowsTop"
					:collapsed="!!collapsedGroups[group.id]"
					:hide-header="isHeaderHidden"
					:instance-id="instance.id"
					:selected-keys="selectedKeys"
					:selection-active="selectedKeys.size > 0"
					@update:collapsed="toggleGroupCollapsed(group.id)"
					@toggle-selection="toggleSelection"
					@open-folder="openFolder"
					@rename="startRename"
					@delete="promptDelete"
				/>
			</div>
		</div>
		<EmptyState
			v-else
			type="empty-inbox"
			:heading="formatMessage(messages.noReplaysHeading)"
			:description="formatMessage(messages.noReplaysDescription)"
		>
			<template #actions>
				<Button type="colored" color="brand" size="lg" :disabled="importing" @click="addReplay">
					<PlusIcon class="size-5" />
					{{ formatMessage(importing ? messages.adding : messages.addReplay) }}
				</Button>
			</template>
		</EmptyState>
	</ReadyTransition>

	<!--
		Modrinth Studios addition: bulk-selection action bar — mirrors
		screenshots-page/index.vue's FloatingActionBar usage. Only a Delete
		action for now (rename/move don't make sense in bulk for replays).
	-->
	<FloatingActionBar
		:shown="selectedKeys.size > 0"
		:aria-label="formatMessage(messages.selectionAriaLabel)"
		hide-when-modal-open
	>
		<div class="flex items-center gap-0.5">
			<span class="px-4 py-2.5 text-base font-semibold tabular-nums text-contrast">
				{{ formatMessage(messages.selectedCount, { count: selectedKeys.size }) }}
			</span>
			<div class="mx-1 h-6 w-px bg-surface-5" />
			<Button
				v-tooltip="formatMessage(commonMessages.clearButton)"
				type="quiet"
				:aria-label="formatMessage(commonMessages.clearButton)"
				:disabled="bulkDeleting"
				@click="clearSelection"
			>
				<XIcon class="hidden cq-show-icon" />
				<span class="bar-label">{{ formatMessage(commonMessages.clearButton) }}</span>
			</Button>
		</div>
		<div class="ml-auto flex items-center gap-0.5">
			<Button
				v-tooltip="formatMessage(commonMessages.deleteLabel)"
				type="quiet"
				color="red"
				interaction="filled"
				:aria-label="formatMessage(commonMessages.deleteLabel)"
				:disabled="bulkDeleting"
				@click="bulkDeleteModal?.show()"
			>
				<TrashIcon />
				<span class="bar-label">{{ formatMessage(commonMessages.deleteLabel) }}</span>
			</Button>
		</div>
	</FloatingActionBar>
</template>
<script setup lang="ts">
import { ArrowUpDownIcon, LayoutGridIcon, PlusIcon, SearchIcon, TrashIcon, XIcon } from '@modrinth/assets'
import {
	Button,
	Combobox,
	type ComboboxOption,
	commonMessages,
	ConfirmModal,
	defineMessages,
	EmptyState,
	FloatingActionBar,
	injectNotificationManager,
	Input,
	ReadyTransition,
	useReadyState,
	useScrollViewport,
	useVIntl,
} from '@modrinth/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { confirm, open } from '@tauri-apps/plugin-dialog'
import { useStorage } from '@vueuse/core'
import dayjs from 'dayjs'
import { computed, ref } from 'vue'

import ReplayGroupSection from '@/components/ui/replays/ReplayGroupSection.vue'
import RenameReplayModal from '@/components/ui/replays/RenameReplayModal.vue'
import { get_full_path } from '@/helpers/instance'
import { deleteReplay, importReplay, type Replay } from '@/helpers/replays'
import { openPath } from '@/helpers/utils'

import { injectInstancePage } from '../instance-context'
import { instanceKeys, instanceReplaysQueryOptions } from '../query-options'

type ReplaySort = 'newest' | 'oldest' | 'name' | 'duration' | 'size'
type ReplayGroupBy = 'none' | 'source' | 'date'

const messages = defineMessages({
	heading: {
		id: 'app.instance.replays.heading',
		defaultMessage: '{count, plural, one {# replay} other {# replays}}',
	},
	addReplay: {
		id: 'app.instance.replays.add-replay',
		defaultMessage: 'Add replay file',
	},
	adding: {
		id: 'app.instance.replays.adding',
		defaultMessage: 'Adding...',
	},
	noReplaysHeading: {
		id: 'app.instance.replays.no-replays-heading',
		defaultMessage: 'No replays found',
	},
	noReplaysDescription: {
		id: 'app.instance.replays.no-replays-description',
		defaultMessage:
			'Recordings from ReplayMod or Flashback will show up here once you make one, or add an existing file below',
	},
	deleteConfirmTitle: {
		id: 'app.instance.replays.delete-confirm-title',
		defaultMessage: 'Delete this replay?',
	},
	deleteConfirmBody: {
		id: 'app.instance.replays.delete-confirm-body',
		defaultMessage: 'This will permanently delete {name} from disk. This cannot be undone.',
	},
	searchPlaceholder: {
		id: 'app.instance.replays.search-placeholder',
		defaultMessage: 'Search {count} replays...',
	},
	sortBy: { id: 'app.instance.replays.sort-by', defaultMessage: 'Sort by' },
	groupBy: { id: 'app.instance.replays.group-by', defaultMessage: 'Group by' },
	sortNewest: { id: 'app.instance.replays.sort.newest', defaultMessage: 'Newest' },
	sortOldest: { id: 'app.instance.replays.sort.oldest', defaultMessage: 'Oldest' },
	sortName: { id: 'app.instance.replays.sort.name', defaultMessage: 'Name' },
	sortDuration: { id: 'app.instance.replays.sort.duration', defaultMessage: 'Duration' },
	sortSize: { id: 'app.instance.replays.sort.size', defaultMessage: 'File size' },
	groupNone: { id: 'app.instance.replays.group.none', defaultMessage: 'No grouping' },
	groupSource: { id: 'app.instance.replays.group.source', defaultMessage: 'Server / world' },
	groupDate: { id: 'app.instance.replays.group.date', defaultMessage: 'Date' },
	groupUnknownSource: {
		id: 'app.instance.replays.group.unknown-source',
		defaultMessage: 'Unknown',
	},
	groupSingleplayer: {
		id: 'app.instance.replays.group.singleplayer',
		defaultMessage: 'Singleplayer',
	},
	groupMultiplayer: {
		id: 'app.instance.replays.group.multiplayer',
		defaultMessage: 'Multiplayer (unknown server)',
	},
	dateToday: { id: 'app.instance.replays.group.today', defaultMessage: 'Today' },
	dateYesterday: { id: 'app.instance.replays.group.yesterday', defaultMessage: 'Yesterday' },
	dateThisWeek: { id: 'app.instance.replays.group.this-week', defaultMessage: 'This week' },
	dateThisMonth: { id: 'app.instance.replays.group.this-month', defaultMessage: 'This month' },
	selectionAriaLabel: {
		id: 'app.instance.replays.selection.aria-label',
		defaultMessage: 'Selected replays',
	},
	selectedCount: {
		id: 'app.instance.replays.selection.selected-count',
		defaultMessage: '{count} selected',
	},
	bulkDeleteTitle: {
		id: 'app.instance.replays.selection.delete-title',
		defaultMessage: 'Delete selected replays',
	},
	bulkDeleteDescription: {
		id: 'app.instance.replays.selection.delete-description',
		defaultMessage:
			'Permanently delete {count, plural, one {# replay} other {# replays}} from disk? This cannot be undone.',
	},
})

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()
const instancePage = injectInstancePage()
const queryClient = useQueryClient()

const instance = computed(() => instancePage.instance.value!)

const replaysQuery = useQuery(
	computed(() => ({
		...instanceReplaysQueryOptions(instancePage.instanceId.value),
		enabled: !!instancePage.instanceId.value,
	})),
)
const replaysReadyPending = useReadyState(replaysQuery)
const replays = computed<Replay[]>(() => replaysQuery.data.value ?? [])

const searchFilter = ref('')
const filteredReplays = computed(() => {
	const query = searchFilter.value.trim().toLowerCase()
	if (!query) return replays.value
	return replays.value.filter(
		(replay) =>
			replay.name.toLowerCase().includes(query) ||
			replay.fileName.toLowerCase().includes(query) ||
			(replay.serverName?.toLowerCase().includes(query) ?? false) ||
			(replay.worldName?.toLowerCase().includes(query) ?? false),
	)
})

// Modrinth Studios addition: sort + group controls, mirroring the
// Screenshots tab's own sort/group state (see screenshots-page/index.vue's
// `sort`/`groupBy` `useStorage` refs) so "so many replays I can't find
// anything" gets the same answer here as it already does there.
const sort = useStorage<ReplaySort>('replays-sort', 'newest')
const groupBy = useStorage<ReplayGroupBy>('replays-group', 'none')
const sortModel = computed<string>({
	get: () => sort.value,
	set: (value) => {
		sort.value = value as ReplaySort
	},
})
const groupByModel = computed<string>({
	get: () => groupBy.value,
	set: (value) => {
		groupBy.value = value as ReplayGroupBy
	},
})

// Modrinth Studios addition: per-group collapse state, keyed by the same
// `header:${key}` id used in `replayGroups` below — mirrors the Screenshots
// tab's own `collapsedGroups` (screenshots-page/index.vue).
const collapsedGroups = useStorage<Record<string, boolean>>('replays-collapsed-groups', {})
function toggleGroupCollapsed(headerId: string) {
	collapsedGroups.value = {
		...collapsedGroups.value,
		[headerId]: !collapsedGroups.value[headerId],
	}
}

const sortOptions = computed<ComboboxOption<string>[]>(() => [
	{ value: 'newest', label: formatMessage(messages.sortNewest) },
	{ value: 'oldest', label: formatMessage(messages.sortOldest) },
	{ value: 'name', label: formatMessage(messages.sortName) },
	{ value: 'duration', label: formatMessage(messages.sortDuration) },
	{ value: 'size', label: formatMessage(messages.sortSize) },
])
const groupOptions = computed<ComboboxOption<string>[]>(() => [
	{ value: 'none', label: formatMessage(messages.groupNone) },
	{ value: 'source', label: formatMessage(messages.groupSource) },
	{ value: 'date', label: formatMessage(messages.groupDate) },
])

function replayTimestamp(replay: Replay): number {
	return replay.recordedAt ?? replay.modified
}

const sortedReplays = computed(() => {
	const list = [...filteredReplays.value]
	switch (sort.value) {
		case 'oldest':
			list.sort((a, b) => replayTimestamp(a) - replayTimestamp(b))
			break
		case 'name':
			list.sort((a, b) => a.name.localeCompare(b.name))
			break
		case 'duration':
			list.sort((a, b) => (b.durationMs ?? 0) - (a.durationMs ?? 0))
			break
		case 'size':
			list.sort((a, b) => b.size - a.size)
			break
		case 'newest':
		default:
			list.sort((a, b) => replayTimestamp(b) - replayTimestamp(a))
	}
	return list
})

function sourceGroupOf(replay: Replay): string {
	if (replay.serverName) return replay.serverName
	if (replay.worldName) return replay.worldName
	if (replay.singleplayer === true) return formatMessage(messages.groupSingleplayer)
	if (replay.singleplayer === false) return formatMessage(messages.groupMultiplayer)
	return formatMessage(messages.groupUnknownSource)
}

function dateGroupOf(replay: Replay): string {
	const recorded = dayjs(replayTimestamp(replay) * 1000)
	const now = dayjs()
	if (recorded.isSame(now, 'day')) return formatMessage(messages.dateToday)
	if (recorded.isSame(now.subtract(1, 'day'), 'day')) return formatMessage(messages.dateYesterday)
	if (recorded.isSame(now, 'week')) return formatMessage(messages.dateThisWeek)
	if (recorded.isSame(now, 'month')) return formatMessage(messages.dateThisMonth)
	return recorded.format('MMMM YYYY')
}

// Modrinth Studios addition: groups render in normal document flow (see
// the template) — the same architecture the Screenshots tab uses for its
// own groups (screenshots-page/index.vue's `screenshotGroupLayouts` /
// `virtualizedScreenshotGroups`), copied here row-list-shaped instead of
// grid-shaped. `replayGroups` builds the groups themselves (by walking the
// *already sorted* list and bucketing as we go, so a group's position
// naturally follows wherever its first — by the active sort — replay
// falls); `replayGroupLayouts` gives each one a `top`/`height` in the
// page's overall flow; `virtualizedReplayGroups` then windows the ROWS
// inside whichever groups are actually near the viewport, exactly like
// Screenshots windows the photos inside its own groups.
interface ReplayGroupData {
	id: string
	label: string
	replays: Replay[]
}

const isHeaderHidden = computed(() => groupBy.value === 'none')

const replayGroups = computed<ReplayGroupData[]>(() => {
	if (groupBy.value === 'none') {
		return [{ id: 'all', label: '', replays: sortedReplays.value }]
	}

	const groupKeyOf = groupBy.value === 'source' ? sourceGroupOf : dateGroupOf
	const order: string[] = []
	const buckets = new Map<string, Replay[]>()
	for (const replay of sortedReplays.value) {
		const key = groupKeyOf(replay)
		let bucket = buckets.get(key)
		if (!bucket) {
			bucket = []
			buckets.set(key, bucket)
			order.push(key)
		}
		bucket.push(replay)
	}

	return order.map((key) => ({ id: `header:${key}`, label: key, replays: buckets.get(key)! }))
})

// ReplayItem's card is `min-h-20` (80px) with single-line/truncated content
// throughout, so it doesn't grow with content — 88px accounts for that plus
// the 8px (`gap-2`) row spacing the old flex layout used.
const REPLAY_ROW_HEIGHT = 88
// Matches ReplayGroupHeader's own `h-11` (44px).
const REPLAY_GROUP_HEADER_HEIGHT = 44
// Breathing room between one group's content and the next group's header —
// mirrors Screenshots' own SCREENSHOT_GROUP_SPACING.
const REPLAY_GROUP_SPACING = 12
// How far past the visible viewport (in px) a group's rows still get
// rendered — mirrors Screenshots' own SCREENSHOT_GROUP_OVERSCAN, sized down
// a bit since replay rows are single-column (narrower render cost per row).
const REPLAY_GROUP_OVERSCAN = 1200

interface ReplayGroupLayout extends ReplayGroupData {
	top: number
	height: number
	isOpen: boolean
	rowsHeight: number
	rowsTop: number
}

const replayGroupLayouts = computed<ReplayGroupLayout[]>(() => {
	const layouts: ReplayGroupLayout[] = []
	let top = 0
	for (const group of replayGroups.value) {
		const isOpen = isHeaderHidden.value || !collapsedGroups.value[group.id]
		const headerHeight = isHeaderHidden.value ? 0 : REPLAY_GROUP_HEADER_HEIGHT
		const rowsHeight = group.replays.length * REPLAY_ROW_HEIGHT
		const rowsTop = top + headerHeight
		const height = headerHeight + (isOpen ? rowsHeight : 0) + REPLAY_GROUP_SPACING
		layouts.push({ ...group, top, height, isOpen, rowsHeight, rowsTop })
		top += height
	}
	return layouts
})

const { listContainer, relativeScrollTop, scrollContainer, viewportHeight } = useScrollViewport()

interface VirtualizedReplayGroupLayout extends ReplayGroupLayout {
	renderedReplays: Replay[]
	virtualRowsTop: number
}

const virtualizedReplayGroups = computed<VirtualizedReplayGroupLayout[]>(() => {
	const hasViewport = Boolean(listContainer.value && scrollContainer.value)
	const viewportStart = hasViewport
		? Math.max(0, relativeScrollTop.value - REPLAY_GROUP_OVERSCAN)
		: 0
	const viewportEnd = hasViewport
		? relativeScrollTop.value + viewportHeight.value + REPLAY_GROUP_OVERSCAN
		: REPLAY_GROUP_OVERSCAN

	return replayGroupLayouts.value.map((layout) => {
		const isInRenderRange = layout.top + layout.height >= viewportStart && layout.top <= viewportEnd
		if (!layout.isOpen || layout.replays.length === 0 || !isInRenderRange) {
			return { ...layout, renderedReplays: [], virtualRowsTop: 0 }
		}

		const rowCount = layout.replays.length
		const firstRow = Math.min(
			rowCount,
			Math.max(0, Math.floor((viewportStart - layout.rowsTop) / REPLAY_ROW_HEIGHT)),
		)
		const lastRow = Math.min(
			rowCount,
			Math.max(firstRow, Math.ceil((viewportEnd - layout.rowsTop) / REPLAY_ROW_HEIGHT)),
		)

		return {
			...layout,
			renderedReplays: layout.replays.slice(firstRow, lastRow),
			virtualRowsTop: firstRow * REPLAY_ROW_HEIGHT,
		}
	})
})

// Modrinth Studios addition: bulk selection, keyed the same way the row
// `:key` already is (kind + file name is unique per instance).
function selectionKey(replay: Replay): string {
	return `${replay.kind}-${replay.fileName}`
}
const selectedKeys = ref(new Set<string>())
function toggleSelection(replay: Replay) {
	const key = selectionKey(replay)
	const next = new Set(selectedKeys.value)
	if (next.has(key)) {
		next.delete(key)
	} else {
		next.add(key)
	}
	selectedKeys.value = next
}
function clearSelection() {
	if (!bulkDeleting.value) selectedKeys.value = new Set()
}

const importing = ref(false)
const bulkDeleting = ref(false)
const renameModal = ref<InstanceType<typeof RenameReplayModal>>()
const bulkDeleteModal = ref<InstanceType<typeof ConfirmModal>>()

async function refresh() {
	await queryClient.invalidateQueries({ queryKey: instanceKeys.replays(instance.value.id) })
	await queryClient.invalidateQueries({ queryKey: instanceKeys.hasReplays(instance.value.id) })
}

async function addReplay() {
	if (importing.value) return
	const files = await open({
		multiple: true,
		filters: [{ name: 'Replays', extensions: ['mcpr', 'zip'] }],
	}).catch(() => null)
	if (!files) return

	importing.value = true
	try {
		const paths = (Array.isArray(files) ? files : [files]).map((f) =>
			typeof f === 'string' ? f : (f as { path: string }).path,
		)
		for (const path of paths) {
			await importReplay(instance.value.id, path).catch(handleError)
		}
		await refresh()
	} finally {
		importing.value = false
	}
}

async function openFolder(replay: Replay) {
	const fullPath = await get_full_path(instance.value.id)
	await openPath(`${fullPath}/${replay.folder}`)
}

function startRename(replay: Replay) {
	renameModal.value?.show(replay)
}

async function onRenamed() {
	await refresh()
}

async function promptDelete(replay: Replay) {
	const confirmed = await confirm(formatMessage(messages.deleteConfirmBody, { name: replay.name }), {
		title: formatMessage(messages.deleteConfirmTitle),
		kind: 'warning',
	})
	if (!confirmed) return
	await deleteReplay(instance.value.id, replay.kind, replay.fileName).catch(handleError)
	await refresh()
}

async function deleteSelected() {
	if (bulkDeleting.value || selectedKeys.value.size === 0) return
	const targets = replays.value.filter((replay) => selectedKeys.value.has(selectionKey(replay)))
	bulkDeleting.value = true
	try {
		for (const replay of targets) {
			await deleteReplay(instance.value.id, replay.kind, replay.fileName).catch(handleError)
		}
		selectedKeys.value = new Set()
		await refresh()
	} finally {
		bulkDeleting.value = false
	}
}
</script>
