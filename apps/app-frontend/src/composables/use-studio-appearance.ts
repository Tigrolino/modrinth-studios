// Modrinth Studios addition: custom accent color / background / modal
// opacity / app icon. New file — everything here is additive and layered on
// top of the app via CSS custom properties + a small !important override
// stylesheet (src/assets/styles/studio-overrides.css), so nothing in the
// shared @modrinth/ui / packages/assets theme files needs to change.
//
// Preferences are stored in localStorage (per-machine, which is fine here —
// each friend running their own copy of the app can pick their own look).
import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { reactive, watch } from 'vue'

const STORAGE_KEY = 'studio-appearance-v1'

export type StudioBackgroundMode = 'default' | 'image' | 'gradient'

export interface StudioAppearanceState {
	accentColor: string | null
	backgroundMode: StudioBackgroundMode
	/** Absolute path under $APPCONFIG/studio, set by studio_set_background_image */
	backgroundImagePath: string | null
	gradientFrom: string
	gradientTo: string
	gradientAngle: number
	/** 0-100: darkens/tints the image or gradient toward the app's normal background, for readability */
	backgroundOverlay: number
	/** 0-100 */
	modalOpacity: number
	windowIconPath: string | null
	/** When true, don't recolor --color-green to the accent color — keeps
	 * update buttons, the Beta tag, and other "success"-styled elements
	 * their original green regardless of accent color. */
	preserveUpdateGreen: boolean
}

const defaults: StudioAppearanceState = {
	accentColor: null,
	backgroundMode: 'default',
	backgroundImagePath: null,
	gradientFrom: '#7c3aed',
	gradientTo: '#2563eb',
	gradientAngle: 135,
	backgroundOverlay: 55,
	modalOpacity: 100,
	windowIconPath: null,
	preserveUpdateGreen: false,
}

function load(): StudioAppearanceState {
	try {
		const raw = localStorage.getItem(STORAGE_KEY)
		if (!raw) return { ...defaults }
		return { ...defaults, ...JSON.parse(raw) }
	} catch (err) {
		console.error('[modrinth-studios] failed to load appearance prefs', err)
		return { ...defaults }
	}
}

const state = reactive<StudioAppearanceState>(load())

// The full state gets JSON.stringify'd + written to localStorage on every
// change. That's cheap on its own, but a slider fires many updates a second
// while being dragged, so persisting on every single tick was previously
// enough to visibly tank frame rate. Debounce it — the CSS side effects
// below still apply instantly, only the disk write is delayed.
let persistTimeout: ReturnType<typeof setTimeout> | null = null
function schedulePersist() {
	if (persistTimeout) clearTimeout(persistTimeout)
	persistTimeout = setTimeout(() => {
		persistTimeout = null
		try {
			localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
		} catch (err) {
			console.error('[modrinth-studios] failed to save appearance prefs', err)
		}
	}, 300)
}

function hexToRgba(hex: string, alpha: number): string {
	const clean = hex.replace('#', '')
	const value = Number.parseInt(clean.length === 3 ? clean.replace(/(.)/g, '$1$1') : clean, 16)
	const r = (value >> 16) & 255
	const g = (value >> 8) & 255
	const b = value & 255
	return `rgba(${r}, ${g}, ${b}, ${alpha})`
}

// Deliberately NOT included here: --color-button-bg-selected. In the stock
// theme it's defined as `var(--color-brand-highlight)` (or `var(--color-brand)`
// in the light theme) — a reference, not its own value — so it already picks
// up our accent color for free via normal CSS cascading. Overriding it
// directly to the flat accent color used to make it identical to the icon
// color drawn on top of it, which is what turned "selected" icon buttons
// into solid blobs with an invisible icon.
const ACCENT_PROPERTIES = [
	'--color-brand',
	'--color-brand-highlight',
	'--color-brand-shadow',
	'--loading-bar-gradient',
] as const

// A lot of upstream UI (the home page's quick-play buttons, the "Beta" tag,
// server ping indicators, update buttons, etc.) is styled with a literal
// "green" color rather than the semantic "brand" one — historically they
// were the same color, so nothing distinguished them. Overriding brand alone
// leaves large chunks of the UI stuck on the original green, which is the
// "accent color shows but not everywhere" issue; overriding green too makes
// the accent consistent everywhere, at the cost of also recoloring a few
// things that are semantically "positive/success"/"update available" rather
// than "brand". Kept as a separate list (instead of folded into
// ACCENT_PROPERTIES) so `preserveUpdateGreen` can opt out of just this part.
const GREEN_ACCENT_PROPERTIES = ['--color-green', '--color-green-highlight'] as const

