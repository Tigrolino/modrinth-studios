<script setup lang="ts">
/**
 * Modrinth Studios: small dependency-free SVG chart for the analytics page.
 * Line/area or bar series over a shared x axis, with a hover read-out.
 */
import { computed, ref } from 'vue'

export type ChartSeries = {
	name: string
	color: string
	values: (number | null)[]
}

const props = withDefaults(
	defineProps<{
		labels: number[]
		series: ChartSeries[]
		type?: 'line' | 'bar'
		height?: number
		formatValue?: (value: number) => string
		formatLabel?: (t: number) => string
		formatAxisLabel?: (t: number) => string
		min?: number
		max?: number
		stacked?: boolean
		/** Whole-number axis (counts like players). */
		integer?: boolean
	}>(),
	{
		type: 'line',
		height: 180,
		formatValue: (v: number) => String(Math.round(v * 10) / 10),
		formatLabel: (t: number) => new Date(t).toLocaleString(),
		formatAxisLabel: (t: number) =>
			new Date(t).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
		min: undefined,
		max: undefined,
		stacked: false,
		integer: false,
	},
)

const W = 1000
const PAD = { top: 10, right: 8, bottom: 22, left: 48 }

const plotW = W - PAD.left - PAD.right
const plotH = computed(() => props.height - PAD.top - PAD.bottom)

const extent = computed(() => {
	let lo = props.min ?? Infinity
	let hi = props.max ?? -Infinity
	const n = props.labels.length
	for (let i = 0; i < n; i++) {
		let stack = 0
		for (const s of props.series) {
			const v = s.values[i]
			if (v === null || v === undefined || Number.isNaN(v)) continue
			if (props.stacked) stack += v
			else {
				if (props.min === undefined) lo = Math.min(lo, v)
				if (props.max === undefined) hi = Math.max(hi, v)
			}
		}
		if (props.stacked && props.max === undefined) hi = Math.max(hi, stack)
	}
	if (!Number.isFinite(lo)) lo = 0
	if (!Number.isFinite(hi)) hi = 1
	if (props.min === undefined) lo = Math.min(0, lo)
	if (hi <= lo) hi = lo + (props.integer ? 4 : 1)
	if (props.max === undefined) {
		// Round the top up to a "nice" step so ticks are readable (0, 2, 4…).
		const step = niceStep((hi - lo) / 4, props.integer)
		hi = lo + Math.ceil((hi - lo) / step) * step
		if (hi - lo < step * 4) hi = lo + step * 4
	}
	return { lo, hi }
})

function niceStep(raw: number, integer: boolean) {
	if (raw <= 0) return integer ? 1 : 1
	const pow = 10 ** Math.floor(Math.log10(raw))
	const n = raw / pow
	const step = (n <= 1 ? 1 : n <= 2 ? 2 : n <= 2.5 ? 2.5 : n <= 5 ? 5 : 10) * pow
	return integer ? Math.max(1, Math.ceil(step)) : step
}

const x = (i: number) => {
	const n = props.labels.length
	if (props.type === 'bar') return PAD.left + ((i + 0.5) / Math.max(1, n)) * plotW
	return PAD.left + (n <= 1 ? plotW / 2 : (i / (n - 1)) * plotW)
}
const y = (v: number) =>
	PAD.top + plotH.value - ((v - extent.value.lo) / (extent.value.hi - extent.value.lo)) * plotH.value

const gridLines = computed(() => {
	const lines = []
	for (let k = 0; k <= 4; k++) {
		const v = extent.value.lo + ((extent.value.hi - extent.value.lo) * k) / 4
		lines.push({ y: y(v), label: props.formatValue(v) })
	}
	return lines
})

const axisLabels = computed(() => {
	const n = props.labels.length
	if (!n) return []
	const count = Math.min(6, n)
	const out = []
	for (let k = 0; k < count; k++) {
		const i = count === 1 ? 0 : Math.round((k / (count - 1)) * (n - 1))
		out.push({ x: x(i), label: props.formatAxisLabel(props.labels[i]) })
	}
	return out
})

