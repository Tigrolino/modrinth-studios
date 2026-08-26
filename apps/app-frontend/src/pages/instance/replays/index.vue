<!--
	Modrinth Studios addition: shows ReplayMod / Flashback recordings found in
	this instance. New file, new route — doesn't modify any existing
	upstream instance tab.
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
			<div class="flex flex-col w-full gap-2">
				<div
					v-for="replay in filteredReplays"
					:key="`${replay.kind}-${replay.fileName}`"
					class="flex items-center gap-3 rounded-2xl border border-solid border-divider bg-bg-raised p-3"
				>
					<div
						class="flex items-center justify-center size-10 rounded-xl shrink-0"
						:class="
							replay.kind === 'flashback'
								? 'bg-[color-mix(in_srgb,var(--color-purple)_18%,transparent)] text-purple'
								: 'bg-brand-highlight text-brand'
						"
					>
						<VideoIcon class="size-5" />
					</div>
					<div class="flex flex-col min-w-0 flex-1">
						<div class="flex items-center gap-2 flex-wrap">
							<span class="font-semibold text-contrast truncate">{{ replay.name }}</span>
							<TagItem class="text-xs" :style="`--_color: var(--color-secondary)`">
								{{ replay.kind === 'flashback' ? 'Flashback' : 'ReplayMod' }}
							</TagItem>
							<span v-if="replay.minecraftVersion" class="text-sm text-secondary">
								{{ replay.minecraftVersion }}
							</span>
						</div>
						<div class="flex items-center gap-2 flex-wrap text-sm text-secondary">
							<span class="truncate font-medium text-primary">
								{{
									replay.serverName ??
									(replay.singleplayer !== false
										? formatMessage(messages.singleplayer)
										: formatMessage(messages.multiplayer))
								}}
							</span>
							<BulletDivider />
							<span class="truncate">{{ replay.fileName }}</span>
							<BulletDivider />
							<span>{{
								formatDateTime(new Date((replay.recordedAt ?? replay.modified) * 1000))
							}}</span>
							<template v-if="replay.durationMs">
								<BulletDivider />
								<span>{{ formatDuration(replay.durationMs) }}</span>
							</template>
							<BulletDivider />
							<span>{{ formatFileSize(replay.size) }}</span>
						</div>
					</div>
					<div class="flex items-center gap-1 shrink-0">
						<Button type="colored" color="brand" :disabled="launching" @click="launch(replay)">
							<PlayIcon class="size-4" />
							{{ formatMessage(messages.launch) }}
						</Button>
						<IconButton :label="formatMessage(messages.openFolder)" @click="openFolder(replay)">
							<FolderOpenIcon />
						</IconButton>
						<IconButton
							:label="formatMessage(commonMessages.renameButton)"
							@click="startRename(replay)"
						>
							<EditIcon />
						</IconButton>
						<IconButton
							:label="formatMessage(commonMessages.deleteLabel)"
							class="hover:!text-brand-red"
							@click="promptDelete(replay)"
						>
							<TrashIcon />
						</IconButton>
					</div>
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
import {
	EditIcon,
	FolderOpenIcon,
	PlayIcon,
	PlusIcon,
	SearchIcon,
	TrashIcon,
	VideoIcon,
} from '@modrinth/assets'
import {
	BulletDivider,
	Button,
	commonMessages,
	defineMessages,
	EmptyState,
	IconButton,
	injectNotificationManager,
	Input,
	ReadyTransition,
	TagItem,
	useFormatDateTime,
	useReadyState,
	useVIntl,
} from '@modrinth/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { confirm, open } from '@tauri-apps/plugin-dialog'
import { computed, ref } from 'vue'

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
	launch: {
		id: 'app.instance.replays.launch',
		defaultMessage: 'Launch',
	},
	openFolder: {
		id: 'app.instance.replays.open-folder',
		defaultMessage: 'Open folder',
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
	singleplayer: {
		id: 'app.instance.replays.singleplayer',
		defaultMessage: 'Singleplayer',
	},
	multiplayer: {
		id: 'app.instance.replays.multiplayer',
		defaultMessage: 'Multiplayer',
	},
})

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()
const instancePage = injectInstancePage()
const queryClient = useQueryClient()

const formatDateTime = useFormatDateTime({ timeStyle: 'short', dateStyle: 'medium' })

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
			(replay.serverName?.toLowerCase().includes(query) ?? false),
	)
})

const importing = ref(false)
const launching = ref(false)
const renameModal = ref<InstanceType<typeof RenameReplayModal>>()

function formatDuration(ms: number): string {
	const totalSeconds = Math.floor(ms / 1000)
	const hours = Math.floor(totalSeconds / 3600)
	const minutes = Math.floor((totalSeconds % 3600) / 60)
	const seconds = totalSeconds % 60
	const pad = (n: number) => n.toString().padStart(2, '0')
	return hours > 0 ? `${hours}:${pad(minutes)}:${pad(seconds)}` : `${minutes}:${pad(seconds)}`
}

function formatFileSize(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`
	const units = ['KB', 'MB', 'GB']
	let value = bytes / 1024
	let unitIndex = 0
	while (value >= 1024 && unitIndex < units.length - 1) {
		value /= 1024
		unitIndex++
	}
	return `${value.toFixed(1)} ${units[unitIndex]}`
}

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

async function launch(replay: Replay) {
	if (instance.value.quarantined || launching.value) return
	launching.value = true
	try {
		await instancePage.play('InstanceReplays')
	} catch (err) {
		handleError(err as Error)
	} finally {
		launching.value = false
	}
	// Neither ReplayMod nor Flashback expose a way to pre-select a replay
	// before the game boots, so this launches the instance the same way the
	// normal Play button does; open `replay.name` from the mod's own UI
	// once you're in-game.
	void replay
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
	const confirmed = await confirm(
		formatMessage(messages.deleteConfirmBody, { name: replay.name }),
		{ title: formatMessage(messages.deleteConfirmTitle), kind: 'warning' },
	)
	if (!confirmed) return
	await deleteReplay(instance.value.id, replay.kind, replay.fileName).catch(handleError)
	await refresh()
}
</script>
