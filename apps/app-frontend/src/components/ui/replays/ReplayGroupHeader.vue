<!--
	Modrinth Studios addition: a section-header row for the Replays tab's
	group-by view (see replays/index.vue). Kept at the same fixed height as
	a real ReplayItem row on purpose — the virtualizer this list uses
	(`useVirtualScroll` from `@modrinth/ui`) assumes every item in its flat
	array is the same height, and giving headers their own shorter height
	would need a variable-height virtualizer this codebase doesn't have.
	Trades a slightly spacious header for reusing the existing, already
	battle-tested (see the freeze-fix history in STUDIO.md) virtualizer as-is.
-->
<script setup lang="ts">
defineProps<{
	label: string
	count: number
}>()
</script>

<template>
	<div class="flex h-full items-center gap-2 px-1">
		<h3 class="m-0 text-sm font-semibold text-secondary uppercase tracking-wide truncate">
			{{ label }}
		</h3>
		<span class="text-sm text-secondary tabular-nums shrink-0">{{ count }}</span>
		<div class="h-px flex-1 bg-surface-4" />
	</div>
</template>
