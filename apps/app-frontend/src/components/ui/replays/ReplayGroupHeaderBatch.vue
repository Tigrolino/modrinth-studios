<!--
	Modrinth Studios addition: renders 2-3 consecutive collapsed group
	headers packed into ONE shared virtualizer slot (see
	`mergeCollapsedHeaderRuns` in replays/index.vue). A single
	ReplayGroupHeader.vue is forced to occupy a fixed 88px slot no matter
	how little text it holds, because the virtualizer needs every item at
	the same height — fine for one header, but a run of several in a row
	each reserving their own mostly-empty 88px box is what read as a big
	gap "between sections" with no way to trim it (see
	ReplayGroupHeader.vue's own comment for why bottom/top alignment alone
	can't fix that case).

	This component sidesteps the problem instead of fighting it: rather
	than each header centering/aligning within its own box, several
	headers share one box and stack directly against each other in a flex
	column, so there's no per-header dead space left to show up as a gap.
-->
<script setup lang="ts">
import { DropdownIcon } from '@modrinth/assets'
import { defineMessages, useVIntl } from '@modrinth/ui'

defineProps<{
	headers: { id: string; label: string; count: number }[]
}>()

const emit = defineEmits<{
	toggle: [id: string]
}>()

const { formatMessage } = useVIntl()
const messages = defineMessages({
	expandGroup: {
		id: 'app.instance.replays.group.expand',
		defaultMessage: 'Expand {label}',
	},
})
</script>

<template>
	<div class="flex h-20 w-full flex-col justify-center gap-1.5 px-1">
		<button
			v-for="header in headers"
			:key="header.id"
			type="button"
			class="flex w-full shrink-0 cursor-pointer items-center gap-2 border-0 bg-transparent p-0 text-left"
			:aria-label="formatMessage(messages.expandGroup, { label: header.label })"
			@click="emit('toggle', header.id)"
		>
			<DropdownIcon class="size-4 shrink-0 text-secondary" />
			<h3 class="m-0 truncate text-sm font-semibold uppercase tracking-wide text-secondary">
				{{ header.label }}
			</h3>
			<span class="shrink-0 text-sm tabular-nums text-secondary">{{ header.count }}</span>
			<div class="h-px flex-1 bg-surface-4" />
		</button>
	</div>
</template>
