<script lang="ts"></script>

<script setup lang="ts">
import { KeyboardSensor, PointerSensor, useDraggable } from '@dnd-kit/vue'
import { CheckIcon, ClipboardCopyIcon, EditIcon, MoreHorizontalIcon } from '@modrinth/assets'
import { defineMessages, IconButton, useFormatDateTime, useVIntl } from '@modrinth/ui'
import { computed, onMounted, ref, watch } from 'vue'

import { get_screenshot_thumbnail, type InstanceScreenshot } from '@/helpers/instance'
const loadedScreenshotUrls = new Set<string>()

// Modrinth Studios addition: see the doc comment on `get_screenshot_thumbnail`
// in operations.rs for why the grid uses a small generated-and-cached
// thumbnail instead of decoding every card's full-resolution original. This
// module-level map remembers what each screenshot resolved to (a thumbnail
// URL, or `null` if none could be generated) so scrolling a card back into
// view — it gets unmounted/remounted by the grid's virtualization — reuses
// that result instantly instead of re-awaiting the Tauri call every time.
const resolvedThumbnailUrls = new Map<string, string | null>()

// Modrinth Studios addition: caps how many `get_screenshot_thumbnail` calls
// can be in flight across the whole grid at once. Without this, scrolling
// fast into content that's never been viewed before mounts a burst of new
// cards in the same frame, and every one of them fires its own IPC call +
// Rust-side decode/resize/encode + a brand-new JPEG the browser has to
// decode, all landing at once. That's the same "unbounded concurrency looks
// like a freeze" lesson this codebase already learned for backend
// spawn_blocking storms (see `list_replays`) and frontend row-mounting (see
// the original Replays virtualization fix) — just one layer further out:
// each individual fetch is cheap and off the main thread, but enough of them
// resolving in the same short window is what showed up, on a frame-by-frame
// recording, as the scroll pausing on the same content for a beat and then
// snapping forward — worse scrolling down into new cards than back up over
// already-resolved ones, since revisited cards skip all of this entirely.
const THUMBNAIL_FETCH_CONCURRENCY = 4
let activeThumbnailFetches = 0
const thumbnailFetchQueue: Array<() => void> = []

function acquireThumbnailFetchSlot(): Promise<void> {
	if (activeThumbnailFetches < THUMBNAIL_FETCH_CONCURRENCY) {
		activeThumbnailFetches++
		return Promise.resolve()
	}
	return new Promise((resolve) => {
		thumbnailFetchQueue.push(() => {
			activeThumbnailFetches++
			resolve()
		})
	})
}

function releaseThumbnailFetchSlot() {
	activeThumbnailFetches--
	thumbnailFetchQueue.shift()?.()
}

function screenshotThumbnailCacheKey(screenshot: InstanceScreenshot): string {
	return `${screenshot.instance_id}:${screenshot.file_name}`
}

const props = defineProps<{
	screenshot: InstanceScreenshot
	selectionKey: string
	selected: boolean
	selectionActive: boolean
	activeDragged: boolean
	canDrag: boolean
	showInstanceName: boolean
	highlighted: boolean
	copied: boolean
	// Modrinth Studios addition: the card's height in px, computed once in
	// index.vue (`screenshotCardHeight`) from the grid's own measured width
	// and column count. Row virtualization in group.vue (`renderedScreenshots`
	// / `virtualGridTop`) assumes every row is exactly `screenshotRowHeight`
	// (= this value + the grid gap) tall to work out which rows are visible
	// and where to place them. Before this prop existed, the card just used
	// Tailwind's `aspect-video` and let the browser derive its height from
	// whatever width the CSS grid actually gave it — which is a SEPARATE
	// computation from the one in index.vue, and any sub-pixel difference
	// between the two (grid gap rounding, scrollbar width, etc.) meant the
	// virtualizer's idea of "row height" was very slightly wrong. That error
	// is invisible on any one row, but it accumulates by that same tiny
	// amount every single row, and every time a scroll crosses a row
	// boundary the visible window gets re-sliced and repositioned at
	// `firstRow * (assumed) screenshotRowHeight` — a value that drifts
	// further from where the browser had actually laid out that row the
	// deeper you'd scrolled. That drift, snapping back into alignment at
	// each row boundary, is what looked like "the images move a row down
	// when I scroll." Setting height explicitly here, from the exact same
	// number the virtualizer itself uses, makes the two impossible to
	// disagree.
	cardHeight: number
}>()

const emit = defineEmits<{
	(e: 'activate', event: MouseEvent | KeyboardEvent): void
	(e: 'copy' | 'edit' | 'toggle-selection'): void
	(e: 'more', event: MouseEvent): void
}>()

const card = ref<HTMLElement>()
const image = ref<HTMLImageElement>()

