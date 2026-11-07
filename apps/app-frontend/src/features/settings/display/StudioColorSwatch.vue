<!--
	Modrinth Studios addition: new file. A fully custom color picker (swatch
	trigger + popover with a saturation/value area, hue slider, and hex
	input) styled to match Modrinth's UI — no native browser <input
	type="color"> picker involved anywhere, since that was the whole
	complaint: it looks like raw browser/OS UI, not part of the app.
-->
<script setup lang="ts">
import { onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'

const props = defineProps<{
	modelValue: string
	label?: string
}>()

const emit = defineEmits<{
	'update:modelValue': [string]
}>()

const open = ref(false)
const rootEl = ref<HTMLElement>()
const svArea = ref<HTMLElement>()
const hueTrack = ref<HTMLElement>()

// Modrinth Studios addition: the popover used to always be `left: 0` (grow
// rightward) and `top: calc(100% + 8px)` (grow downward) from the trigger,
// with no awareness of the viewport at all — so a swatch sitting near the
// right edge of the Settings window (the "Background" gradient's "To" swatch
// especially, but really any of them depending on window width) would have
// its 220px-wide popover run straight off the edge and get clipped. This
// measures the trigger's actual position right when it opens and flips the
// popover to grow from whichever side/direction actually has room, instead
// of assuming there's always space to the right and below.
const POPOVER_WIDTH = 220
const POPOVER_HEIGHT_ESTIMATE = 260
const POPOVER_MARGIN = 12
const alignRight = ref(false)
const placeAbove = ref(false)

function updatePopoverPlacement() {
	const el = rootEl.value
	if (!el) return
	const rect = el.getBoundingClientRect()

	// Modrinth Studios addition: checking against the *window* was the bug —
	// this lives inside the Settings modal's own scrollable panel
	// (`.modal-body`), which is narrower than the window itself (it's
	// centered with its own max-width). The popover could still be fully
	// within the window's bounds while overflowing that narrower panel,
	// which doesn't "clip" so much as make the panel itself gain a
	// horizontal scrollbar to accommodate it. Bounding against the panel
	// instead of the window is what actually matches what the user sees.
	const boundsEl = (el.closest('.modal-body') as HTMLElement | null) ?? document.documentElement
	const bounds = boundsEl.getBoundingClientRect()

	alignRight.value = rect.left + POPOVER_WIDTH + POPOVER_MARGIN > bounds.right
	placeAbove.value = rect.bottom + POPOVER_HEIGHT_ESTIMATE + POPOVER_MARGIN > bounds.bottom
}

// Hue 0-360, saturation/value 0-100. Kept separate from the hex model value
// so dragging around the SV square / hue strip doesn't fight with rounding
// error from repeatedly converting hex -> hsv -> hex.
const hsv = reactive({ h: 0, s: 0, v: 0 })
const hexInput = ref(props.modelValue)

function clamp(value: number, min: number, max: number) {
	return Math.min(max, Math.max(min, value))
}

function hexToHsv(hex: string): { h: number; s: number; v: number } | null {
	const clean = hex.replace('#', '')
	if (!/^[0-9a-fA-F]{6}$/.test(clean) && !/^[0-9a-fA-F]{3}$/.test(clean)) return null
	const full = clean.length === 3 ? clean.replace(/(.)/g, '$1$1') : clean
	const r = Number.parseInt(full.slice(0, 2), 16) / 255
	const g = Number.parseInt(full.slice(2, 4), 16) / 255
	const b = Number.parseInt(full.slice(4, 6), 16) / 255
	const max = Math.max(r, g, b)
	const min = Math.min(r, g, b)
	const delta = max - min
	let h = 0
	if (delta !== 0) {
		if (max === r) h = ((g - b) / delta) % 6
		else if (max === g) h = (b - r) / delta + 2
		else h = (r - g) / delta + 4
		h *= 60
		if (h < 0) h += 360
	}
	const s = max === 0 ? 0 : delta / max
	const v = max
	return { h, s: s * 100, v: v * 100 }
}

function hsvToHex(h: number, s: number, v: number): string {
	const sNorm = s / 100
	const vNorm = v / 100
	const c = vNorm * sNorm
	const x = c * (1 - Math.abs(((h / 60) % 2) - 1))
	const m = vNorm - c
	let [r, g, b] = [0, 0, 0]
	if (h < 60) [r, g, b] = [c, x, 0]
	else if (h < 120) [r, g, b] = [x, c, 0]
	else if (h < 180) [r, g, b] = [0, c, x]
	else if (h < 240) [r, g, b] = [0, x, c]
	else if (h < 300) [r, g, b] = [x, 0, c]
	else [r, g, b] = [c, 0, x]
	const toHex = (n: number) =>
		Math.round((n + m) * 255)
			.toString(16)
			.padStart(2, '0')
	return `#${toHex(r)}${toHex(g)}${toHex(b)}`
}

function syncFromModelValue() {
	const parsed = hexToHsv(props.modelValue)
	if (parsed) {
		hsv.h = parsed.h
		hsv.s = parsed.s
		hsv.v = parsed.v
	}
	hexInput.value = props.modelValue
}

watch(() => props.modelValue, syncFromModelValue)
syncFromModelValue()

function commitFromHsv() {
	const hex = hsvToHex(hsv.h, hsv.s, hsv.v)
	hexInput.value = hex
	emit('update:modelValue', hex)
}

function onHexInput() {
	const value = hexInput.value.trim()
	const normalized = value.startsWith('#') ? value : `#${value}`
	if (/^#[0-9a-fA-F]{6}$/.test(normalized)) {
		const parsed = hexToHsv(normalized)
		if (parsed) {
			hsv.h = parsed.h
			hsv.s = parsed.s
			hsv.v = parsed.v
		}
		emit('update:modelValue', normalized)
	}
}

function svPointFromEvent(event: PointerEvent): { s: number; v: number } | null {
	const el = svArea.value
	if (!el) return null
	const rect = el.getBoundingClientRect()
	const x = clamp(event.clientX - rect.left, 0, rect.width)
	const y = clamp(event.clientY - rect.top, 0, rect.height)
	return { s: (x / rect.width) * 100, v: 100 - (y / rect.height) * 100 }
}

function hueFromEvent(event: PointerEvent): number | null {
	const el = hueTrack.value
	if (!el) return null
	const rect = el.getBoundingClientRect()
	const x = clamp(event.clientX - rect.left, 0, rect.width)
	return (x / rect.width) * 360
}

let draggingSv = false
let draggingHue = false

function onSvPointerDown(event: PointerEvent) {
	draggingSv = true
	;(event.target as HTMLElement).setPointerCapture(event.pointerId)
	const point = svPointFromEvent(event)
	if (point) {
		hsv.s = point.s
		hsv.v = point.v
		commitFromHsv()
	}
}

function onSvPointerMove(event: PointerEvent) {
	if (!draggingSv) return
	const point = svPointFromEvent(event)
	if (point) {
		hsv.s = point.s
		hsv.v = point.v
		commitFromHsv()
	}
}

function onHuePointerDown(event: PointerEvent) {
	draggingHue = true
	;(event.target as HTMLElement).setPointerCapture(event.pointerId)
	const h = hueFromEvent(event)
	if (h !== null) {
		hsv.h = h
		commitFromHsv()
	}
}

function onHuePointerMove(event: PointerEvent) {
	if (!draggingHue) return
	const h = hueFromEvent(event)
	if (h !== null) {
		hsv.h = h
		commitFromHsv()
	}
}

function stopDragging() {
	draggingSv = false
	draggingHue = false
}

function toggleOpen() {
	open.value = !open.value
	if (open.value) updatePopoverPlacement()
}

function onDocumentClick(event: MouseEvent) {
	if (!open.value) return
	if (rootEl.value && !rootEl.value.contains(event.target as Node)) {
		open.value = false
	}
}

function onWindowResize() {
	if (open.value) updatePopoverPlacement()
}

onMounted(() => {
	document.addEventListener('pointerdown', onDocumentClick)
	document.addEventListener('pointerup', stopDragging)
	window.addEventListener('resize', onWindowResize)
})
onBeforeUnmount(() => {
	document.removeEventListener('pointerdown', onDocumentClick)
	document.removeEventListener('pointerup', stopDragging)
	window.removeEventListener('resize', onWindowResize)
})
</script>

<template>
	<div ref="rootEl" class="studio-color-swatch">
		<button
			type="button"
			class="studio-color-swatch__trigger"
			:style="{ '--studio-color-swatch-value': props.modelValue }"
			@click="toggleOpen"
		>
			<span class="studio-color-swatch__ring">
				<span class="studio-color-swatch__fill" />
			</span>
			<span v-if="label" class="studio-color-swatch__label">{{ label }}</span>
		</button>

		<div
			v-if="open"
			class="studio-color-swatch__popover"
			:class="{
				'studio-color-swatch__popover--right': alignRight,
				'studio-color-swatch__popover--above': placeAbove,
			}"
		>
			<div
				ref="svArea"
				class="studio-color-swatch__sv"
				:style="{ '--studio-hue': hsv.h }"
				@pointerdown="onSvPointerDown"
				@pointermove="onSvPointerMove"
			>
				<div
					class="studio-color-swatch__sv-thumb"
					:style="{ left: `${hsv.s}%`, top: `${100 - hsv.v}%` }"
				/>
			</div>
			<div ref="hueTrack" class="studio-color-swatch__hue" @pointerdown="onHuePointerDown" @pointermove="onHuePointerMove">
				<div class="studio-color-swatch__hue-thumb" :style="{ left: `${(hsv.h / 360) * 100}%` }" />
			</div>
			<div class="studio-color-swatch__hex-row">
				<span class="studio-color-swatch__hex-preview" :style="{ backgroundColor: props.modelValue }" />
				<input
					v-model="hexInput"
					type="text"
					class="studio-color-swatch__hex-input"
					spellcheck="false"
					autocomplete="off"
					@input="onHexInput"
					@keyup.enter="onHexInput"
				/>
			</div>
		</div>
	</div>
