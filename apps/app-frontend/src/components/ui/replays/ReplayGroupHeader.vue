<!--
	Modrinth Studios addition: a collapsible section-header row for the
	Replays tab's group-by view (see replays/index.vue). Two earlier
	versions of this file tried to make a header look right while still
	occupying the SAME fixed 88px slot every ReplayItem row does — first by
	centering its content in that box, then by bottom-aligning it — and
	both approaches only ever moved *where* the box's leftover ~60px of
	dead space sat, never got rid of it. That was fine for a single open
	group (bottom-aligning happened to put the dead space where nothing
	needed it), but a run of several consecutive collapsed headers meant
	several of those mostly-empty boxes stacked on each other, which read
	as a big, uneven gap "between sections" no per-header alignment tweak
	could close (the dead space was trapped inside each header's own slot,
	not shared with its neighbor).

	The real fix was to stop pretending a header needs the same height as
	a replay card at all. `useVirtualScroll` (see
	`packages/ui/src/composables/virtual-scroll.ts`) now accepts a
	per-item height function, so headers get their own short, uniform
	HEADER_ROW_HEIGHT slot (see replays/index.vue) instead of ReplayItem's
	88px — every header, whether it's alone, part of a long collapsed run,
	or sitting right above an open group's first row, gets the exact same
	small amount of padding. Centering the content in that shorter box is
	enough on its own now; there's no large slack left to distribute.
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
		class="flex h-11 w-full cursor-pointer items-center gap-2 border-0 bg-transparent px-1 text-left"
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