// `undefined` while the thumbnail lookup is in flight (nothing is loaded
// yet — the `<img>` has no `src` at all during this window, so the browser
// never starts fetching the full-resolution original only to immediately
// swap it out again a moment later), `null` once resolved to "no thumbnail
// available" (falls back to the full-resolution `url`), a thumbnail URL
// once resolved successfully.
const thumbnailUrl = ref<string | null | undefined>(
	resolvedThumbnailUrls.get(screenshotThumbnailCacheKey(props.screenshot)),
)
const displaySrc = computed(() => {
	if (thumbnailUrl.value === undefined) return undefined
	return thumbnailUrl.value ?? props.screenshot.url
})
const loaded = ref(displaySrc.value !== undefined && loadedScreenshotUrls.has(displaySrc.value))
const { formatMessage } = useVIntl()
const formatTime = useFormatDateTime({ dateStyle: 'medium', timeStyle: 'short' })
const messages = defineMessages({
	select: { id: 'app.screenshots.select', defaultMessage: 'Select {name}' },
	deselect: { id: 'app.screenshots.deselect', defaultMessage: 'Deselect {name}' },
	copy: { id: 'app.screenshots.copy', defaultMessage: 'Copy image' },
	copied: { id: 'app.screenshots.copied', defaultMessage: 'Copied' },
	edit: { id: 'app.screenshots.edit', defaultMessage: 'Edit screenshot' },
	moreActions: { id: 'app.screenshots.more-actions', defaultMessage: 'More actions' },
})

const sensors = [
	PointerSensor.configure({
		preventActivation: () => false,
	}),
	KeyboardSensor,
]

useDraggable({
	id: computed(() => `screenshot:${props.selectionKey}`),
	element: card,
	disabled: computed(() => !props.canDrag),
	sensors,
	data: computed(() => ({
		selectionKey: props.selectionKey,
		instanceId: props.screenshot.instance_id,
	})),
})

function activate(event: MouseEvent | KeyboardEvent) {
	if (event instanceof KeyboardEvent) {
		if (event.target !== event.currentTarget || (event.key !== 'Enter' && event.key !== ' ')) {
			return
		}
		event.preventDefault()
	}
	emit('activate', event)
}

function markImageLoaded() {
	if (displaySrc.value) loadedScreenshotUrls.add(displaySrc.value)
	loaded.value = true
}

async function loadThumbnail(screenshot: InstanceScreenshot) {
	const key = screenshotThumbnailCacheKey(screenshot)
	if (resolvedThumbnailUrls.has(key)) {
		thumbnailUrl.value = resolvedThumbnailUrls.get(key) ?? null
		return
	}

	await acquireThumbnailFetchSlot()
	let result: string | null
	try {
		result = await get_screenshot_thumbnail({
			instance_id: screenshot.instance_id,
			file_name: screenshot.file_name,
		}).catch((error) => {
			// Modrinth Studios: don't swallow this silently — if the Tauri
			// command itself is failing (most likely because a running `tauri
			// dev` session hasn't picked up/recompiled this command yet), every
			// card falls back to the full-resolution image forever and the grid
			// is exactly as laggy as before this feature existed, with no signal
			// in the UI that anything's wrong. Logging it means that's visible
			// in devtools instead of just looking like "the fix didn't work."
			console.error('Failed to fetch screenshot thumbnail, falling back to full-resolution image', {
				key: { instance_id: screenshot.instance_id, file_name: screenshot.file_name },
				error,
			})
			return null
		})
	} finally {
		releaseThumbnailFetchSlot()
	}

	resolvedThumbnailUrls.set(key, result)
	// Only apply the result if we're still looking at the same screenshot —
	// a fast-scrolling, recycled card could have moved on to a different one
	// while this call was in flight.
	if (screenshotThumbnailCacheKey(props.screenshot) === key) {
		thumbnailUrl.value = result
	}
}

onMounted(() => {
	if (thumbnailUrl.value === undefined) void loadThumbnail(props.screenshot)
	if (image.value?.complete && image.value.naturalWidth > 0) markImageLoaded()
})

watch(
	() => props.screenshot,
	(screenshot) => {
		const key = screenshotThumbnailCacheKey(screenshot)
		thumbnailUrl.value = resolvedThumbnailUrls.get(key)
		if (thumbnailUrl.value === undefined) void loadThumbnail(screenshot)
	},
)

watch(displaySrc, (src) => {
	loaded.value = src !== undefined && loadedScreenshotUrls.has(src)
})
</script>

