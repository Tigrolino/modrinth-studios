// Modrinth Studios addition: custom accent color / background / surface
// darkness / app icon. New file — everything here is additive and layered on
// top of the app via CSS custom properties + a small !important override
// stylesheet (src/assets/styles/studio-overrides.css), so nothing in the
// shared @modrinth/ui / packages/assets theme files needs to change.
//
// Preferences are stored in localStorage (per-machine, which is fine here —
// each friend running their own copy of the app can pick their own look).
import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { readFile } from '@tauri-apps/plugin-fs'
import { reactive, watch } from 'vue'

import { type ColorTheme, useTheme } from './use-theme'

const STORAGE_KEY = 'studio-appearance-v1'

export type StudioBackgroundMode = 'default' | 'image' | 'gradient' | 'video'

/** How the next image in `backgroundImagePaths` is picked once one is due. */
export type StudioBackgroundRotationMode = 'loop' | 'random'

/**
 * How often the background image changes when more than one is set.
 * `'session'` changes it once per app launch rather than on a timer — see
 * `maybeRotateOnSessionStart()`.
 */
export type StudioBackgroundRotationInterval =
	| 'session'
	| '5m'
	| '15m'
	| '30m'
	| '1h'
	| '6h'
	| '1d'

export interface StudioAppearanceState {
	accentColor: string | null
	backgroundMode: StudioBackgroundMode
	/** The single image currently shown. Always one of `backgroundImagePaths`
	 * (or `null` if that list is empty) — kept as its own field, rather than
	 * just an index, so applyBackgroundVars() and its watcher don't need to
	 * know anything about rotation at all; they just render whatever this
	 * says, same as before multiple images existed. */
	backgroundImagePath: string | null
	/** Every image available to rotate through, in a fixed order (used as-is
	 * for `'loop'` mode). A single picked file still populates this with one
	 * entry. Absolute paths under $APPCONFIG/studio/backgrounds, set by
	 * studio_set_background_images / studio_set_background_folder. */
	backgroundImagePaths: string[]
	/** Same idea as `backgroundImagePath`/`backgroundImagePaths`, but for
	 * video — the two pools are entirely separate (a picked video never
	 * shows up in the image pool or vice versa), but share the same rotation
	 * mode/interval/last-rotated-at fields below, since only one of the two
	 * pools is ever actually active at once (whichever `backgroundMode` is
	 * currently set to). */
	backgroundVideoPath: string | null
	backgroundVideoPaths: string[]
	backgroundRotationMode: StudioBackgroundRotationMode
	backgroundRotationInterval: StudioBackgroundRotationInterval
	/** Unix ms of the last rotation (or when the pool was set). Persisted so
	 * a long interval like "once per day" is judged against real elapsed
	 * time, including while the app was closed — not reset every launch. */
	backgroundLastRotatedAt: number
	gradientFrom: string
	gradientTo: string
	gradientAngle: number
	/** 0-100: darkens/tints the image or gradient itself toward the app's normal background, for readability */
	backgroundOverlay: number
	/** 0-100: darkens panels/cards/sidebar (NOT modals/popups — see
	 * `popupOpacity`) toward black, only while a custom background is active.
	 * Transparency for these surfaces is a fixed constant (FIXED_SURFACE_ALPHA
	 * below), not user controlled — this only ever affects how dark they are
	 * tinted, never how much background shows through them. */
	surfaceDarkness: number
	/** 0-100: how opaque popups/modals are. Unlike `surfaceDarkness`, this
	 * always applies, in every background mode — a popup can be made
	 * translucent purely as a look, independent of whether a custom
	 * background exists to show through it. */
	popupOpacity: number
	/** 0-20: `backdrop-filter: blur()` radius in px applied to surfaces while a
	 * custom background is active (see `applyGlassBlurVar()`) — 0 turns the
	 * blur off entirely (a plain translucent look, no frosted-glass effect)
	 * without needing a separate on/off toggle. Only matters in non-default
	 * background modes, same as `surfaceDarkness`. */
	glassBlurStrength: number
	windowIconPath: string | null
	/** When true, don't recolor --color-green to the accent color — keeps
	 * update buttons, the Beta tag, and other "success"-styled elements
	 * their original green regardless of accent color. */
	preserveUpdateGreen: boolean
	/** Absolute path (under $APPCONFIG/studio) to a custom startup/splash
	 * screen background image, replacing the default cube artwork. `null`
	 * means use the default. */
	splashBackgroundPath: string | null
	/** When true (the default), the running window/taskbar icon is recolored
	 * to match the accent color whenever one is active and no fully custom
	 * icon has been picked — see `applyAccentIconTint()`. Surfaced as a
	 * toggle in Settings only once that situation actually applies. */
	tintDefaultIconWithAccent: boolean
	/** Modrinth Studios addition: when true, the instance detail page's
	 * header shows how much disk space that instance uses next to its total
	 * playtime — see `StorageSettings.vue` (where this toggle lives) and
	 * `use-studio-instance-storage.ts` (the actual size computation). Off by
	 * default since the underlying measurement is a full recursive folder
	 * walk on the Rust side — cheap for one instance, but no reason to pay it
	 * for people who never look at it. */
	showInstanceStorageUsage: boolean
}