function applyAccentVars() {
	if (typeof document === 'undefined') return
	const root = document.documentElement.style

	if (state.accentColor) {
		root.setProperty('--color-brand', state.accentColor)
		root.setProperty('--color-brand-highlight', hexToRgba(state.accentColor, 0.25))
		root.setProperty('--color-brand-shadow', hexToRgba(state.accentColor, 0.7))
		root.setProperty(
			'--loading-bar-gradient',
			`linear-gradient(to right, ${state.accentColor} 0%, ${state.accentColor} 100%)`,
		)
		if (state.preserveUpdateGreen) {
			for (const prop of GREEN_ACCENT_PROPERTIES) root.removeProperty(prop)
		} else {
			root.setProperty('--color-green', state.accentColor)
			root.setProperty('--color-green-highlight', hexToRgba(state.accentColor, 0.25))
		}
	} else {
		for (const prop of ACCENT_PROPERTIES) root.removeProperty(prop)
		for (const prop of GREEN_ACCENT_PROPERTIES) root.removeProperty(prop)
	}
}

function applyBackgroundVars() {
	if (typeof document === 'undefined') return
	const root = document.documentElement.style

	if (state.backgroundMode === 'image' && state.backgroundImagePath) {
		root.setProperty('--studio-bg-image', `url("${convertFileSrc(state.backgroundImagePath)}")`)
	} else if (state.backgroundMode === 'gradient') {
		root.setProperty(
			'--studio-bg-image',
			`linear-gradient(${state.gradientAngle}deg, ${state.gradientFrom}, ${state.gradientTo})`,
		)
	} else {
		root.removeProperty('--studio-bg-image')
	}
}

function applyOverlayVar() {
	if (typeof document === 'undefined') return
	const root = document.documentElement.style
	const overlayAlpha = state.backgroundMode === 'default' ? 0 : state.backgroundOverlay / 100
	root.setProperty('--studio-bg-overlay-alpha', String(overlayAlpha))
}

// Surface transparency: when a custom background is active, let it show
// through virtually every panel/button/input surface in the app, not just
// the modal background, reusing the same "modal opacity" slider so there's
// one consistent transparency knob instead of a separate one per surface.
// This works by reading each surface variable's current *computed* (theme)
// color once per application and re-setting it as a color-mix() of that same
// color — never as `var(--x)` referencing itself, which CSS treats as
// invalid.
//
// This used to be limited to --color-raised-bg only. --color-button-bg and
// the base --surface-1..5 scale are used directly by things like WorldItem's
// `bg-bg-raised`, InputFrame's `bg-surface-4`, and the zebra-striped rows in
// ContentCardTable — the shared mod/resource-pack/shader/datapack list used
// across a huge number of pages, via `bg-surface-1.5`/`bg-surface-2`/
// `bg-surface-2.5`. Note that only the *transparency* here is applied
// broadly like this; the accompanying `backdrop-filter: blur()` (see
// applyGlassBlurVar()/studio-overrides.css) is deliberately NOT applied to
// all of these — blurring every row of a long list at once overwhelmed the
// webview's compositor (see the comment on applyGlassBlurVar for why).
const SURFACE_PROPERTIES = [
	'--color-raised-bg',
	'--color-button-bg',
	'--surface-1',
	'--surface-1-5',
	'--surface-2',
	'--surface-2-5',
	'--surface-3',
	'--surface-4',
	'--surface-5',
] as const
const surfaceOriginals = new Map<string, string>()

