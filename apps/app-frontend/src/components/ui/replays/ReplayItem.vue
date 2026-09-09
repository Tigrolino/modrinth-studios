<!--
	Modrinth Studios addition: new file. Deliberately mirrors WorldItem.vue's
	card markup/classes (grid layout, TagItem usage, action button column)
	as closely as the different data shape allows, instead of inventing a
	new row style, so the Replays tab looks consistent with the Worlds tab.
-->
<script setup lang="ts">
import {
	CheckIcon,
	EditIcon,
	FolderOpenIcon,
	MoreVerticalIcon,
	TrashIcon,
	VideoIcon,
} from '@modrinth/assets'
import {
	BulletDivider,
	commonMessages,
	defineMessages,
	TagItem,
	TeleportOverflowMenu,
	useFormatDateTime,
	useVIntl,
} from '@modrinth/ui'
import { onBeforeUnmount, onMounted, ref } from 'vue'

import { getReplayThumbnail, type Replay } from '@/helpers/replays'

// Modrinth Studios addition: how long a press has to be held before it
// counts as "hold to select" rather than a plain click. Matches the
// mobile/file-manager convention of a long-press entering selection mode.
const LONG_PRESS_MS = 450
// A real long-press involves some hand tremor; anything past this many CSS
// pixels of movement while still holding is treated as an intentional drag
// (e.g. starting a scroll) rather than a hold, and cancels the timer.
const LONG_PRESS_MOVE_TOLERANCE_PX = 8

const props = withDefaults(
	defineProps<{
		replay: Replay
		instanceId: string
		selected?: boolean
		// Modrinth Studios addition: whether *any* replay is currently
		// selected across the whole list. While this is false, rows have no
		// checkbox at all (no permanently-reserved layout space for one
		// either — see the row's `grid-cols-*` toggle in the template) and
		// the only way to select anything is a press-and-hold. Once it's
		// true, every row gets a checkbox and starts responding to plain
		// clicks too.
		selectionActive?: boolean
	}>(),
	{
		selected: false,
		selectionActive: false,
	},
)

const emit = defineEmits<{
	'open-folder': []
	rename: []
	delete: []
	'toggle-selection': []
}>()

const { formatMessage } = useVIntl()
const formatDateTime = useFormatDateTime({ timeStyle: 'short', dateStyle: 'medium' })

// Modrinth Studios addition: fetched lazily per-row instead of coming back
// with the rest of the replay's fields from `listReplays` — see the comment
// on `get_replay_thumbnail` in replays.rs for why bundling image bytes into
// that bulk call isn't a good idea with hundreds of replays. `undefined`
// while loading (shows the fallback icon), `null` once confirmed the replay
// has no embedded thumbnail (also shows the fallback icon, just without
// retrying), a data: URL once loaded.
const thumbnail = ref<string | null | undefined>(undefined)

onMounted(async () => {
	thumbnail.value = await getReplayThumbnail(
		props.instanceId,
		props.replay.kind,
		props.replay.fileName,
	).catch(() => null)
})

// Modrinth Studios addition: press-and-hold anywhere on the row to select
// it (entering selection mode), then — once at least one replay is
// selected — a plain click anywhere on the row (not just a small checkbox
// target) toggles it too. `suppressClick` swallows the click event a
// browser fires right after the pointerup that ends a long press, so
// finishing a hold doesn't immediately toggle the row back off again.
let pressTimer: ReturnType<typeof setTimeout> | undefined
let pressOrigin: { x: number; y: number } | undefined
let suppressClick = false

function clearPressTimer() {
	if (pressTimer !== undefined) {
		clearTimeout(pressTimer)
		pressTimer = undefined
	}
	pressOrigin = undefined
}

function onRowPointerDown(event: PointerEvent) {
	// Left mouse button / primary touch contact only — ignore right-click,
	// middle-click, etc.
	if (event.button !== 0) return
	pressOrigin = { x: event.clientX, y: event.clientY }
	clearTimeout(pressTimer)
	pressTimer = setTimeout(() => {
		pressTimer = undefined
		suppressClick = true
		emit('toggle-selection')
	}, LONG_PRESS_MS)
}

function onRowPointerMove(event: PointerEvent) {
	if (!pressOrigin) return
	const dx = event.clientX - pressOrigin.x
	const dy = event.clientY - pressOrigin.y
	if (Math.hypot(dx, dy) > LONG_PRESS_MOVE_TOLERANCE_PX) clearPressTimer()
}

function onRowClick() {
	if (suppressClick) {
		suppressClick = false
		return
	}
	if (props.selectionActive) emit('toggle-selection')
}

onBeforeUnmount(clearPressTimer)

