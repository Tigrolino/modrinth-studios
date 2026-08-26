<!--
	Modrinth Studios addition: new file. Deliberately mirrors WorldItem.vue's
	card markup/classes (grid layout, TagItem usage, action button column)
	as closely as the different data shape allows, instead of inventing a
	new row style, so the Replays tab looks consistent with the Worlds tab.
-->
<script setup lang="ts">
import { EditIcon, FolderOpenIcon, MoreVerticalIcon, TrashIcon, VideoIcon } from '@modrinth/assets'
import {
	BulletDivider,
	commonMessages,
	defineMessages,
	TagItem,
	TeleportOverflowMenu,
	useFormatDateTime,
	useVIntl,
} from '@modrinth/ui'

import type { Replay } from '@/helpers/replays'

const props = defineProps<{
	replay: Replay
}>()

const emit = defineEmits<{
	'open-folder': []
	rename: []
	delete: []
}>()

const { formatMessage } = useVIntl()
const formatDateTime = useFormatDateTime({ timeStyle: 'short', dateStyle: 'medium' })

const messages = defineMessages({
	moreOptions: {
		id: 'app.instance.replays.more-options',
		defaultMessage: 'More options',
	},
	openFolder: {
		id: 'app.instance.replays.open-folder',
		defaultMessage: 'Open folder',
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
</script>

<template>
	<div
		class="clickable-card grid grid-cols-[auto_minmax(0,3fr)_minmax(0,4fr)_auto] items-center gap-2 p-3 bg-bg-raised border border-solid border-surface-4 rounded-[20px] transition-[filter] ease-out min-h-20"
	>
		<div
			class="flex items-center justify-center size-12 shrink-0 !rounded-[14px]"
			:class="
				replay.kind === 'flashback'
					? 'bg-[color-mix(in_srgb,var(--color-purple)_18%,transparent)] text-purple'
					: 'bg-brand-highlight text-brand'
			"
		>
			<VideoIcon class="size-6" />
		</div>
		<div class="flex flex-col justify-center gap-0.5 h-full min-w-0">
			<div class="flex items-center gap-1.5">
				<div class="text-base text-contrast font-semibold truncate">{{ replay.name }}</div>
				<TagItem class="text-xs" :style="`--_color: var(--color-secondary)`">
					{{ replay.kind === 'flashback' ? 'Flashback' : 'ReplayMod' }}
				</TagItem>
				<span v-if="replay.minecraftVersion" class="text-sm text-secondary shrink-0">
					{{ replay.minecraftVersion }}
				</span>
			</div>
			<div class="flex items-center gap-1.5 text-sm text-secondary min-w-0">
				<span class="truncate">{{ replay.fileName }}</span>
				<BulletDivider class="shrink-0" />
				<span class="shrink-0">
					{{ formatDateTime(new Date((replay.recordedAt ?? replay.modified) * 1000)) }}
				</span>
			</div>
		</div>
		<div class="font-semibold flex items-center gap-1 justify-center text-center text-secondary">
			<span class="truncate">
				{{
					replay.serverName ??
					(replay.singleplayer !== false
						? formatMessage(messages.singleplayer)
						: formatMessage(messages.multiplayer))
				}}
			</span>
			<template v-if="replay.durationMs">
				<BulletDivider />
				<span class="shrink-0 font-normal">{{ formatDuration(replay.durationMs) }}</span>
			</template>
			<BulletDivider />
			<span class="shrink-0 font-normal">{{ formatFileSize(replay.size) }}</span>
		</div>
		<div class="flex gap-1 justify-end">
			<!--
				Modrinth Studios: deliberately no Play/Launch button here. Neither
				ReplayMod nor Flashback expose any way to launch Minecraft
				pre-loaded into a specific replay file, so a button here could only
				ever start the instance normally — which would look like it should
				open this replay and not do that. Open the replay from the mod's
				own in-game screen after launching the instance instead.
			-->
			<TeleportOverflowMenu
				type="quiet"
				:label="formatMessage(messages.moreOptions)"
				:options="[
					{
						id: 'open-folder',
						label: formatMessage(messages.openFolder),
						action: () => emit('open-folder'),
					},
					{
						id: 'rename',
						label: formatMessage(commonMessages.renameButton),
						action: () => emit('rename'),
					},
					{
						id: 'delete',
						label: formatMessage(commonMessages.deleteLabel),
						tone: 'red',
						action: () => emit('delete'),
					},
				]"
			>
				<MoreVerticalIcon aria-hidden="true" />
				<template #open-folder>
					<FolderOpenIcon aria-hidden="true" />
					{{ formatMessage(messages.openFolder) }}
				</template>
				<template #rename>
					<EditIcon aria-hidden="true" />
					{{ formatMessage(commonMessages.renameButton) }}
				</template>
				<template #delete>
					<TrashIcon aria-hidden="true" />
					{{ formatMessage(commonMessages.deleteLabel) }}
				</template>
			</TeleportOverflowMenu>
		</div>
	</div>
</template>
