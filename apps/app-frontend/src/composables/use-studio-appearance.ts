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
	} else {
		for (const prop of ACCENT_PROPERTIES) root.removeProperty(prop)
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
// through the app's normal opaque cards/panels too (not just the modal
// background), reusing the same "modal opacity" slider so there's one
// consistent transparency knob instead of a second one. This works by
// reading each surface variable's current *computed* (theme) color once
// per application and re-setting it as a color-mix() of that same color —
// never as `var(--x)` referencing itself, which CSS treats as invalid.
const SURFACE_PROPERTIES = [
	'--color-raised-bg',
	'--color-bg-secondary',
	'--color-button-bg',
	'--color-surface-1',
	'--color-surface-2',
	'--color-surface-3',
	'--color-surface-4',
	'--color-surface-5',
	'--color-surface-6',
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
		rootStyle.setProperty(prop, `color-mix(in srgb, ${original} ${alpha * 100}%, transparent)`)
	}
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

export async function setStudioWindowIcon(path: string | null) {
	state.windowIconPath = path
	if (path) {
		await applyStudioWindowIcon()
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
}

// Deliberately split into narrow watchers instead of one `watch(state, ...,
// { deep: true })`. The old version re-ran *every* effect — including
// re-setting the background-image CSS var (and therefore re-decoding/
// repainting the image) — on every tick of an unrelated slider like modal
// opacity, which is what caused severe frame drops while dragging.
watch(() => state.accentColor, applyAccentVars)
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
watch([() => state.backgroundMode, () => state.modalOpacity], applySurfaceVars)
watch(state, schedulePersist, { deep: true })

export function useStudioAppearance() {
	return state
}
