import type { Ref } from 'vue'
import { computed, ref, watch, watchEffect } from 'vue'

export interface ScrollViewportOptions {
	onScroll?: () => void
	onResize?: () => void
}

export interface VirtualScrollOptions {
	itemHeight: number
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

export function useVirtualScroll<T>(items: Ref<T[]>, options: VirtualScrollOptions) {
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

	const totalHeight = computed(() => items.value.length * itemHeight)

	const visibleRange = computed(() => {
		if (enabled && !enabled.value) {
			return { start: 0, end: items.value.length }
		}

		if (!listContainer.value || !scrollContainer.value) {
			return { start: 0, end: Math.min(items.value.length, initialItemCount) }
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

	const visibleTop = computed(() =>
		enabled && !enabled.value ? 0 : visibleRange.value.start * itemHeight,
	)

	const visibleItems = computed(() =>
		items.value.slice(visibleRange.value.start, visibleRange.value.end),
	)

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
		resetScrollState,
		syncScrollState,
	}
}
