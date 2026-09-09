<!--
	Modrinth Studios addition: this is the Replays tab copying, directly,
	the technique the Screenshots tab already uses for its own groups (see
	screenshots-page/group.vue + section.vue) — two earlier attempts at a
	Replays-specific open/close animation (a `transition-transform` on
	absolutely-positioned rows, then a manual FLIP) never actually
	animated anything, because the whole list lived in ONE globally
	absolutely-positioned array: toggling a group didn't just resize that
	group, it reassigned every row below it to a different array index,
	so from the DOM's perspective almost nothing below the toggle was
	"the same element moving" — there was nothing continuous to transition.

	Screenshots never has that problem because each group is a normal,
	in-flow block; toggling one only ever changes *that* block's own
	height, and the browser's layout engine naturally reflows and repaints
	everything below it — no JS position math needed for that part at all.
	This component reproduces exactly that: it renders in normal flow
	(the parent — replays/index.vue — just `v-for`s these, no absolute
	positioning at the group level), and its own collapse/expand uses the
	identical CSS `grid-template-rows: 0fr -> 1fr` trick as the shared
	`Accordion` component (`packages/ui/src/components/base/Accordion.vue`)
	that Screenshots' section.vue already relies on.

	Only the ROWS *inside* an open group are virtualized (still needed —
	one group here can hold 700+ replays), using the same
	windowed-slice-plus-local-offset approach Screenshots' group.vue uses
	for its own photo grid (`renderedReplays`/`virtualRowsTop`, computed by
	the parent from page scroll position — see `virtualizedReplayGroups`
	in replays/index.vue).
-->
<script setup lang="ts">
import type { Replay } from '@/helpers/replays'

import ReplayGroupHeader from './ReplayGroupHeader.vue'
import ReplayItem from './ReplayItem.vue'

const props = defineProps<{
	label: string
	replays: Replay[]
	renderedReplays: Replay[]
	rowsHeight: number
	virtualRowsTop: number
	collapsed: boolean
	hideHeader: boolean
	instanceId: string
	selectedKeys: ReadonlySet<string>
	selectionActive: boolean
}>()

const emit = defineEmits<{
	(e: 'update:collapsed', collapsed: boolean): void
	(e: 'toggle-selection' | 'open-folder' | 'rename' | 'delete', replay: Replay): void
}>()

// Keep in sync with replays/index.vue's REPLAY_ROW_HEIGHT — this is the
// per-row spacing used to lay out `renderedReplays` within this group's
// own local (not page-relative) virtualized window.
const ROW_HEIGHT = 88

function selectionKey(replay: Replay): string {
	return `${replay.kind}-${replay.fileName}`
}
</script>

<template>
	<div class="flex w-full flex-col">
		<ReplayGroupHeader
			v-if="!hideHeader"
			:label="label"
			:count="replays.length"
			:is-open="!collapsed"
			@toggle="emit('update:collapsed', !collapsed)"
		/>
		<div class="group-content" :class="{ open: hideHeader || !collapsed }">
			<div>
				<div class="relative w-full" :style="{ height: `${rowsHeight}px` }">
					<div
						v-for="(replay, index) in renderedReplays"
						:key="`${replay.kind}-${replay.fileName}`"
						class="absolute inset-x-0"
						:style="{ transform: `translateY(${virtualRowsTop + index * ROW_HEIGHT}px)` }"
					>
						<ReplayItem
							:replay="replay"
							:instance-id="instanceId"
							:selected="selectedKeys.has(selectionKey(replay))"
							:selection-active="selectionActive"
							@toggle-selection="emit('toggle-selection', replay)"
							@open-folder="emit('open-folder', replay)"
							@rename="emit('rename', replay)"
							@delete="emit('delete', replay)"
						/>
					</div>
				</div>
			</div>
		</div>
	</div>
</template>

<style scoped>
/* Modrinth Studios addition: identical to Accordion.vue's own
   `.accordion-content` — same grid-template-rows 0fr/1fr collapse trick,
   copied locally rather than reusing that component directly so this can
   keep ReplayGroupHeader's own header markup (chevron/count/divider)
   instead of Accordion's button/slot structure. */
.group-content {
	display: grid;
	grid-template-rows: 0fr;
	transition: grid-template-rows 0.3s ease-in-out;
}

@media (prefers-reduced-motion) {
	.group-content {
		transition: none !important;
	}
}

.group-content.open {
	grid-template-rows: 1fr;
}

.group-content > div {
	overflow: hidden;
}
</style>