/** Line paths split at gaps (null values). */
const linePaths = computed(() =>
	props.series.map((s) => {
		const segments: string[] = []
		const areas: string[] = []
		const dots: [number, number][] = []
		let current: [number, number][] = []
		const push = () => {
			// Lone points (data between gaps) would be invisible as a path.
			if (current.length === 1) dots.push(current[0])
			if (current.length) {
				segments.push(current.map(([px, py], j) => `${j ? 'L' : 'M'}${px},${py}`).join(''))
				const base = y(Math.max(extent.value.lo, 0))
				areas.push(
					`M${current[0][0]},${base}` +
						current.map(([px, py]) => `L${px},${py}`).join('') +
						`L${current[current.length - 1][0]},${base}Z`,
				)
			}
			current = []
		}
		s.values.forEach((v, i) => {
			if (v === null || v === undefined || Number.isNaN(v)) push()
			else current.push([x(i), y(v)])
		})
		push()
		return { color: s.color, line: segments, area: areas, dots }
	}),
)

const bars = computed(() => {
	const n = props.labels.length
	const slot = plotW / Math.max(1, n)
	const groups = props.stacked ? 1 : props.series.length
	const bw = Math.max(1, (slot * 0.7) / groups)
	const out: { x: number; y: number; w: number; h: number; color: string }[] = []
	for (let i = 0; i < n; i++) {
		let stackTop = 0
		props.series.forEach((s, si) => {
			const v = s.values[i]
			if (!v) return
			const left = PAD.left + i * slot + slot * 0.15 + (props.stacked ? 0 : si * bw)
			const top = props.stacked ? y(stackTop + v) : y(v)
			const bottom = props.stacked ? y(stackTop) : y(Math.max(0, extent.value.lo))
			out.push({ x: left, y: top, w: bw, h: Math.max(0, bottom - top), color: s.color })
			stackTop += v
		})
	}
	return out
})

/** Stretches with no data between recorded points (Studio wasn't open), shaded on the chart. */
const gaps = computed(() => {
	if (props.type !== 'line') return []
	const n = props.labels.length
	const has = (i: number) => props.series.some((s) => s.values[i] !== null && s.values[i] !== undefined)
	const out: { x: number; w: number }[] = []
	let start = -1
	let seen = false
	for (let i = 0; i < n; i++) {
		if (has(i)) {
			if (start !== -1 && seen) {
				const x0 = x(start - 1)
				out.push({ x: x0, w: x(i) - x0 })
			}
			start = -1
			seen = true
		} else if (start === -1) start = i
	}
	return out
})

const isEmpty = computed(() =>
	props.series.every((s) => s.values.every((v) => v === null || v === undefined)),
)

const hover = ref<number | null>(null)
const svgRef = ref<SVGSVGElement>()

function onMove(event: MouseEvent) {
	const svg = svgRef.value
	if (!svg || !props.labels.length) return
	const rect = svg.getBoundingClientRect()
	const px = ((event.clientX - rect.left) / rect.width) * W
	const n = props.labels.length
	let i: number
	if (props.type === 'bar') i = Math.floor(((px - PAD.left) / plotW) * n)
	else i = Math.round(((px - PAD.left) / plotW) * (n - 1))
	hover.value = Math.min(n - 1, Math.max(0, i))
}

const tooltipLeft = computed(() =>
	hover.value === null ? 0 : (x(hover.value) / W) * 100,
)
</script>