<template>
	<article
		ref="card"
		role="button"
		tabindex="0"
		class="group relative min-w-0 cursor-pointer overflow-hidden rounded-xl border border-solid border-surface-5 bg-surface-2 p-0 text-left shadow-sm transition-[filter] hover:brightness-110 focus-visible:outline focus-visible:outline-2 focus-visible:outline-brand"
		:class="{
			'!border-contrast brightness-110': selected,
			'!border-brand ring-2 ring-brand animate-pulse': highlighted,
			'opacity-50': activeDragged,
			'cursor-grab active:cursor-grabbing': canDrag,
		}"
		:style="{ height: `${cardHeight}px` }"
		data-screenshot-card
		:data-screenshot-id="screenshot.id"
		:data-selection-key="selectionKey"
		:aria-label="
			selectionActive
				? formatMessage(selected ? messages.deselect : messages.select, {
						name: screenshot.file_name,
					})
				: screenshot.file_name
		"
		:aria-pressed="selectionActive ? selected : undefined"
		@click="activate"
		@contextmenu.prevent.stop="emit('more', $event)"
		@keydown="activate"
	>
		<button
			type="button"
			class="selection-button group/selection absolute right-0.5 top-0 z-[2] flex size-[50px] cursor-pointer items-start justify-center border-0 bg-transparent p-0 pt-4"
			:aria-label="
				formatMessage(selected ? messages.deselect : messages.select, {
					name: screenshot.file_name,
				})
			"
			:aria-pressed="selected"
			@click.stop="emit('toggle-selection')"
		>
			<span
				class="relative flex size-6 items-center justify-center rounded-full opacity-0 transition-opacity duration-200 ease-out group-hover:opacity-100 group-focus-within:opacity-100 group-hover/selection:brightness-125"
				:class="
					selected ? 'border-0 !opacity-100' : 'border-2 border-solid border-primary bg-transparent'
				"
			>
				<span v-if="selected" class="absolute inset-0 rounded-full bg-contrast" />
				<CheckIcon v-if="selected" class="relative size-4 invert [stroke-width:3]" />
			</span>
		</button>
		<!--
			Modrinth Studios: `screenshot-thumbnail-skeleton` (see
			studio-overrides.css) opts this out of the app's frosted-glass
			backdrop-filter blur. Same bug ContentCardTable already hit and
			fixed for its own rows: a virtualized grid can have a couple dozen
			of these mounted/animating at once (every card still waiting on its
			thumbnail), and blurring that many simultaneously-pulsing elements
			overwhelmed the webview's compositor — exactly the "corrupted color
			blocks, heavy flashing while scrolling" ContentCardTable's fix
			describes, just not caught here since this skeleton used a
			surface class that fix never excluded.
		-->
		<div v-if="!loaded" class="absolute inset-0 animate-pulse bg-surface-3 screenshot-thumbnail-skeleton" />
		<img
			v-if="displaySrc"
			ref="image"
			:src="displaySrc"
			:alt="screenshot.file_name"
			loading="lazy"
			decoding="async"
			draggable="false"
			class="h-full w-full object-cover transition duration-200"
			:class="loaded ? 'opacity-100' : 'opacity-0'"
			@load="markImageLoaded"
		/>
		<div
			class="absolute inset-x-0 bottom-0 flex items-end justify-between gap-2 bg-gradient-to-t from-surface-1 to-transparent p-3 pt-[120px] text-contrast opacity-0 transition-opacity duration-200 group-hover:opacity-100 group-focus-within:opacity-100"
		>
			<div class="min-w-0">
				<div v-tooltip="screenshot.file_name" class="truncate text-sm font-semibold">
					{{ screenshot.file_name }}
				</div>
				<div class="truncate text-xs text-secondary">
					{{ showInstanceName ? screenshot.instance_name : formatTime(screenshot.created_at) }}
				</div>
			</div>
			<div
				v-if="!selectionActive"
				class="flex shrink-0 translate-y-1 gap-1 opacity-0 transition group-hover:translate-y-0 group-hover:opacity-100 group-focus-within:translate-y-0 group-focus-within:opacity-100"
				@click.stop
			>
				<IconButton
					v-tooltip="formatMessage(messages.edit)"
					:label="formatMessage(messages.edit)"
					type="quiet"
					class="bg-surface-2 text-contrast hover:bg-surface-3"
					@click="emit('edit')"
				>
					<EditIcon />
				</IconButton>
				<IconButton
					v-tooltip="formatMessage(copied ? messages.copied : messages.copy)"
					:label="formatMessage(copied ? messages.copied : messages.copy)"
					type="quiet"
					class="bg-surface-2 text-contrast hover:bg-surface-3"
					@click="emit('copy')"
				>
					<CheckIcon v-if="copied" class="text-green" />
					<ClipboardCopyIcon v-else />
				</IconButton>
				<IconButton
					v-tooltip="formatMessage(messages.moreActions)"
					:label="formatMessage(messages.moreActions)"
					type="quiet"
					class="bg-surface-2 text-contrast hover:bg-surface-3"
					@click="emit('more', $event)"
				>
					<MoreHorizontalIcon />
				</IconButton>
			</div>
		</div>
	</article>
</template>