const defaults: StudioAppearanceState = {
	accentColor: null,
	backgroundMode: 'default',
	backgroundImagePath: null,
	backgroundImagePaths: [],
	backgroundVideoPath: null,
	backgroundVideoPaths: [],
	backgroundRotationMode: 'loop',
	backgroundRotationInterval: 'session',
	backgroundLastRotatedAt: 0,
	gradientFrom: '#7c3aed',
	gradientTo: '#2563eb',
	gradientAngle: 135,
	backgroundOverlay: 55,
	surfaceDarkness: 40,
	popupOpacity: 100,
	glassBlurStrength: 10,
	windowIconPath: null,
	preserveUpdateGreen: false,
	splashBackgroundPath: null,
	tintDefaultIconWithAccent: true,
	showInstanceStorageUsage: false,
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

function hexToRgb(hex: string): [number, number, number] {
	const clean = hex.replace('#', '')
	const value = Number.parseInt(clean.length === 3 ? clean.replace(/(.)/g, '$1$1') : clean, 16)
	return [(value >> 16) & 255, (value >> 8) & 255, value & 255]
}

function hexToRgba(hex: string, alpha: number): string {
	const [r, g, b] = hexToRgb(hex)
	return `rgba(${r}, ${g}, ${b}, ${alpha})`
}

// WCAG relative luminance, used to pick readable text for whatever sits on
// top of a solid accent-colored background (see --color-accent-contrast
// below) — this is the actual bug behind "light theme is unreadable with a
// custom accent color". Light theme's own `--color-button-text-selected`
// (Settings' left nav active item, dropdown selections, etc.) is defined as
// `var(--color-accent-contrast)` painted on a SOLID `var(--color-brand)`
// background. We override --color-brand to the user's flat accent hex, but
// were never touching --color-accent-contrast to match — so light theme kept
// using its own fixed white text (`--color-accent-contrast: #ffffff` in
// light-properties), which reads fine against the *original* green but can
// disappear against whatever arbitrary color the user actually picked.
// (Dark theme doesn't have this problem: its equivalent pairing is
// brand-colored *text* on a translucent brand-highlight background, so both
// sides always move together automatically whenever --color-brand changes —
// which is exactly why only light theme was ever reported as broken.)
function pickContrastColor(hex: string): string {
	const [r, g, b] = hexToRgb(hex)
	const [rl, gl, bl] = [r, g, b].map((c) => {
		const s = c / 255
		return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
	})
	const luminance = 0.2126 * rl + 0.7152 * gl + 0.0722 * bl
	return luminance > 0.45 ? '#000000' : '#ffffff'
}

// Deliberately NOT included here: --color-button-bg-selected. In the stock
// theme it's defined as `var(--color-brand-highlight)` (or `var(--color-brand)`
// in the light theme) — a reference, not its own value — so it already picks
// up our accent color for free via normal CSS cascading. Overriding it
// directly to the flat accent color used to make it identical to the icon
// color drawn on top of it, which is what turned "selected" icon buttons
// into solid blobs with an invisible icon.
// --color-button-text-selected and --studio-accent-safe-text are included
// here for the same reason: stock dark theme's `--color-button-text-selected`
// is `var(--color-brand)` — colored text — sitting on `--color-brand-highlight`,
// a translucent *tint* of that same color over an otherwise dark panel. That
// reads fine for any normal accent (colored text against a dark panel is
// readable regardless of the color), but breaks down once the accent itself
// is dark enough to read as text — the Settings nav's selected item, the
// Beta badge, and the Combobox dropdown's selected item (`studio-overrides.css`)
// all do exactly this. Rather than replace "colored text" outright (which
// would lose the nice accent-tinted look for every normal color, not just
// the broken ones), applyAccentVars() below only overrides these when the
// accent is dark enough that its own contrast color came out white — i.e.
// only in the case that was actually broken — and leaves them alone
// (removeProperty, falling back to stock `var(--color-brand)`) otherwise.
const ACCENT_PROPERTIES = [
	'--color-brand',
	'--color-brand-highlight',
	'--color-brand-shadow',
	'--loading-bar-gradient',
	'--color-accent-contrast',
	'--color-button-text-selected',
	'--studio-accent-safe-text',
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
		const contrastColor = pickContrastColor(state.accentColor)
		root.setProperty('--color-accent-contrast', contrastColor)
		// Only the "accent is dark, so white was picked" case is the one
		// that's actually broken for these two (see the comment on
		// ACCENT_PROPERTIES above) — leave them referencing the accent color
		// directly otherwise, so a normal/bright accent keeps its own color
		// as text here instead of always going flat white or black.
		if (contrastColor === '#ffffff') {
			root.setProperty('--color-button-text-selected', contrastColor)
			root.setProperty('--studio-accent-safe-text', contrastColor)
		} else {
			root.removeProperty('--color-button-text-selected')
			root.removeProperty('--studio-accent-safe-text')
		}
		if (state.preserveUpdateGreen) {
			for (const prop of GREEN_ACCENT_PROPERTIES) root.removeProperty(prop)
		} else {
			root.setProperty('--color-green', state.accentColor)
			root.setProperty('--color-green-highlight', hexToRgba(state.accentColor, 0.25))
		}
		// Bridge var for SplashScreen.vue only. The splash forces a `.dark`
		// class on its own root (so it looks the same regardless of the
		// user's light/dark theme setting), and variables.scss's `.dark`
		// theme block redeclares --color-brand (back to plain green) on
		// that same element. A CSS custom property declared directly on an
		// element always wins over whatever was inherited from an ancestor
		// (like the accent override set on <html> here), no matter how much
		// more specific the ancestor's own rule was — so the splash was
		// stuck on default green even with a custom accent set. This name
		// isn't referenced anywhere in the `.dark`/`.light`/theme blocks, so
		// it survives that reset; SplashScreen.vue reads it back with a
		// `.splash-screen.dark { --color-brand: var(--studio-brand-override, ...) }`
		// override of its own that's specific enough to beat the theme
		// block's plain `.dark` selector.
		root.setProperty('--studio-brand-override', state.accentColor)
	} else {
		for (const prop of ACCENT_PROPERTIES) root.removeProperty(prop)
		for (const prop of GREEN_ACCENT_PROPERTIES) root.removeProperty(prop)
		root.removeProperty('--studio-brand-override')
	}
}

