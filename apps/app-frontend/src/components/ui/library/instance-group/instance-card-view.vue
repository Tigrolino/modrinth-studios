<script setup lang="ts">
import { Avatar, truncatedTooltip } from '@modrinth/ui'
import { computed, ref } from 'vue'

import { useAppSettings } from '@/composables/use-app-settings.ts'
import { useImageThumbnail } from '@/composables/use-image-thumbnail'
import { getInstanceIconUrl } from '@/helpers/instance'
import type { GameInstance } from '@/helpers/types'

const props = withDefaults(
	defineProps<{
		instance: GameInstance
		selected?: boolean
	}>(),
	{
		selected: false,
	},
)

const localIcon = computed(() => {
	const path = props.instance.icon_path
	return path && !/^https?:/.test(path) && !path.toLowerCase().endsWith('.svg') ? path : undefined
})
const appSettings = useAppSettings()
const compactMode = computed(() => appSettings.getFeatureFlag('compact_instance_cards'))
const thumbnail = useImageThumbnail(
	localIcon,
	() => (compactMode.value ? 96 : 384),
	() => String(props.instance.modified),
)
const iconSrc = computed(() =>
	localIcon.value ? thumbnail.value : getInstanceIconUrl(props.instance.icon_path),
)

const nameRef = ref<HTMLElement | null>(null)
const versionRef = ref<HTMLElement | null>(null)

// Modrinth Studios addition: the root `<div>` below uses a narrowed
// `transition-[...]` list instead of `transition-all`, which (as its name
// says) transitions *every* animatable property that changes on the
// element — including its own `width`, since that's set by the grid's
// responsive `auto-fill`/`minmax` column sizing in the parent, not by us.
// During any resize (window drag *or* the sidebar's push animation), that
// width is changing continuously, many times a second — so the card's
// actual rendered box was perpetually chasing a 150ms-lagged,
// constantly-moving target instead of just tracking the grid directly,
// which is exactly what reads as a springy "jiggle" rather than a clean
// resize. This was never about Vue re-rendering or the TransitionGroup move
// animation at all (two earlier, wrong guesses at this same bug) —
// narrowing the transition to only the properties that are actually meant
// to animate here (hover/selection color, border, brightness, and the
// click/drag scale transform) leaves the card's layout box itself untouched
// by any transition, so it just tracks the grid's real size instantly, same
// as every other element on the page.
//
// This comment used to live directly inside <template>, right above the
// root <div> — turns out that's exactly what was silently breaking card
// dragging: with this comment there, `InstanceCardView`'s compiled render
// produced more than one root node (an empty placeholder alongside the real
// div), so Vue's own `$el`/`subTree.el` for this component pointed at that
// empty placeholder instead of the actual element. `instance-card.vue`'s
// `useDraggable` hands dnd-kit exactly that `$el`, so dnd-kit was being
// handed a non-element every time — with no error, since a placeholder text
// node is still a valid enough object to pass around, it just isn't
// draggable. Moved here since a `<script>`-level comment can never affect
// the compiled template, whatever the root cause in Vue/the compiler
// actually was.
</script>

<template>
	<div
		class="relative flex w-full min-w-0 select-none overflow-clip border border-solid bg-surface-3 text-left transition-[color,background-color,border-color,filter,transform] duration-150 ease-in-out"
		:class="{
			'flex-row items-center justify-start gap-2.5 rounded-xl p-2.5': compactMode,
			'flex-col items-start justify-end gap-3 rounded-[20px] p-3': !compactMode,
			'[border-color:color-mix(in_srgb,var(--color-text-primary)_40%,transparent)] brightness-110':
				selected,
			'border-surface-4': !selected,
		}"
	>
		<div
			class="relative flex shrink-0 items-center max-w-full overflow-clip"
			:class="compactMode ? 'size-10 rounded-lg' : 'aspect-square min-w-full rounded-2xl'"
		>
			<Avatar
				class="pointer-events-none outline-none"
				:class="compactMode ? '!rounded-lg' : '!rounded-2xl'"
				size="100%"
				:src="iconSrc"
				loading="lazy"
				:tint-by="instance.id"
				alt=""
				no-shadow
				pad-transparent-corners
			/>
			<slot name="loading" :compact="compactMode" />
			<div
				class="absolute z-[1] flex items-center justify-center"
				:class="compactMode ? 'inset-0' : 'bottom-1.5 right-1.5 size-12'"
			>
				<slot name="leading" :compact="compactMode" />
			</div>
		</div>
		<div
			class="flex min-w-0 w-full flex-col items-start justify-center gap-1 px-0.5"
			:class="{ 'pr-10': compactMode }"
		>
			<p
				ref="nameRef"
				v-tooltip="truncatedTooltip(nameRef, instance.name)"
				class="m-0 w-full truncate text-base font-semibold leading-5 text-contrast"
			>
				{{ instance.name }}
			</p>
			<p
				ref="versionRef"
				v-tooltip="truncatedTooltip(versionRef, `${instance.loader} ${instance.game_version}`)"
				class="m-0 w-full truncate text-sm font-medium capitalize leading-[18px] text-primary"
			>
				{{ instance.loader }} {{ instance.game_version }}
			</p>
		</div>
		<slot name="overlay" :compact="compactMode" />
	</div>
</template>
