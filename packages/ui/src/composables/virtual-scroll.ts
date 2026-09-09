import type { Ref } from 'vue'
import { computed, ref, watch, watchEffect } from 'vue'

export interface ScrollViewportOptions {
	onScroll?: () => void
	onResize?: () => void
}

export interface VirtualScrollOptions<T = unknown> {
	// Modrinth Studios addition: `itemHeight` may also be a per-item function
	// so a list can mix item types of different heights (e.g. the Replays
	// tab's compact group headers alongside its taller replay cards) without
	// every item being forced into the same fixed slot. The numeric form is
	// unchanged and stays the fast path for every existing (uniform-height)
	// caller.
	itemHeight: number | ((item: T, index: number) => number)
	bufferSize?: number
	initialItemCount?: number
	enabled?: Ref<boolean>
	onNearEnd?: () => void
	nearEndThreshold?: number
}

export function findScrollableAncestor(element: HTMLElement | null): HTMLElement | Window {
	if (!element) return window

	let current: HTMLElement | null = element.parentElement
	while (current) {
		const { overflowY } = getComputedStyle(current)
		if (overflowY === 'auto' || overflowY === 'scroll') {
			return current
		}
		current = current.parentElement
	}
	return window
}

export function getScrollTop(container: HTMLElement | Window): number {
	return container instanceof Window ? window.scrollY : container.scrollTop
}

export function getViewportHeight(container: HTMLElement | Window): number {
	return container instanceof Window ? window.innerHeight : container.clientHeight
}

export function useScrollViewport(options: ScrollViewportOptions = {}) {
	const listContainer = ref<HTMLElement | null>(null)
	const scrollContainer = ref<HTMLElement | Window | null>(null)
	const scrollTop = ref(0)
	const viewportHeight = ref(0)
	const containerOffset = ref(0)
	const relativeScrollTop = computed(() => Math.max(0, scrollTop.value - containerOffset.value))

	function updateContainerOffset() {
		const listEl = listContainer.value
		const container = scrollContainer.value
		if (!listEl || !container) return

		if (container instanceof Window) {
			containerOffset.value = listEl.getBoundingClientRect().top + window.scrollY
		} else {
			const listRect = listEl.getBoundingClientRect()
			const containerRect = container.getBoundingClientRect()
			containerOffset.value = listRect.top - containerRect.top + container.scrollTop
		}
	}

	function syncScrollState() {
		const listEl = listContainer.value
		if (!listEl) return

		const container = findScrollableAncestor(listEl)
		scrollContainer.value = container
		scrollTop.value = getScrollTop(container)
		viewportHeight.value = getViewportHeight(container)
		updateContainerOffset()

		// Modrinth Studios addition: this is the real scroll-anchoring fix —
		// setting `overflow-anchor: none` on a virtualized list's own content
		// div (as screenshots-page/index.vue did on its own) doesn't disable
		// scroll anchoring for the page, because that div isn't the scrolling
		// element; it's just tall content sitting inside one (found here via
		// `findScrollableAncestor`, often several components up — e.g.
		// `pages/instance/layout.vue`'s `overflow-y-auto` wrapper). The
		// browser's native scroll anchoring is still fully active on that real
		// container, and it does exactly what it's designed to do: nudge
		// `scrollTop` to compensate whenever content mutates above the fold —
		// which is continuously true here, since virtualization means rows and
		// groups are mounting/unmounting above the viewport on every scroll.
		// That compensation is what looked like "the scroll position itself
		// snapping/jerking" and why it was worse scrolling down: anchoring
		// corrects for content appearing above the anchor point, which is more
		// active while scrolling toward not-yet-rendered content. Disabling it
		// on the actual scroll container (not the inner content div) is the
		// standard fix for this well-known class of virtualized-list jank.
		if (!(container instanceof Window)) {
			container.style.overflowAnchor = 'none'
		}
	}

	function resetScrollState() {
		scrollTop.value = 0
		viewportHeight.value = 0
		containerOffset.value = 0
	}

	// Modrinth Studios addition: the native `scroll` event can fire far more
	// often than once per animation frame (many times per frame during a fast
	// trackpad/wheel fling, especially on Windows) — and every firing used to
	// synchronously write two refs (`scrollTop`, `containerOffset`, the
	// latter via a layout-forcing `getBoundingClientRect()` read) and run
	// `onScroll` (which recomputes the virtualizer's visible range). Vue
	// batches the *reactive re-render* from ref writes into a microtask, but
	// it doesn't stop this handler's own synchronous work — the layout read
	// chief among it — from running many more times than the screen can ever
	// present, which is exactly what shows up as stutter during a scroll
	// gesture regardless of how fast the rows themselves are to render.
	// Coalescing to at most once per animation frame (last-value-wins if
	// several `scroll` events land in the same frame) matches the browser's
	// own paint cadence and is the standard fix for this class of jank.
	let pendingScrollFrame: number | null = null

	function flushScroll() {
		pendingScrollFrame = null
		if (scrollContainer.value) {
			scrollTop.value = getScrollTop(scrollContainer.value)
			updateContainerOffset()
		}

		options.onScroll?.()
	}

	function handleScroll() {
		if (pendingScrollFrame !== null) return
		pendingScrollFrame = window.requestAnimationFrame(flushScroll)
	}

	function handleResize() {
		syncScrollState()
		options.onResize?.()
	}

	watchEffect((onCleanup) => {
		if (typeof window === 'undefined') return

		const listEl = listContainer.value
		if (!listEl) return

		const container = findScrollableAncestor(listEl)
		scrollContainer.value = container
		syncScrollState()

		container.addEventListener('scroll', handleScroll, { passive: true })
		window.addEventListener('resize', handleResize, { passive: true })

		let resizeObserver: ResizeObserver | undefined
		if (!(container instanceof Window)) {
			resizeObserver = new ResizeObserver(() => {
				syncScrollState()
			})
			resizeObserver.observe(container)
		}

		onCleanup(() => {
			container.removeEventListener('scroll', handleScroll)
			window.removeEventListener('resize', handleResize)
			resizeObserver?.disconnect()
			if (pendingScrollFrame !== null) {
				window.cancelAnimationFrame(pendingScrollFrame)
				pendingScrollFrame = null
			}
		})
	})

	return {
		resetScrollState,
		containerOffset,
		listContainer,
		relativeScrollTop,
		scrollContainer,
		scrollTop,
		syncScrollState,
		updateContainerOffset,
		viewportHeight,
	}
}