</template>

<style scoped>
.studio-color-swatch {
	position: relative;
	display: inline-block;
}

.studio-color-swatch__trigger {
	position: relative;
	display: inline-flex;
	align-items: center;
	gap: 0.5rem;
	padding: 0.3125rem 0.75rem 0.3125rem 0.3125rem;
	border-radius: 9999px;
	border: 1px solid var(--color-surface-5, var(--color-divider));
	background-color: var(--color-button-bg);
	color: var(--color-primary);
	cursor: pointer;
	font: inherit;
	transition:
		border-color 100ms ease,
		background-color 100ms ease;
}

.studio-color-swatch__trigger:hover {
	background-color: var(--color-button-bg-hover, var(--color-button-bg));
	border-color: var(--color-surface-6, var(--color-brand));
}

.studio-color-swatch__trigger:focus-visible {
	outline: 2px solid var(--color-brand);
	outline-offset: 2px;
}

.studio-color-swatch__ring {
	display: inline-flex;
	align-items: center;
	justify-content: center;
	width: 1.75rem;
	height: 1.75rem;
	border-radius: 9999px;
	padding: 3px;
	background-color: color-mix(in srgb, var(--studio-color-swatch-value) 35%, transparent);
	flex-shrink: 0;
}

.studio-color-swatch__fill {
	display: block;
	width: 100%;
	height: 100%;
	border-radius: 9999px;
	background-color: var(--studio-color-swatch-value);
	box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.15);
}

