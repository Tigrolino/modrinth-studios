<!--
	Modrinth Studios addition: a collapsible section-header row for the
	Replays tab's group-by view (see replays/index.vue). Kept at the same
	fixed height as a real ReplayItem row on purpose — the virtualizer this
	list uses (`useVirtualScroll` from `@modrinth/ui`) assumes every item in
	its flat array is the same height, and giving headers their own shorter
	height would need a variable-height virtualizer this codebase doesn't
	have. Content is bottom-aligned within that fixed height instead, so the
	visible gap reads as "space before this new section" (above the header)
	rather than "space between the header and its own group's first row"
	(below it).
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
		class="flex h-full w-full cursor-pointer items-end gap-2 border-0 bg-transparent px-1 pb-2 text-left"
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
