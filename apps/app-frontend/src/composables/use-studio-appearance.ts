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
	/** 0-100: how much of the normal solid background still shows over the image/gradient, for readability */
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

function persist() {
	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
	} catch (err) {
		console.error('[modrinth-studios] failed to save appearance prefs', err)
	}
}

function hexToRgba(hex: string, alpha: number): string {
	const clean = hex.replace('#', '')
	const value = Number.parseInt(clean.length === 3 ? clean.replace(/(.)/g, '$1$1') : clean, 16)
	const r = (value >> 16) & 255
	const g = (value >> 8) & 255
	const b = value & 255
	return `rgba(${r}, ${g}, ${b}, ${alpha})`
}

const ACCENT_PROPERTIES = [
	'--color-brand',
	'--color-brand-highlight',
	'--color-brand-shadow',
	'--color-button-bg-selected',
	'--loading-bar-gradient',
] as const

export function applyStudioAppearance() {
	if (typeof document === 'undefined') return
	const root = document.documentElement.style

	if (state.accentColor) {
		root.setProperty('--color-brand', state.accentColor)
		root.setProperty('--color-brand-highlight', hexToRgba(state.accentColor, 0.2))
		root.setProperty('--color-brand-shadow', hexToRgba(state.accentColor, 0.7))
		root.setProperty('--color-button-bg-selected', state.accentColor)
		root.setProperty(
			'--loading-bar-gradient',
			`linear-gradient(to right, ${state.accentColor} 0%, ${state.accentColor} 100%)`,
		)
	} else {
		for (const prop of ACCENT_PROPERTIES) root.removeProperty(prop)
	}

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

	const overlayAlpha = state.backgroundMode === 'default' ? 1 : state.backgroundOverlay / 100
	root.setProperty('--studio-bg-overlay-alpha', String(overlayAlpha))
	root.setProperty('--studio-modal-opacity', String(state.modalOpacity / 100))
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

watch(
	state,
	() => {
		persist()
		applyStudioAppearance()
	},
	{ deep: true },
)

export function useStudioAppearance() {
	return state
}
