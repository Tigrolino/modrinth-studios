<!--
	Modrinth Studios addition: a collapsible section-header row for the
	Replays tab's group-by view (see replays/index.vue). Kept at the same
	fixed height as a real ReplayItem row on purpose — the virtualizer this
	list uses (`useVirtualScroll` from `@modrinth/ui`) assumes every item in
	its flat array is the same height, and giving headers their own shorter
	height would need a variable-height virtualizer this codebase doesn't
	have.

	The root element sets an *explicit* height (`h-20`, 80px — the same
	80px a ReplayItem card renders at within its own 88px slot, via its
	`min-h-20`) rather than `h-full`: the wrapper `index.vue` renders this
	inside (`<div class="absolute inset-x-0">`) never gets an explicit
	height itself (it's only positioned via `transform: translateY(...)`),
	so `h-full` silently resolved to nothing and this button collapsed to
	its own text's height instead of the 88px slot the virtualizer had
	already reserved for it — the gap this was supposed to avoid, showing
	up both above a group's first replay and between two consecutive
	collapsed group headers.

	Content is bottom-aligned (`items-end`) within that 80px box rather
	than vertically centered — centering a short single line of text in an
	80px-tall box splits the empty space evenly above *and* below the
	text, which visually reads as a big gap both before AND after every
	header (the text itself is only ~20px tall). Pinning it to the bottom
	consolidates all of that empty space above the header instead, so it
	reads as one gap — "space before this new section" — and the header's
	text sits right up against whatever comes directly after it, open
	group or collapsed neighbor alike.
-->
<script setup lang="ts">
import { DropdownIcon } from '@modrinth/assets'
import { useVIntl, defineMessages } from '@modrinth/ui'

const props = defineProps<{
	label: string
	count: number
	isOpen: boolean
}>()

const emit = defineEmits<{
	toggle: []
}>()

const { formatMessage } = useVIntl()
const messages = defineMessages({
	collapseGroup: { id: 'app.instance.replays.group.collapse', defaultMessage: 'Collapse {label}' },
	expandGroup: { id: 'app.instance.replays.group.expand', defaultMessage: 'Expand {label}' },
})
</script>

<template>
	<button
		type="button"
		class="flex h-20 w-full cursor-pointer items-end gap-2 border-0 bg-transparent px-1 pb-2 text-left"
		:aria-expanded="isOpen"
		:aria-label="
			formatMessage(isOpen ? messages.collapseGroup : messages.expandGroup, { label: props.label })
		"
		@click="emit('toggle')"
	>
		<DropdownIcon
			class="size-4 shrink-0 text-secondary transition-transform duration-200"
			:class="{ 'rotate-180': isOpen }"
		/>
		<h3 class="m-0 text-sm font-semibold text-secondary uppercase tracking-wide truncate">
			{{ label }}
		</h3>
		<span class="text-sm text-secondary tabular-nums shrink-0">{{ count }}</span>
		<div class="h-px flex-1 bg-surface-4" />
	</button>
</template>