function applySurfaceVars() {
	if (typeof document === 'undefined') return
	const root = document.documentElement
	const rootStyle = root.style

	if (state.backgroundMode === 'default') {
		for (const prop of SURFACE_PROPERTIES) rootStyle.removeProperty(prop)
		surfaceOriginals.clear()
		return
	}

	const alpha = state.modalOpacity / 100
	// The "darken" slider (backgroundOverlay) only ever tinted the raw
	// background image itself (see .app-contents in studio-overrides.css) —
	// it had no effect on these panels, so a bright image behind a very
	// transparent panel could leave text on that panel hard to read with no
	// way to fix it short of cranking transparency all the way down. Blend
	// the same darken amount into the panel's own base color before making
	// it translucent, so raising "darken" also directly improves contrast on
	// every glass panel, not just the parts of the image visible in the
	// gaps between them. Capped at 50% black so a maxed-out slider still
	// leaves the surface's own hue recognizable rather than going flat black.
	const darkenAlpha = (state.backgroundOverlay / 100) * 0.5
	const computed = getComputedStyle(root)
	for (const prop of SURFACE_PROPERTIES) {
		// Capture the theme's real value once (before we override it) so
		// switching theme/accent doesn't get "stuck" reading our own override
		// back as the original.
		if (!surfaceOriginals.has(prop)) {
			rootStyle.removeProperty(prop)
			const original = computed.getPropertyValue(prop).trim()
			if (!original) continue
			surfaceOriginals.set(prop, original)
		}
		const original = surfaceOriginals.get(prop)
		if (!original) continue
		const darkened =
			darkenAlpha > 0
				? `color-mix(in srgb, ${original} ${(1 - darkenAlpha) * 100}%, black ${darkenAlpha * 100}%)`
				: original
		rootStyle.setProperty(prop, `color-mix(in srgb, ${darkened} ${alpha * 100}%, transparent)`)
	}
}

// The right sidebar's actual visible background is --brand-gradient-bg, not
// any of the --color-* surface tokens above — easy to miss since it's a
// literal `linear-gradient(...)` value, not a plain color, so it can't go
// through the generic color-mix() approach (color-mix needs two <color>
// values, not a gradient). This is a separate, dedicated override for that
// one variable: a flat dark tint whose strength follows the same slider,
// replacing the theme's subtle built-in gradient while a custom background
// is active so the sidebar is actually see-through instead of unaffected.
function applySidebarTintVar() {
	if (typeof document === 'undefined') return
	const root = document.documentElement.style
	if (state.backgroundMode === 'default') {
		root.removeProperty('--brand-gradient-bg')
		return
	}
	const alpha = state.modalOpacity / 100
	root.setProperty('--brand-gradient-bg', `rgba(0, 0, 0, ${(alpha * 0.35).toFixed(3)})`)
}

// Library instance cards without custom cover art fall back to a solid,
// procedurally hash-tinted placeholder background (Avatar's `tintBy`, via
// --color-button-bg), and the card panel itself is `bg-surface-3` (i.e.
// --surface-3). Both are now covered automatically by the broadened
// SURFACE_PROPERTIES above — CSS custom properties resolve `var(...)`
// recursively, so Avatar's own `color-mix(in oklch, var(--color-button-bg)
// ..., ...)` picks up our translucent override for free. No dedicated code
// needed here any more (there used to be a --studio-card-alpha special case
// before SURFACE_PROPERTIES was broadened to include --color-button-bg).

// Plain color-mix() transparency alone reads as "washed out" rather than
// "see-through" on small/detailed surfaces, because the background image's
// full sharp detail shows straight through with nothing to visually
// separate foreground content from it — exactly what the app's own modals
// avoid by blurring what's behind them (NewModal.vue's
// `.modal-overlay { backdrop-filter: blur(5px) }`).
//
// This var is consumed in studio-overrides.css on most of the surfaces
// above directly — but deliberately NOT on --surface-1/1-5/2/2-5, the
// zebra-striped row backgrounds ContentCardTable uses for its mod/
// resource-pack/shader/datapack list. That list can run to hundreds of
// rows, and blurring every one of them at once overwhelmed the webview's
// compositor (corrupted color blocks, heavy flashing while scrolling) — a
// single shared wrapper blurred once instead of per-row was also tried and
// looked worse in practice. Those rows still get the plain color-mix()
// transparency from SURFACE_PROPERTIES, just with no blur layer. Everything
// else here is normally a much smaller, bounded set of elements on screen
// at once and keeps its blur.
//
// Deliberately a flat on/off value (not scaled by the opacity slider) and
// watched only on backgroundMode, not modalOpacity — recalculating this on
// every slider tick was exactly what caused the old FPS-drop-while-dragging
// bug elsewhere.
function applyGlassBlurVar() {
	if (typeof document === 'undefined') return
	const root = document.documentElement.style
	if (state.backgroundMode === 'default') {
		root.removeProperty('--studio-glass-blur')
		return
	}
	root.setProperty('--studio-glass-blur', 'blur(10px)')
}