export function useVirtualScroll<T>(items: Ref<T[]>, options: VirtualScrollOptions<T>) {
	const {
		itemHeight,
		bufferSize = 5,
		initialItemCount = 20,
		enabled,
		onNearEnd,
		nearEndThreshold = 0.2,
	} = options

	const {
		listContainer,
		relativeScrollTop,
		resetScrollState,
		scrollContainer,
		syncScrollState,
		viewportHeight,
	} = useScrollViewport({
		onScroll: checkNearEnd,
	})

	// Modrinth Studios addition: variable-height support. `offsets` holds the
	// cumulative top offset of every item plus one trailing entry for the
	// total height (so `offsets[i+1] - offsets[i]` is item i's own height);
	// it's only computed at all when `itemHeight` is a function, so the
	// original fixed-height callers (every one before this addition) pay
	// nothing extra — they keep the plain `index * itemHeight` arithmetic
	// below.
	const offsets = computed<number[] | null>(() => {
		if (typeof itemHeight !== 'function') return null
		const heights = items.value
		const result = new Array<number>(heights.length + 1)
		result[0] = 0
		for (let i = 0; i < heights.length; i++) {
			result[i + 1] = result[i] + itemHeight(heights[i], i)
		}
		return result
	})

	function heightOf(index: number): number {
		if (typeof itemHeight === 'function') return itemHeight(items.value[index], index)
		return itemHeight
	}

	// Largest index `i` such that `offsets[i] <= target` (offsets is
	// non-decreasing since heights are always >= 0).
	function findOffsetIndex(target: number): number {
		const off = offsets.value!
		let lo = 0
		let hi = off.length - 1
		while (lo < hi) {
			const mid = (lo + hi + 1) >> 1
			if (off[mid] <= target) lo = mid
			else hi = mid - 1
		}
		return Math.min(lo, items.value.length - 1)
	}

	const totalHeight = computed(() => {
		if (offsets.value) return offsets.value[offsets.value.length - 1]
		return items.value.length * (itemHeight as number)
	})

	const visibleRange = computed(() => {
		if (enabled && !enabled.value) {
			return { start: 0, end: items.value.length }
		}

		if (!listContainer.value || !scrollContainer.value) {
			return { start: 0, end: Math.min(items.value.length, initialItemCount) }
		}

		if (offsets.value) {
			if (items.value.length === 0) return { start: 0, end: 0 }
			const off = offsets.value
			const start = findOffsetIndex(relativeScrollTop.value)
			const viewportBottom = relativeScrollTop.value + viewportHeight.value
			let end = start
			while (end < items.value.length && off[end] < viewportBottom) end++

			const rangeStart = Math.max(0, start - bufferSize)
			const rangeEnd = Math.min(items.value.length, end + bufferSize)
			return { start: rangeStart, end: rangeEnd }
		}

		const start = Math.floor(relativeScrollTop.value / itemHeight)
		const visibleCount = Math.ceil(viewportHeight.value / itemHeight)
		const rangeSize = visibleCount + bufferSize * 2

		const rangeStart = Math.min(
			Math.max(0, start - bufferSize),
			Math.max(0, items.value.length - rangeSize),
		)
		const rangeEnd = Math.min(items.value.length, rangeStart + rangeSize)

		return {
			start: rangeStart,
			end: rangeEnd,
		}
	})

	const visibleTop = computed(() => {
		if (enabled && !enabled.value) return 0
		if (offsets.value) return offsets.value[visibleRange.value.start]
		return visibleRange.value.start * (itemHeight as number)
	})

	const visibleItems = computed(() =>
		items.value.slice(visibleRange.value.start, visibleRange.value.end),
	)

	// Modrinth Studios addition: per-visible-item offset (relative to
	// `visibleTop`) and height, for variable-height lists — a fixed-height
	// list can keep deriving position from `index * itemHeight` directly, so
	// this is only meaningful (non-null entries) when `itemHeight` is a
	// function.
	const visibleItemLayout = computed(() => {
		const { start, end } = visibleRange.value
		const top = visibleTop.value
		const result: { offset: number; height: number }[] = []
		for (let i = start; i < end; i++) {
			const itemTop = offsets.value ? offsets.value[i] - top : (i - start) * (itemHeight as number)
			result.push({ offset: itemTop, height: heightOf(i) })
		}
		return result
	})

	function checkNearEnd() {
		if (!onNearEnd || !listContainer.value || !viewportHeight.value) return

		const containerBottom = listContainer.value.getBoundingClientRect().bottom
		const remainingScroll = containerBottom - viewportHeight.value

		if (remainingScroll < viewportHeight.value * nearEndThreshold) {
			onNearEnd()
		}
	}

	watch(items, () => {
		syncScrollState()
	})

	return {
		listContainer,
		totalHeight,
		visibleRange,
		visibleTop,
		visibleItems,
		visibleItemLayout,
		resetScrollState,
		syncScrollState,
	}
}