const messages = defineMessages({
	moreOptions: {
		id: 'app.instance.replays.more-options',
		defaultMessage: 'More options',
	},
	select: { id: 'app.instance.replays.select', defaultMessage: 'Select' },
	deselect: { id: 'app.instance.replays.deselect', defaultMessage: 'Deselect' },
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
		class="clickable-card select-none grid items-center gap-2 p-3 bg-bg-raised border border-solid border-surface-4 rounded-[20px] transition-[filter] ease-out min-h-20"
		:class="[
			selectionActive
				? 'grid-cols-[auto_auto_minmax(0,3fr)_minmax(0,4fr)_auto] cursor-pointer'
				: 'grid-cols-[auto_minmax(0,3fr)_minmax(0,4fr)_auto]',
			{ '!border-brand': selected },
		]"
		@pointerdown="onRowPointerDown"
		@pointermove="onRowPointerMove"
		@pointerup="clearPressTimer"
		@pointerleave="clearPressTimer"
		@pointercancel="clearPressTimer"
		@click="onRowClick"
	>
		<!--
			Modrinth Studios addition: bulk-selection checkbox — mirrors the
			Library instance card's circular selection toggle (same visual
			language). Only shown once selection mode is actually active
			(hold-to-select on any row starts it — see `onRowPointerDown`);
			it's a discrete, still-clickable target for precision/keyboard
			use, but once selection is active a plain click anywhere else on
			the row toggles it too (`onRowClick`), so this is a convenience,
			not the only way in.
		-->
		<button
			v-if="selectionActive"
			type="button"
			class="flex items-center justify-center size-6 shrink-0 rounded-full border-0 bg-transparent p-0 cursor-pointer"
			:aria-label="formatMessage(selected ? messages.deselect : messages.select)"
			:aria-pressed="selected"
			@pointerdown.stop
			@click.stop="emit('toggle-selection')"
		>
			<span
				class="relative flex items-center justify-center size-5 rounded-full"
				:class="
					selected
						? 'bg-brand'
						: 'border-2 border-solid border-secondary bg-transparent hover:border-primary'
				"
			>
				<CheckIcon v-if="selected" class="size-3.5 [stroke-width:3]" />
			</span>
		</button>
		<div
			class="flex items-center justify-center size-12 shrink-0 overflow-hidden !rounded-[14px]"
			:class="
				thumbnail
					? 'bg-surface-4'
					: replay.kind === 'flashback'
						? 'bg-[color-mix(in_srgb,var(--color-purple)_18%,transparent)] text-purple'
						: 'bg-brand-highlight text-brand'
			"
		>
			<img
				v-if="thumbnail"
				:src="thumbnail"
				alt=""
				loading="lazy"
				decoding="async"
				class="size-full object-cover"
			/>
			<VideoIcon v-else class="size-6" />
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
			<!--
				Modrinth Studios: this used to default to "Singleplayer" any time
				`replay.singleplayer` wasn't explicitly `false` — which included
				`undefined`/`null`, i.e. every replay whose metadata just didn't
				have that field at all. That was *always* the case for Flashback
				replays: confirmed against Flashback's own `FlashbackMeta.java`,
				its metadata.json has no `singleplayer`/`serverName` fields at
				all — it never records that distinction, it's not just sometimes
				missing. So "unknown" was rendering as "confirmed singleplayer"
				for every single Flashback replay, which is exactly backwards for
				anyone who mostly plays on servers.

				What Flashback's metadata does have is `world_name` (the
				recorded world/server's own display name) — show that instead of
				guessing. For ReplayMod, `serverName`/`singleplayer` are real,
				confirmed fields (via ReplayStudio's `ReplayMetaData.java`), so
				that guess is legitimate there and is kept as a fallback.
			-->
			<template v-if="replay.serverName || replay.worldName || typeof replay.singleplayer === 'boolean'">
				<span class="truncate">
					{{
						replay.serverName ??
						replay.worldName ??
						(replay.singleplayer
							? formatMessage(messages.singleplayer)
							: formatMessage(messages.multiplayer))
					}}
				</span>
				<BulletDivider />
			</template>
			<template v-if="replay.durationMs">
				<span class="shrink-0 font-normal">{{ formatDuration(replay.durationMs) }}</span>
				<BulletDivider />
			</template>
			<span class="shrink-0 font-normal">{{ formatFileSize(replay.size) }}</span>
		</div>
		<div class="flex gap-1 justify-end" @pointerdown.stop @click.stop>
			<!--
				Modrinth Studios addition: `@pointerdown.stop`/`@click.stop`
				above keep the row's hold-to-select / click-to-toggle
				handlers from firing when someone's actually just trying to
				open this menu.
			-->
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
