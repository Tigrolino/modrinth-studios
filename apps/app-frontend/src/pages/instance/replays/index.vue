<!--
	Modrinth Studios addition: shows ReplayMod / Flashback recordings found in
	this instance. New file, new route — doesn't modify any existing
	upstream instance tab. Row rendering lives in ReplayItem.vue, built to
	mirror WorldItem.vue's card so this tab looks consistent with Worlds.
-->
<template>
	<RenameReplayModal ref="renameModal" :instance-id="instance.id" @submit="onRenamed" />
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
			<Input
				v-if="replays.length > 8"
				v-model="searchFilter"
				:icon="SearchIcon"
				type="text"
				autocomplete="off"
				:spellcheck="false"
				input-class="!h-10"
				clearable
				:placeholder="formatMessage(messages.searchPlaceholder, { count: replays.length })"
			/>
			<!--
				Modrinth Studios addition: virtualized. See the
				`useVirtualScroll` call below for why — rendering every
				ReplayItem at once mounted (and thumbnail-fetched) all of
				them simultaneously, which is what caused the multi-second
				freeze on instances with hundreds/thousands of replays.
			-->
			<div ref="listContainer" class="relative w-full" :style="{ height: `${totalHeight}px` }">
				<div
					v-for="(replay, index) in visibleItems"
					:key="`${replay.kind}-${replay.fileName}`"
					class="absolute inset-x-0"
					:style="{ transform: `translateY(${visibleTop + index * REPLAY_ROW_HEIGHT}px)` }"
				>
					<ReplayItem
						:replay="replay"
						:instance-id="instance.id"
						@open-folder="openFolder(replay)"
						@rename="startRename(replay)"
						@delete="promptDelete(replay)"
					/>
				</div>
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
</template>
<script setup lang="ts">
import { PlusIcon, SearchIcon } from '@modrinth/assets'
import {
	Button,
	defineMessages,
	EmptyState,
	injectNotificationManager,
	Input,
	ReadyTransition,
	useReadyState,
	useVIntl,
	useVirtualScroll,
} from '@modrinth/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { confirm, open } from '@tauri-apps/plugin-dialog'
import { computed, ref } from 'vue'

import ReplayItem from '@/components/ui/replays/ReplayItem.vue'
import RenameReplayModal from '@/components/ui/replays/RenameReplayModal.vue'
import { get_full_path } from '@/helpers/instance'
import { deleteReplay, importReplay, type Replay } from '@/helpers/replays'
import { openPath } from '@/helpers/utils'

import { injectInstancePage } from '../instance-context'
import { instanceKeys, instanceReplaysQueryOptions } from '../query-options'

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

// Modrinth Studios addition: virtualize the replay list. Each ReplayItem row
// independently fetches its own thumbnail via a Tauri IPC call in its
// onMounted hook (a per-row `spawn_blocking` zip read on the Rust side — see
// `get_replay_thumbnail` in replays.rs), and this list previously rendered
// every replay unconditionally via `v-for`. With hundreds or (as reported)
// ~1000 replays, that meant mounting all of them — and firing all of their
// thumbnail fetches — at once, which is what caused the multi-second freeze
// on opening this tab. Only mounting the rows near the viewport keeps the
// number of concurrently in-flight thumbnail fetches bounded no matter how
// many replays an instance has, the same way screenshots-page/index.vue
// already bounds its own image work during scroll.
//
// ReplayItem's card is `min-h-20` (80px) with single-line/truncated content
// throughout, so it doesn't grow with content — 88px accounts for that plus
// the 8px (`gap-2`) row spacing the old flex layout used.
const REPLAY_ROW_HEIGHT = 88

const { listContainer, totalHeight, visibleTop, visibleItems } = useVirtualScroll(filteredReplays, {
	itemHeight: REPLAY_ROW_HEIGHT,
	bufferSize: 10,
	initialItemCount: 30,
})

const importing = ref(false)
const renameModal = ref<InstanceType<typeof RenameReplayModal>>()

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
</script>