<template>
	<div class="relative w-full">
		<svg
			ref="svgRef"
			:viewBox="`0 0 ${W} ${height}`"
			preserveAspectRatio="none"
			class="block w-full"
			:style="{ height: `${height}px` }"
			@mousemove="onMove"
			@mouseleave="hover = null"
		>
			<g>
				<line
					v-for="(g, i) in gridLines"
					:key="i"
					:x1="PAD.left"
					:x2="W - PAD.right"
					:y1="g.y"
					:y2="g.y"
					stroke="var(--color-divider, rgba(127,127,127,0.18))"
					stroke-width="1"
					vector-effect="non-scaling-stroke"
				/>
			</g>
			<rect
				v-for="(g, gi) in gaps"
				:key="`gap${gi}`"
				:x="g.x"
				:y="PAD.top"
				:width="g.w"
				:height="plotH"
				fill="var(--color-button-bg)"
				opacity="0.45"
			>
				<title>Not recorded (Studio wasn't open on this server)</title>
			</rect>
			<template v-if="type === 'line'">
				<g v-for="(p, si) in linePaths" :key="si">
					<path
						v-for="(a, ai) in p.area"
						:key="`a${ai}`"
						:d="a"
						:fill="p.color"
						fill-opacity="0.12"
					/>
					<circle
						v-for="(d, di) in p.dots"
						:key="`d${di}`"
						:cx="d[0]"
						:cy="d[1]"
						r="3"
						:fill="p.color"
						vector-effect="non-scaling-stroke"
					/>
					<path
						v-for="(l, li) in p.line"
						:key="`l${li}`"
						:d="l"
						fill="none"
						:stroke="p.color"
						stroke-width="2"
						stroke-linejoin="round"
						vector-effect="non-scaling-stroke"
					/>
				</g>
			</template>
			<template v-else>
				<rect
					v-for="(b, bi) in bars"
					:key="bi"
					:x="b.x"
					:y="b.y"
					:width="b.w"
					:height="b.h"
					:fill="b.color"
					rx="2"
				/>
			</template>
			<line
				v-if="hover !== null"
				:x1="x(hover)"
				:x2="x(hover)"
				:y1="PAD.top"
				:y2="PAD.top + plotH"
				stroke="var(--color-secondary, #888)"
				stroke-dasharray="3 3"
				vector-effect="non-scaling-stroke"
			/>
		</svg>
		<!-- axis labels rendered as HTML so text isn't stretched -->
		<div class="pointer-events-none absolute inset-0">
			<span
				v-for="(g, i) in gridLines"
				:key="`g${i}`"
				class="absolute left-0 -translate-y-1/2 text-[11px] text-secondary"
				:style="{ top: `${g.y}px` }"
			>
				{{ g.label }}
			</span>
			<span
				v-for="(a, i) in axisLabels"
				:key="`x${i}`"
				class="absolute bottom-0 -translate-x-1/2 whitespace-nowrap text-[11px] text-secondary"
				:style="{ left: `${(a.x / W) * 100}%` }"
			>
				{{ a.label }}
			</span>
		</div>
		<div
			v-if="isEmpty"
			class="pointer-events-none absolute inset-0 flex items-center justify-center text-sm text-secondary"
		>
			No data recorded yet
		</div>
		<div
			v-if="hover !== null && !isEmpty"
			class="pointer-events-none absolute top-0 z-10 rounded-xl border border-solid border-surface-5 bg-bg-raised px-3 py-2 text-sm shadow-lg"
			:style="{
				left: `${tooltipLeft}%`,
				transform: tooltipLeft > 60 ? 'translateX(calc(-100% - 12px))' : 'translateX(12px)',
			}"
		>
			<div class="mb-1 font-semibold text-contrast">{{ formatLabel(labels[hover]) }}</div>
			<div v-for="s in series" :key="s.name" class="flex items-center gap-2 whitespace-nowrap">
				<span class="inline-block size-2 rounded-full" :style="{ background: s.color }" />
				<span class="text-secondary">{{ s.name }}</span>
				<span class="ml-auto pl-3 font-semibold text-contrast">
					{{
						s.values[hover] === null || s.values[hover] === undefined
							? '—'
							: formatValue(s.values[hover] as number)
					}}
				</span>
			</div>
		</div>
	</div>
</template>