.studio-color-swatch__label {
	font-size: 0.8125rem;
	font-weight: 600;
	white-space: nowrap;
}

.studio-color-swatch__popover {
	position: absolute;
	z-index: 50;
	top: calc(100% + 8px);
	left: 0;
	display: flex;
	flex-direction: column;
	gap: 0.75rem;
	width: 220px;
	padding: 0.75rem;
	border-radius: 12px;
	border: 1px solid var(--color-surface-5, var(--color-divider));
	background-color: var(--color-raised-bg);
	box-shadow: var(--shadow-floating, 0 8px 24px rgba(0, 0, 0, 0.4));
}

/* Modrinth Studios addition: flipped placement, toggled from script once the
	trigger's actual position is known — see updatePopoverPlacement(). */
.studio-color-swatch__popover--right {
	left: auto;
	right: 0;
}

.studio-color-swatch__popover--above {
	top: auto;
	bottom: calc(100% + 8px);
}

.studio-color-swatch__sv {
	position: relative;
	width: 100%;
	height: 120px;
	border-radius: 8px;
	touch-action: none;
	cursor: crosshair;
	background-color: hsl(var(--studio-hue) 100% 50%);
	background-image:
		linear-gradient(to top, #000, transparent),
		linear-gradient(to right, #fff, transparent);
}

.studio-color-swatch__sv-thumb {
	position: absolute;
	width: 12px;
	height: 12px;
	border-radius: 9999px;
	border: 2px solid white;
	box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.5);
	transform: translate(-50%, -50%);
	pointer-events: none;
}

.studio-color-swatch__hue {
	position: relative;
	width: 100%;
	height: 12px;
	border-radius: 9999px;
	touch-action: none;
	cursor: pointer;
	background-image: linear-gradient(
		to right,
		#ff0000 0%,
		#ffff00 17%,
		#00ff00 33%,
		#00ffff 50%,
		#0000ff 67%,
		#ff00ff 83%,
		#ff0000 100%
	);
}

.studio-color-swatch__hue-thumb {
	position: absolute;
	top: 50%;
	width: 14px;
	height: 14px;
	border-radius: 9999px;
	border: 2px solid white;
	box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.5);
	transform: translate(-50%, -50%);
	pointer-events: none;
}

.studio-color-swatch__hex-row {
	display: flex;
	align-items: center;
	gap: 0.5rem;
}

.studio-color-swatch__hex-preview {
	width: 1.75rem;
	height: 1.75rem;
	border-radius: 8px;
	flex-shrink: 0;
	box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.15);
}

.studio-color-swatch__hex-input {
	flex: 1;
	min-width: 0;
	padding: 0.375rem 0.5rem;
	border-radius: 8px;
	border: 1px solid var(--color-surface-5, var(--color-divider));
	background-color: var(--color-button-bg);
	color: var(--color-contrast);
	font: inherit;
	font-family: var(--mono-font, monospace);
}

.studio-color-swatch__hex-input:focus-visible {
	outline: 2px solid var(--color-brand);
	outline-offset: 1px;
}
</style>