function applyBackgroundVars() {
	if (typeof document === 'undefined') return
	const root = document.documentElement.style

	if (state.backgroundMode === 'image' && state.backgroundImagePath) {
		root.setProperty('--studio-bg-image', `url("${convertFileSrc(state.backgroundImagePath)}")`)
		root.removeProperty('--studio-app-bg-color')
	} else if (state.backgroundMode === 'gradient') {
		root.setProperty(
			'--studio-bg-image',
			`linear-gradient(${state.gradientAngle}deg, ${state.gradientFrom}, ${state.gradientTo})`,
		)
		root.removeProperty('--studio-app-bg-color')
	} else if (state.backgroundMode === 'video' && state.backgroundVideoPath) {
		// There's no way to play a video through a CSS `background-image` — the
		// actual pixels come from a real `<video>` element (StudioVideoBackground.vue)
		// fixed behind the whole app. This just clears any leftover image/gradient
		// and makes `.app-contents`'s own background transparent (via
		// --studio-app-bg-color, consumed in studio-overrides.css) so that video
		// shows through it. The "Darken/tint strength" overlay gradient
		// (applyOverlayVar() below) still paints on top of `.app-contents`
		// regardless of what's behind it, so that slider keeps working unmodified
		// for video too.
		root.removeProperty('--studio-bg-image')
		root.setProperty('--studio-app-bg-color', 'transparent')
	} else {
		root.removeProperty('--studio-bg-image')
		root.removeProperty('--studio-app-bg-color')
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
// the modal background. This used to be tied to a user-facing "modal
// opacity"/"background transparency" slider, but that meant the one control
// simultaneously changed both how dark surfaces looked *and* how much of the
// background image was revealed through them — dragging what looked like a
// "darkness" slider could visibly brighten the screen if the image itself
// was bright, since less-opaque panels just let more of it through. Split
// that apart: how much background shows through every surface is now a
// fixed constant (FIXED_SURFACE_ALPHA), and `surfaceDarkness` only ever
// controls how dark the surface's own color is tinted before that fixed
// transparency applies — a real "darkness" knob with no side effect on how
// much of the image bleeds through.
//
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

// How much background shows through every panel/button/card/modal while a
// custom background is active. Deliberately NOT user-controlled (see the
// comment above SURFACE_PROPERTIES) — 0.75 is subtle enough that surfaces
// still read clearly as their own panels rather than a pane of glass, while
// still letting the custom background meaningfully show through.
const FIXED_SURFACE_ALPHA = 0.75

function applySurfaceVars() {
	if (typeof document === 'undefined') return
	const root = document.documentElement
	const rootStyle = root.style

	if (state.backgroundMode === 'default') {
		for (const prop of SURFACE_PROPERTIES) rootStyle.removeProperty(prop)
		surfaceOriginals.clear()
		return
	}

	const alpha = FIXED_SURFACE_ALPHA
	// Blend the darken amount into the panel's own base color before making
	// it translucent, so raising "surface darkness" directly improves
	// contrast on every glass panel, not just the parts of the image visible
	// in the gaps between them. Capped at 50% black so a maxed-out slider
	// still leaves the surface's own hue recognizable rather than going flat
	// black.
	//
	// This entire darken step is skipped in light theme. It was written
	// assuming a dark theme, where panels start dark and white text needs
	// them to stay dark enough for contrast — pushing them further toward
	// black only helps. Light theme is the opposite: panels start light and
	// TEXT ITSELF is dark, so forcibly mixing panels toward black actively
	// destroys contrast instead of helping it — that's the "light theme is
	// unreadable with a custom background" bug. Light theme still gets the
	// fixed transparency below (so the custom background shows through), it
	// just isn't additionally darkened first.
	const isLightTheme = useTheme().active === 'light'
	const darkenAlpha = isLightTheme ? 0 : (state.surfaceDarkness / 100) * 0.5
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
	const alpha = state.surfaceDarkness / 100
	// Same theme-awareness as applySurfaceVars() above: tinting toward black
	// is a dark-theme assumption. In light theme, tint toward white instead
	// so the sidebar stays legible with light theme's dark text.
	const tint = useTheme().active === 'light' ? '255, 255, 255' : '0, 0, 0'
	root.setProperty('--brand-gradient-bg', `rgba(${tint}, ${(alpha * 0.35).toFixed(3)})`)
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
// Scaled by `glassBlurStrength` (0-20px, user-adjustable — see the Slider in
// StudioAppearanceSettings.vue) and watched narrowly on just that +
// backgroundMode, same pattern as applyModalOpacityVar()/popupOpacity below.
// This is NOT the FPS-drop-while-dragging bug from the comment on
// applyStudioAppearance() below — that came from one combined watcher
// re-running *every* apply function (including the expensive
// background-image re-decode) on every tick of an unrelated slider.
// applyGlassBlurVar() itself only ever sets this one CSS custom property, so
// it's just as cheap to scrub on every tick as applyModalOpacityVar() already
// is.
function applyGlassBlurVar() {
	if (typeof document === 'undefined') return
	const root = document.documentElement.style
	if (state.backgroundMode === 'default' || state.glassBlurStrength <= 0) {
		root.removeProperty('--studio-glass-blur')
		return
	}
	root.setProperty('--studio-glass-blur', `blur(${state.glassBlurStrength}px)`)
}

// Modrinth Studios addition: light/oled/retro were never actually going to
// work well with a custom background — the surface-darkening/tint logic
// above is fundamentally a dark-theme thing, and beyond that, Modrinth's own
// light theme uses solid (not translucent) selected-state backgrounds, so it
// doesn't compose with "see-through surfaces" at all. Rather than keep
// patching individual color mismatches one screenshot at a time, custom
// background mode now just locks the color theme to Dark outright — the one
// theme every other studio effect here was actually designed against — and
// hides the theme picker in Settings (see the `studio-theme-locked` class
// this toggles, consumed in studio-overrides.css) so there's nothing left to
// pick that would break it.
//
// `themeBeforeLock` remembers whatever the user's *real* theme choice was so
// switching back to the default background restores it, instead of leaving
// them stuck on Dark forever.
let themeBeforeLock: ColorTheme | null = null

function applyThemeLock() {
	if (typeof document === 'undefined') return
	const theme = useTheme()
	document.documentElement.classList.toggle(
		'studio-theme-locked',
		state.backgroundMode !== 'default',
	)

	if (state.backgroundMode === 'default') {
		if (themeBeforeLock !== null) {
			theme.preferred = themeBeforeLock
			themeBeforeLock = null
		}
		return
	}

	// Previews (an in-progress, unsaved pick in Settings) don't matter once
	// the picker is hidden — clear it so nothing lingers.
	theme.preview = null
	if (theme.preferred !== 'dark') {
		if (themeBeforeLock === null) themeBeforeLock = theme.preferred
		theme.preferred = 'dark'
	}
}

// Popups/modals (e.g. this very Settings window) get their own transparency
// slider, independent of `surfaceDarkness`/FIXED_SURFACE_ALPHA above, and —
// unlike those — it's not limited to when a custom background is active:
// making popups translucent is a reasonable look on its own, so this applies
// in every mode.
function applyModalOpacityVar() {
	if (typeof document === 'undefined') return
	document.documentElement.style.setProperty(
		'--studio-modal-opacity',
		String(state.popupOpacity / 100),
	)
}

/** Re-applies the saved custom window icon. Call once on app startup, and
 * again whenever the user picks a new one. This only changes the *running*
 * window/taskbar icon — the .exe's own embedded icon (what Explorer shows
 * before the app is even open) can only be changed by rebuilding the app
 * with new files under apps/app/icons/. */
export async function applyStudioWindowIcon() {
	if (!state.windowIconPath) return

	const myGeneration = ++iconApplyGeneration
	const isCurrent = () => myGeneration === iconApplyGeneration

	try {
		await getCurrentWindow().setIcon(state.windowIconPath)
	} catch (err) {
		console.error('[modrinth-studios] failed to apply custom window icon', err)
		return
	}

	// Modrinth Studios addition: push the same custom icon out to any pinned
	// shortcut too — see pushPinnedIcon()'s doc comment. The picker only
	// allows .png/.ico (StudioAppearanceSettings.vue), so there's no other
	// format to handle here. An already-.ico file is passed straight
	// through unwrapped — it may well already be multi-resolution itself,
	// which is strictly as good as or better than anything built here. A
	// .png gets resampled into the same multi-resolution set as the
	// generated accent icon (buildResizedIconSet()) rather than wrapped as a
	// single entry at its native size — a single entry has Windows scaling
	// that one image for every icon slot it needs, which looks pixelated
	// when upscaled or soft when downscaled; this is the exact "low res"
	// complaint the accent icon had before it got the same treatment.
	if (!isCurrent()) return
	try {
		const path = state.windowIconPath
		const rawBytes = await readFile(path)
		let icoBytes: Uint8Array
		if (path.toLowerCase().endsWith('.ico')) {
			icoBytes = rawBytes
		} else {
			const sizeSet = await buildResizedIconSet(rawBytes)
			// Falls back to a single native-size entry only if resizing
			// somehow produced nothing at all — still better than no pinned
			// icon whatsoever.
			icoBytes = wrapPngSetAsIco(
				sizeSet.length > 0 ? sizeSet : [{ size: getPngPixelSize(rawBytes), bytes: rawBytes }],
			)
		}
		if (!isCurrent()) return
		await pushPinnedIcon(icoBytes, isCurrent)
	} catch (err) {
		console.error('[modrinth-studios] failed to read custom icon for pinned shortcut', err)
	}
}

// Modrinth Studios addition: recolors the app's own ring mark — the same
// path data as packages/assets/branding/logo.svg — to the current accent
// color, for people who like a custom accent but haven't picked a fully
// custom window icon of their own. Rendered at runtime with Canvas rather
// than shipped as a second static asset, so it always tracks whatever
// accent color is active, including one picked mid-session.
const ACCENT_ICON_VIEWBOX = { width: 512, height: 514 }
const ACCENT_ICON_RING_PATHS = [
	'M503.16 323.56C514.55 281.47 515.32 235.91 503.2 190.76C466.57 54.2299 326.04 -26.8001 189.33 9.77991C83.8101 38.0199 11.3899 128.07 0.689941 230.47H43.99C54.29 147.33 113.74 74.7298 199.75 51.7098C306.05 23.2598 415.13 80.6699 453.17 181.38L411.03 192.65C391.64 145.8 352.57 111.45 306.3 96.8198L298.56 140.66C335.09 154.13 364.72 184.5 375.56 224.91C391.36 283.8 361.94 344.14 308.56 369.17L320.09 412.16C390.25 383.21 432.4 310.3 422.43 235.14L464.41 223.91C468.91 252.62 467.35 281.16 460.55 308.07L503.16 323.56Z',
	'M321.99 504.22C185.27 540.8 44.7501 459.77 8.11011 323.24C3.84011 307.31 1.17 291.33 0 275.46H43.27C44.36 287.37 46.4699 299.35 49.6799 311.29C53.0399 323.8 57.45 335.75 62.79 347.07L101.38 323.92C98.1299 316.42 95.39 308.6 93.21 300.47C69.17 210.87 122.41 118.77 212.13 94.7601C229.13 90.2101 246.23 88.4401 262.93 89.1501L255.19 133C244.73 133.05 234.11 134.42 223.53 137.25C157.31 154.98 118.01 222.95 135.75 289.09C136.85 293.16 138.13 297.13 139.59 300.99L188.94 271.38L174.07 231.95L220.67 184.08L279.57 171.39L296.62 192.38L269.47 219.88L245.79 227.33L228.87 244.72L237.16 267.79C237.16 267.79 253.95 285.63 253.98 285.64L277.7 279.33L294.58 260.79L331.44 249.12L342.42 273.82L304.39 320.45L240.66 340.63L212.08 308.81L162.26 338.7C187.8 367.78 226.2 383.93 266.01 380.56L277.54 423.55C218.13 431.41 160.1 406.82 124.05 361.64L85.6399 384.68C136.25 451.17 223.84 484.11 309.61 461.16C371.35 444.64 419.4 402.56 445.42 349.38L488.06 364.88C457.17 431.16 398.22 483.82 321.99 504.22Z',
]
// Close to the dark theme's --surface-1, not read from CSS: this needs a
// fixed, resolved color at generation time, not a variable the OS-level
// taskbar icon can't see.
const ACCENT_ICON_BG_COLOR = '#16181c'
const ACCENT_ICON_SIZE = 256

/** Renders the ring mark at `size`×`size`, straight from the vector path
 * data — every call draws fresh at its own resolution rather than scaling a
 * bitmap, so a 16px render is exactly as sharp as a 256px one. This matters
 * for `renderAccentIconSet()` below: packing several *natively*-rendered
 * sizes into one `.ico` is what actually fixes a generated icon looking
 * softer than the app's real (professionally multi-resolution-exported)
 * default — a single 256px image alone forces Windows to downscale it for
 * every smaller spot it's shown (a 16px taskbar icon, in particular), which
 * always looks softer than something rendered for that size directly. */
async function renderAccentIconPng(accentColor: string, size: number): Promise<Uint8Array | null> {
	if (typeof document === 'undefined') return null

	const canvas = document.createElement('canvas')
	canvas.width = size
	canvas.height = size
	const ctx = canvas.getContext('2d')
	if (!ctx) return null

	const radius = size / 2
	ctx.fillStyle = ACCENT_ICON_BG_COLOR
	ctx.beginPath()
	ctx.arc(radius, radius, radius, 0, Math.PI * 2)
	ctx.fill()

	const scale = Math.min(size / ACCENT_ICON_VIEWBOX.width, size / ACCENT_ICON_VIEWBOX.height)
	ctx.save()
	ctx.translate(
		(size - ACCENT_ICON_VIEWBOX.width * scale) / 2,
		(size - ACCENT_ICON_VIEWBOX.height * scale) / 2,
	)
	ctx.scale(scale, scale)
	ctx.fillStyle = accentColor
	for (const d of ACCENT_ICON_RING_PATHS) {
		ctx.fill(new Path2D(d))
	}
	ctx.restore()

	const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'))
	if (!blob) return null
	return new Uint8Array(await blob.arrayBuffer())
}

// Standard Windows icon sizes — covers the small taskbar/title-bar icon,
// alt-tab and alt-tab-hold (large), Start-menu/jump-list, and the
// full-size/high-DPI slot, without generating sizes nothing ever asks for.
// Shared by both the generated accent icon and a resized custom icon (see
// buildResizedIconSet() below) — same reasoning applies to both: one image
// stretched to fit every slot looks worse than art actually sized for it.
const PINNED_ICON_SIZES = [16, 24, 32, 48, 64, 128, 256] as const

/** Renders every size in `PINNED_ICON_SIZES` and returns them ready for
 * `wrapPngSetAsIco()`. Used only for the *pinned-shortcut* icon — the live
 * running-window icon (`setIcon()`) still gets a single `ACCENT_ICON_SIZE`
 * PNG, since Tauri's `setIcon()` takes one image, not a size set. */
async function renderAccentIconSet(
	accentColor: string,
): Promise<{ size: number; bytes: Uint8Array }[]> {
	const rendered = await Promise.all(
		PINNED_ICON_SIZES.map(async (size) => {
			const bytes = await renderAccentIconPng(accentColor, size)
			return bytes ? { size, bytes } : null
		}),
	)
	return rendered.filter((entry): entry is { size: number; bytes: Uint8Array } => entry !== null)
}

/** Resamples an arbitrary source PNG's bytes down (or up) to `size`×`size`,
 * center-cropped to a square if it isn't one already, with high-quality
 * smoothing enabled — used for a *custom* picked icon, which (unlike the
 * accent icon) has no vector source to redraw at each size, only a single
 * raster image to resample from. Returns null on any failure (an
 * undecodable file, no canvas/`createImageBitmap` support, etc.) rather
 * than throwing, so `buildResizedIconSet()` can just skip that one size. */
async function resizePngToSize(sourceBytes: Uint8Array, size: number): Promise<Uint8Array | null> {
	if (typeof document === 'undefined') return null
	try {
		const blob = new Blob([sourceBytes], { type: 'image/png' })
		const bitmap = await createImageBitmap(blob)
		const canvas = document.createElement('canvas')
		canvas.width = size
		canvas.height = size
		const ctx = canvas.getContext('2d')
		if (!ctx) return null
		ctx.imageSmoothingEnabled = true
		ctx.imageSmoothingQuality = 'high'

		// Cover-fit: fills the square, cropping any excess rather than
		// letterboxing — most custom icon images are already square, so this
		// only matters as a graceful fallback for ones that aren't.
		const scale = Math.max(size / bitmap.width, size / bitmap.height)
		const drawWidth = bitmap.width * scale
		const drawHeight = bitmap.height * scale
		ctx.drawImage(bitmap, (size - drawWidth) / 2, (size - drawHeight) / 2, drawWidth, drawHeight)
		bitmap.close()

		const outBlob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'))
		if (!outBlob) return null
		return new Uint8Array(await outBlob.arrayBuffer())
	} catch (err) {
		console.error(`[modrinth-studios] failed to resize custom icon to ${size}px`, err)
		return null
	}
}

/** Builds the same `PINNED_ICON_SIZES` set as `renderAccentIconSet()`, but
 * by resampling a custom picked PNG instead of redrawing vector art — see
 * `resizePngToSize()`. Sizes that fail to resize are just omitted; an empty
 * result means the caller should fall back to a single native-size entry
 * rather than an empty (and therefore useless) `.ico`. */
async function buildResizedIconSet(sourceBytes: Uint8Array): Promise<{ size: number; bytes: Uint8Array }[]> {
	const rendered = await Promise.all(
		PINNED_ICON_SIZES.map(async (size) => {
			const bytes = await resizePngToSize(sourceBytes, size)
			return bytes ? { size, bytes } : null
		}),
	)
	return rendered.filter((entry): entry is { size: number; bytes: Uint8Array } => entry !== null)
}

// Matches the dark theme's --color-green-500 (packages/assets/styles/
// variables.scss) — the stock ring color before any accent is applied.
// Resolved here as a fixed hex rather than read from CSS for the same
// reason as ACCENT_ICON_BG_COLOR above: this needs a concrete color at
// generation time, not a variable the OS-level taskbar icon can't see.
const DEFAULT_ICON_COLOR = '#1bd96a'

// Tracks whether this running session has ever actually overridden the
// window icon away from the exe's true built-in one. Only matters for the
// "turn tinting off again" case just below — on a fresh launch with no
// accent ever picked, this stays false and the function below never touches
// the icon at all, leaving the real default completely alone.
let hasAppliedGeneratedIcon = false

/** Recolors the app's own ring mark and applies it as the running
 * window/taskbar icon: to the current accent color while one is active, the
 * person hasn't picked a fully custom icon of their own (that always wins,
 * see applyStudioWindowIcon() above), and `tintDefaultIconWithAccent` is on
 * (Settings surfaces this toggle only once the other two conditions are
 * already true; it defaults to on) — or back to the stock green if any of
 * those stop being true *after* a tinted icon was already applied this
 * session, so turning the toggle off (or resetting the accent) visibly
 * reverts it immediately instead of leaving the last color stuck until a
 * relaunch. Call once on startup and again whenever the accent color,
 * custom icon, or this toggle changes. */
// Modrinth Studios addition: bumped at the start of every call to
// applyAccentIconTint() or applyStudioWindowIcon(), before any await —
// shared between the two since only one of them ever actually does
// anything at a time (applyAccentIconTint() bails immediately once a custom
// icon is set), but a single counter keeps both safe from each other too,
// not just from themselves. state.accentColor in particular can change many
// times a second while someone drags the color picker, and each change
// re-enters applyAccentIconTint(); without a guard, an older call resuming
// after a newer one has already started could call setIcon() on a file the
// newer call's own cleanup step just deleted (both write to
// `generated-icon-<timestamp>.png` and remove every *other* file with that
// prefix) — surfacing as a spurious "file not found" error, and briefly
// fighting the newer call for which icon actually ends up applied. Each
// call captures its own generation number and re-checks it's still current
// after every await; a superseded call just quietly stops instead.
let iconApplyGeneration = 0

/** Pushes `icoBytes` out to any taskbar/Start-menu shortcut pointing at this
 * app — see `studio_pinned_icon_windows.rs` for why that's needed (a pinned
 * shortcut shows its own icon, independent of the running window's
 * `setIcon()`) and why it's safe (never touches the installed exe). Shared
 * by both `applyAccentIconTint()` and `applyStudioWindowIcon()`, since
 * whichever one actually changes the running window's icon needs to push
 * the same result out to a pinned shortcut too. Best effort and
 * Windows-only (a no-op elsewhere) — failures here shouldn't affect the
 * live in-app icon change that already succeeded, so callers wrap this in
 * their own try and don't let it interrupt anything. `isCurrent` is the
 * caller's own generation check, re-tested here too since reading the file/
 * rendering the PNG the caller passes in already involved an await. */
async function pushPinnedIcon(icoBytes: Uint8Array, isCurrent: () => boolean) {
	if (!isCurrent()) return
	try {
		const summary = await invoke<string>('plugin:studio|studio_apply_pinned_icon', {
			bytes: Array.from(icoBytes),
		})
		// Always logged, success or not — the only way to see whether a
		// pinned shortcut was actually found/matched/updated without a
		// debugger attached to the Rust side. See studio_pinned_icon_windows.rs.
		console.log('[modrinth-studios] pinned icon apply:', summary)
	} catch (err) {
		console.error('[modrinth-studios] failed to apply pinned shortcut icon', err)
	}
}

export async function applyAccentIconTint() {
	if (state.windowIconPath) return

	const shouldTint = Boolean(state.accentColor) && state.tintDefaultIconWithAccent
	if (!shouldTint && !hasAppliedGeneratedIcon) return

	const myGeneration = ++iconApplyGeneration
	const isCurrent = () => myGeneration === iconApplyGeneration

	try {
		const color = shouldTint ? (state.accentColor as string) : DEFAULT_ICON_COLOR
		const bytes = await renderAccentIconPng(color, ACCENT_ICON_SIZE)
		if (!bytes || !isCurrent()) return
		const path = await invoke<string>('plugin:studio|studio_set_generated_app_icon', {
			bytes: Array.from(bytes),
		})
		if (!isCurrent()) return
		await getCurrentWindow().setIcon(path)
		hasAppliedGeneratedIcon = true

		if (!isCurrent()) return
		const iconSet = await renderAccentIconSet(color)
		if (!isCurrent()) return
		await pushPinnedIcon(wrapPngSetAsIco(iconSet), isCurrent)
	} catch (err) {
		console.error('[modrinth-studios] failed to apply accent-tinted app icon', err)
	}
}

/** Delays invoking `fn` until `waitMs` has passed with no further calls —
 * used only for the accent/toggle watcher below, since a color picker can
 * fire many changes a second and each `applyAccentIconTint()` run does a
 * canvas render, a Tauri round-trip, and (on Windows) a scan of every
 * taskbar/Start-menu shortcut. Startup and focus-change call sites still
 * call `applyAccentIconTint()` directly, unthrottled. */
function debounce<Args extends unknown[]>(
	fn: (...args: Args) => void,
	waitMs: number,
): (...args: Args) => void {
	let timeout: ReturnType<typeof setTimeout> | undefined
	return (...args: Args) => {
		if (timeout) clearTimeout(timeout)
		timeout = setTimeout(() => fn(...args), waitMs)
	}
}

/**
 * Packs one or more PNG images (each already at its own target size — see
 * `renderAccentIconSet()`) into a single, real multi-resolution `.ico` file:
 * a 6-byte ICONDIR header, one 16-byte ICONDIRENTRY per image, then every
 * image's bytes back to back. Windows shortcuts (`IconLocation`) need an
 * actual `.ico`/`.exe`/`.dll`, not a bare `.png` — but Vista+ also accepts a
 * PNG-encoded image directly as an ICO entry's payload (no legacy
 * BMP-in-ICO re-encoding needed), so this is pure wrapping, not
 * re-rendering. Packing multiple sizes in one file — rather than just the
 * one biggest render — is what lets Windows pick a size actually rendered
 * for a 16px taskbar slot instead of downscaling a 256px image into it,
 * which is what made an earlier, single-entry version of this look
 * noticeably softer than the app's own (professionally multi-resolution
 * exported) default icon.
 */
/** Reads a PNG's actual pixel size straight out of its `IHDR` chunk (bytes
 * 16–23: 8-byte signature, 4-byte chunk length, 4-byte "IHDR" type, then
 * big-endian width/height) — used only to fill in `wrapPngSetAsIco()`'s
 * width/height byte correctly for a user-picked custom icon, whose actual
 * resolution isn't something this code controls the way it does for the
 * generated accent icon. Square icons are assumed (standard for this
 * format); if the file is non-square or unreadable, falls back to 256,
 * which only affects which size Windows *thinks* this entry is — cosmetic,
 * never a correctness issue for a single-entry ICO. */
function getPngPixelSize(pngBytes: Uint8Array): number {
	if (pngBytes.length < 24) return 256
	const view = new DataView(pngBytes.buffer, pngBytes.byteOffset, pngBytes.byteLength)
	const width = view.getUint32(16, false)
	const height = view.getUint32(20, false)
	const size = Math.max(width, height)
	return size > 0 && size <= 256 ? size : 256
}

function wrapPngSetAsIco(entries: { size: number; bytes: Uint8Array }[]): Uint8Array {
	const HEADER_SIZE = 6
	const ENTRY_SIZE = 16
	const dirSize = HEADER_SIZE + ENTRY_SIZE * entries.length
	const totalSize = dirSize + entries.reduce((sum, entry) => sum + entry.bytes.length, 0)

	const ico = new Uint8Array(totalSize)
	const view = new DataView(ico.buffer)

	// ICONDIR: reserved(2)=0, type(2)=1 (icon), count(2)=entries.length
	view.setUint16(0, 0, true)
	view.setUint16(2, 1, true)
	view.setUint16(4, entries.length, true)

	let dataOffset = dirSize
	entries.forEach((entry, i) => {
		const entryOffset = HEADER_SIZE + i * ENTRY_SIZE
		// width/height: the ICO format encodes 256px as 0, not 256, since
		// each is a single byte and 256 doesn't fit.
		const sizeByte = entry.size >= 256 ? 0 : entry.size
		ico[entryOffset] = sizeByte // width
		ico[entryOffset + 1] = sizeByte // height
		ico[entryOffset + 2] = 0 // color count (0 = no palette, true color)
		ico[entryOffset + 3] = 0 // reserved
		view.setUint16(entryOffset + 4, 1, true) // color planes
		view.setUint16(entryOffset + 6, 32, true) // bits per pixel
		view.setUint32(entryOffset + 8, entry.bytes.length, true) // size of image data
		view.setUint32(entryOffset + 12, dataOffset, true) // offset of image data

		ico.set(entry.bytes, dataOffset)
		dataOffset += entry.bytes.length
	})

	return ico
}

// Modrinth Studios addition: multiple background images that rotate over
// time, rather than always a single fixed one. `backgroundImagePaths` is the
// full pool (a single picked file still populates it with one entry);
// `backgroundImagePath` (the field applyBackgroundVars() actually reads) is
// always whichever one of those is currently on screen.

function applyNewBackgroundPool(destPaths: string[]) {
	state.backgroundImagePaths = destPaths
	state.backgroundImagePath = destPaths[0] ?? null
	state.backgroundLastRotatedAt = Date.now()
	if (destPaths.length > 0) state.backgroundMode = 'image'
}

/** Copies one or more picked files into the app's config dir (so they're
 * inside the asset protocol's allowed scope) and replaces the background
 * pool with them. */
export async function setStudioBackgroundImages(sourcePaths: string[]) {
	const destPaths = await invoke<string[]>('plugin:studio|studio_set_background_images', {
		sourcePaths,
	})
	applyNewBackgroundPool(destPaths)
}

/** Same idea as `setStudioBackgroundImages`, but for every image file found
 * directly inside a picked folder (not recursive). */
export async function setStudioBackgroundFolder(folderPath: string) {
	const destPaths = await invoke<string[]>('plugin:studio|studio_set_background_folder', {
		folderPath,
	})
	applyNewBackgroundPool(destPaths)
}

// Modrinth Studios addition: a second, entirely separate pool for background
// *videos* — its own pick command, its own destination folder on the Rust
// side (`$APPCONFIG/studio/background-videos`, not `.../backgrounds`), and
// its own `backgroundVideoPath(s)` fields, so switching to/from Video mode
// and picking new files there can never delete or clobber a previously-set
// image pool (each pick command wipes and replaces only its own folder — see
// `fresh_video_batch_dir` vs `fresh_backgrounds_batch_dir` in
// apps/app/src/api/studio.rs). It shares `backgroundRotationMode` /
// `backgroundRotationInterval` / `backgroundLastRotatedAt` with the image
// pool below, though, since only one pool is ever actually active at a time
// (whichever `backgroundMode` currently points at) — there was no reason to
// duplicate a whole second set of rotation controls for it.

function applyNewVideoPool(destPaths: string[]) {
	state.backgroundVideoPaths = destPaths
	state.backgroundVideoPath = destPaths[0] ?? null
	state.backgroundLastRotatedAt = Date.now()
	if (destPaths.length > 0) state.backgroundMode = 'video'
}

/** Copies one or more picked video files into the app's config dir and
 * replaces the background video pool with them. */
export async function setStudioBackgroundVideos(sourcePaths: string[]) {
	const destPaths = await invoke<string[]>('plugin:studio|studio_set_background_videos', {
		sourcePaths,
	})
	applyNewVideoPool(destPaths)
}

/** Same idea as `setStudioBackgroundFolder`, but for every video file found
 * directly inside a picked folder (not recursive) — mirrors the Image tab's
 * "Choose folder..." button, which the Video tab was missing. */
export async function setStudioBackgroundVideoFolder(folderPath: string) {
	const destPaths = await invoke<string[]>('plugin:studio|studio_set_background_video_folder', {
		folderPath,
	})
	applyNewVideoPool(destPaths)
}

/** Whichever pool is actually live right now, based on `backgroundMode` —
 * lets rotation logic below stay written once instead of twice. */
function activePoolPaths(): string[] {
	return state.backgroundMode === 'video' ? state.backgroundVideoPaths : state.backgroundImagePaths
}

function activePoolCurrent(): string | null {
	return state.backgroundMode === 'video' ? state.backgroundVideoPath : state.backgroundImagePath
}

function setActivePoolCurrent(path: string) {
	if (state.backgroundMode === 'video') {
		state.backgroundVideoPath = path
	} else {
		state.backgroundImagePath = path
	}
}

// `session` is included (as a value the 30s timer check below can never
// reach) purely so this is a total mapping over the full union — keeps the
// lookup in maybeRotateOnSchedule() type-safe without needing a cast, since
// `state.backgroundRotationInterval` doesn't narrow just from the early
// `=== 'session'` return above (it's a reactive property access, not a
// plain local variable).
const ROTATION_INTERVAL_MS: Record<StudioBackgroundRotationInterval, number> = {
	session: Number.POSITIVE_INFINITY,
	'5m': 5 * 60_000,
	'15m': 15 * 60_000,
	'30m': 30 * 60_000,
	'1h': 60 * 60_000,
	'6h': 6 * 60 * 60_000,
	'1d': 24 * 60 * 60_000,
}

/** Moves to the next image/video in the active pool per
 * `backgroundRotationMode`. No-op with 0 or 1 entries — there's nothing to
 * rotate to. */
function rotateBackgroundImage() {
	const pool = activePoolPaths()
	if (pool.length <= 1) return

	const current: string | null = activePoolCurrent()

	if (state.backgroundRotationMode === 'random') {
		let next = Math.floor(Math.random() * pool.length)
		// Avoid visibly "rotating" to the exact same image again when there's
		// more than one to actually pick from.
		if (pool[next] === current) {
			next = (next + 1) % pool.length
		}
		setActivePoolCurrent(pool[next])
	} else {
		const currentIndex = current === null ? -1 : pool.indexOf(current)
		setActivePoolCurrent(pool[(currentIndex + 1) % pool.length])
	}

	state.backgroundLastRotatedAt = Date.now()
}

/** Rotates once if a timed interval is configured and due. Called on a
 * recurring cheap check (see `startBackgroundRotationScheduler`) rather than
 * scheduling a precise `setTimeout` for the configured interval — that would
 * need re-arming every time the interval, the pool, or the last-rotated time
 * changes, and wouldn't survive the app being asleep/suspended. Checking
 * elapsed real time against `backgroundLastRotatedAt` every 30s is simpler
 * and correct even across those. */
function maybeRotateOnSchedule() {
	// Video deliberately isn't handled here — see advanceVideoOnEnded() below.
	// A video naturally tells us exactly when it's done via its own `ended`
	// event, which is a much better rotation trigger than an arbitrary timer
	// (imagine a 20-minute video on a "every 5 minutes" interval, cut off
	// mid-playback). The image pool has no equivalent signal, which is why it
	// still needs this timer-based approach.
	if (state.backgroundMode !== 'image') return
	if (state.backgroundImagePaths.length <= 1) return
	if (state.backgroundRotationInterval === 'session') return

	const intervalMs = ROTATION_INTERVAL_MS[state.backgroundRotationInterval]
	if (Date.now() - state.backgroundLastRotatedAt >= intervalMs) {
		rotateBackgroundImage()
	}
}

let rotationSchedulerStarted = false
function startBackgroundRotationScheduler() {
	if (rotationSchedulerStarted || typeof window === 'undefined') return
	rotationSchedulerStarted = true
	setInterval(maybeRotateOnSchedule, 30_000)
}

/** Rotates once, meant to be called exactly once per app launch (see
 * `main.js`) — deliberately NOT inside `applyStudioAppearance()`, since that
 * also re-runs on other things like a theme change, which must never count
 * as a new "session". No-op unless rotation is actually set to `'session'`. */
export function initializeStudioBackgroundRotation() {
	startBackgroundRotationScheduler()
	if (state.backgroundRotationInterval !== 'session') return
	if (state.backgroundMode !== 'image') return
	if (state.backgroundImagePaths.length <= 1) return
	rotateBackgroundImage()
}

/** Advances the background video pool to its next entry per
 * `backgroundRotationMode` ('loop' cycles through in order, 'random' picks
 * one of the others) — meant to be called from StudioVideoBackground.vue's
 * `ended` handler, i.e. whenever the currently-showing clip actually
 * finishes, rather than on any kind of timer. No-op with 0 or 1 videos. */
export function advanceVideoOnEnded() {
	if (state.backgroundMode !== 'video') return
	if (state.backgroundVideoPaths.length <= 1) return
	rotateBackgroundImage()
}

/** Copies `sourcePath` into the app's config dir (same reasoning as
 * `setStudioBackgroundImages`: the webview can only load images from inside
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

/** Copies `path` into the app's config dir (same asset-protocol-scope
 * reasoning as `setStudioWindowIcon`) and saves it as the startup/splash
 * screen background. Passing `null` resets to the default cube artwork —
 * this only clears the saved preference, it doesn't need to touch any file,
 * since SplashScreen.vue just falls back to its built-in background whenever
 * this is unset. */
export async function setStudioSplashBackground(path: string | null) {
	if (path) {
		const destPath = await invoke<string>('plugin:studio|studio_set_splash_background', {
			sourcePath: path,
		})
		state.splashBackgroundPath = destPath
	} else {
		state.splashBackgroundPath = null
	}
}

/** Applies every studio appearance effect. Call once on startup; after that,
 * the targeted watchers below keep things in sync without redoing
 * unaffected work on every keystroke/slider tick. */
export function applyStudioAppearance() {
	applyThemeLock()
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
// `immediate: true` so a custom background restored from a previous session
// (state is loaded synchronously above, before this runs) locks the theme
// right away on startup too, not just on the next change made this session.
watch(() => state.backgroundMode, applyThemeLock, { immediate: true })
watch(
	[
		() => state.backgroundMode,
		() => state.backgroundImagePath,
		() => state.backgroundVideoPath,
		() => state.gradientFrom,
		() => state.gradientTo,
		() => state.gradientAngle,
	],
	applyBackgroundVars,
)
watch([() => state.backgroundMode, () => state.backgroundOverlay], applyOverlayVar)
watch(() => state.popupOpacity, applyModalOpacityVar)
watch([() => state.backgroundMode, () => state.surfaceDarkness], applySurfaceVars)
watch([() => state.backgroundMode, () => state.surfaceDarkness], applySidebarTintVar)
watch([() => state.backgroundMode, () => state.glassBlurStrength], applyGlassBlurVar)
watch(
	[() => state.accentColor, () => state.windowIconPath, () => state.tintDefaultIconWithAccent],
	debounce(applyAccentIconTint, 200),
)
// Modrinth Studios addition: applySurfaceVars() captures each surface
// variable's *computed* color once (surfaceOriginals) the first time it
// runs, then keeps tinting/fading that same cached color forever — it never
// re-reads the theme afterwards. That's fine as long as the underlying
// theme doesn't change, but switching Modrinth's own built-in theme
// (Settings → Appearance → dark/light/oled/retro) while a custom background
// was active had no visible effect on any of these surfaces, because they
// were still being computed from the OLD theme's cached colors. Clearing
// the cache and recomputing whenever the active theme changes fixes that.
watch(
	() => useTheme().active,
	() => {
		surfaceOriginals.clear()
		applyStudioAppearance()
	},
)
watch(state, schedulePersist, { deep: true })

export function useStudioAppearance() {
	return state
}