function applyModalOpacityVar() {
	if (typeof document === 'undefined') return
	document.documentElement.style.setProperty('--studio-modal-opacity', String(state.modalOpacity / 100))
}

/** Re-applies the saved custom window icon. Call once on app startup, and
 * again whenever the user picks a new one. This only changes the *running*
 * window/taskbar icon — the .exe's own embedded icon (what Explorer shows
 * before the app is even open) can only be changed by rebuilding the app
 * with new files under apps/app/icons/. */
export async function applyStudioWindowIcon() {
	if (!state.windowIconPath) return
	try {
		await getCurrentWindow().setIcon(state.windowIconPath)
	} catch (err) {
		console.error('[modrinth-studios] failed to apply custom window icon', err)
	}
}

/** Copies `sourcePath` into the app's config dir (so it's inside the asset
 * protocol's allowed scope) and switches the background to it. */
export async function setStudioBackgroundImage(sourcePath: string) {
	const destPath = await invoke<string>('plugin:studio|studio_set_background_image', {
		sourcePath,
	})
	state.backgroundImagePath = destPath
	state.backgroundMode = 'image'
}

/** Copies `sourcePath` into the app's config dir (same reasoning as
 * `setStudioBackgroundImage`: the webview can only load images from inside
 * the asset-protocol scope, and the original file could be moved/deleted
 * later) and applies it as both the running window/taskbar icon and the
 * in-app title bar icon. */
export async function setStudioWindowIcon(path: string | null) {
	if (path) {
		const destPath = await invoke<string>('plugin:studio|studio_set_app_icon', {
			sourcePath: path,
		})
		state.windowIconPath = destPath
		await applyStudioWindowIcon()
	} else {
		state.windowIconPath = null
	}
}

/** Applies every studio appearance effect. Call once on startup; after that,
 * the targeted watchers below keep things in sync without redoing
 * unaffected work on every keystroke/slider tick. */
export function applyStudioAppearance() {
	applyAccentVars()
	applyBackgroundVars()
	applyOverlayVar()
	applyModalOpacityVar()
	applySurfaceVars()
	applySidebarTintVar()
	applyGlassBlurVar()
}

// Deliberately split into narrow watchers instead of one `watch(state, ...,
// { deep: true })`. The old version re-ran *every* effect — including
// re-setting the background-image CSS var (and therefore re-decoding/
// repainting the image) — on every tick of an unrelated slider like modal
// opacity, which is what caused severe frame drops while dragging.
watch([() => state.accentColor, () => state.preserveUpdateGreen], applyAccentVars)
watch(
	[
		() => state.backgroundMode,
		() => state.backgroundImagePath,
		() => state.gradientFrom,
		() => state.gradientTo,
		() => state.gradientAngle,
	],
	applyBackgroundVars,
)
watch([() => state.backgroundMode, () => state.backgroundOverlay], applyOverlayVar)
watch(() => state.modalOpacity, applyModalOpacityVar)
watch(
	[() => state.backgroundMode, () => state.modalOpacity, () => state.backgroundOverlay],
	applySurfaceVars,
)
watch([() => state.backgroundMode, () => state.modalOpacity], applySidebarTintVar)
watch(() => state.backgroundMode, applyGlassBlurVar)
// modalOpacity defaults to 100 (fully opaque), which is correct for popups
// on their own — but it also meant switching on a custom background had no
// visible transparency effect at all until you happened to go drag that
// slider down. Nudge it down automatically, once, the first time a custom
// background is turned on, so the effect is visible immediately. Only fires
// if the user hasn't already touched the slider (still at the 100 default).
watch(
	() => state.backgroundMode,
	(mode, previousMode) => {
		if (mode !== 'default' && previousMode === 'default' && state.modalOpacity === 100) {
			state.modalOpacity = 75
		}
	},
)
watch(state, schedulePersist, { deep: true })

export function useStudioAppearance() {
	return state
}
