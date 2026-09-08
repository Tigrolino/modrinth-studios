<template>
	<Transition name="splash-fade" @after-leave="onAfterLeave">
		<!-- Modrinth Studios: upstream made this screen properly light/dark
		     theme-aware (`${theme.active}-mode`, with its own light cube
		     artwork and tint colors). Studio still hardcodes `dark` here on
		     purpose — see the `.splash-screen.dark` rule below — so this
		     screen looks the same regardless of the person's theme setting;
		     `useTheme()` was intentionally not pulled in for that reason. -->
		<div v-if="!doneLoading || splashPreviewActive" class="splash-screen dark">
			<div class="app-logo-wrapper" data-tauri-drag-region>
				<!--
					Modrinth Studios addition: this used to be a one-off, hand-exported
					"modrinth app" wordmark SVG (icon + "modrinth" text + separately
					drawn "app" letterforms). Reusing the shared TextLogo component
					(icon + "modrinth" text only, same artwork, already used elsewhere
					in the app) instead of maintaining a second copy of that vector
					art, and appending "Studios" as plain text next to it — much
					easier to keep in sync than hand-drawn vector letters, and it
					already respects --color-brand (the accent color) automatically
					since TextLogo's icon uses that variable for its fill.
				-->
				<div class="app-logo-row">
					<TextLogo class="app-logo" color="var(--color-contrast)" />
					<span class="app-logo-suffix">Studios</span>
				</div>
				<ProgressBar class="loading-bar" :progress="Math.min(loadingProgress, 100)" />
				<span v-if="message">{{ message }}</span>
			</div>
			<div class="gradient-bg" data-tauri-drag-region></div>
			<video
				v-if="isVideoBackground"
				key="splash-video-bg"
				class="cube-bg splash-video-bg has-custom-bg"
				:src="customBackgroundSrc"
				autoplay
				muted
				loop
				disablepictureinpicture
				playsinline
			></video>
			<!-- Modrinth Studios: a `key` on both branches forces Vue to fully
			     replace this element (not patch it in place) whenever the mode
			     switches, so a stale <video> element left over from a previous
			     pick never lingers with the wrong `src`/no styling applied. -->
			<div
				v-else
				key="splash-image-bg"
				class="cube-bg"
				:class="{ 'has-custom-bg': !!appearance.splashBackgroundPath }"
				:style="customBackgroundStyle"
			></div>
			<div class="base-bg"></div>
		</div>
	</Transition>
</template>

<script setup>
import { injectLoadingState, TextLogo } from '@modrinth/ui'
import { convertFileSrc } from '@tauri-apps/api/core'
import { computed, ref, watch } from 'vue'

import ProgressBar from '@/components/ui/ProgressBar.vue'
import { useAppEvent } from '@/composables/use-app-event'
import { SPLASH_PREVIEW_DURATION_MS, splashPreviewActive } from '@/composables/use-splash-preview'
import { useStudioAppearance } from '@/composables/use-studio-appearance'

const doneLoading = ref(false)
const loadingProgress = ref(0)
const message = ref()

// Modrinth Studios addition: a user-picked replacement for the default cube
// artwork below (see .cube-bg's `background` in the style block, which stays
// as the fallback whenever no custom image is set). Set inline rather than
// through another CSS custom property because it needs `cover`/`center`
// sizing tuned differently than the default's `contain`/tiled-looking cube
// image — an arbitrary photo shouldn't be forced into the same treatment.
// A GIF needs none of this — an animated GIF used as a plain CSS
// `background-image` already plays and loops on its own, so it flows through
// this exact same path as any other still image with no special-casing.
const appearance = useStudioAppearance()

const VIDEO_EXTENSIONS = ['mp4', 'webm', 'mov', 'm4v']

function extensionOf(path) {
	return path.split(/[/\\]/).pop()?.split('.').pop()?.toLowerCase()
}

const isVideoBackground = computed(() => {
	const path = appearance.splashBackgroundPath
	return !!path && VIDEO_EXTENSIONS.includes(extensionOf(path) ?? '')
})

const customBackgroundSrc = computed(() =>
	appearance.splashBackgroundPath ? convertFileSrc(appearance.splashBackgroundPath) : undefined,
)

const customBackgroundStyle = computed(() => {
	if (!appearance.splashBackgroundPath || isVideoBackground.value) return {}
	return {
		backgroundImage: `url("${customBackgroundSrc.value}")`,
		backgroundSize: 'cover',
		backgroundPosition: 'center',
	}
})

// Modrinth Studios addition: "Replay startup animation" in Settings →
// Appearance re-shows this component briefly so a custom accent color or
// startup background can be previewed without restarting the whole app. That
// button just flips `splashPreviewActive` (see use-splash-preview.ts) — it
// never touches the real loading-state provider below, so it can't be
// confused with (or interfere with) an actual app load. When it's not tied
// to a real load, the progress bar has nothing meaningful to show, so this
// just runs it as a flat few-second fill purely for looks.
watch(splashPreviewActive, (active) => {
	if (!active) return
	message.value = undefined
	loadingProgress.value = 0
	const start = Date.now()
	const tick = () => {
		if (!splashPreviewActive.value) return
		const elapsed = Date.now() - start
		loadingProgress.value = Math.min(100, (elapsed / SPLASH_PREVIEW_DURATION_MS) * 100)
		if (elapsed < SPLASH_PREVIEW_DURATION_MS) setTimeout(tick, 30)
	}
	tick()
})

const MIN_DISPLAY_MS = 500
const mountedAt = Date.now()

const loading = injectLoadingState()

function onAfterLeave() {
	loading.setEnabled(true)
}

watch(
	[loading.barEnabled, loading.pending],
	([barEnabled, pending]) => {
		if (barEnabled) {
			return
		}

		if (pending) {
			loadingProgress.value = 0
			fakeLoadingIncrease()
			return
		}

		const elapsed = Date.now() - mountedAt
		const delay = Math.max(0, MIN_DISPLAY_MS - elapsed)

		setTimeout(() => {
			if (loading.pending.value) {
				return
			}
			doneLoading.value = true
		}, delay)
	},
	{ immediate: true },
)

function fakeLoadingIncrease() {
	if (loadingProgress.value < 95) {
		setTimeout(() => {
			loadingProgress.value += 2
			fakeLoadingIncrease()
		}, 5)
	}
}

useAppEvent('loading', (e) => {
	if (e.event.type === 'directory_move') {
		loadingProgress.value = 100 * (e.fraction ?? 1)
		message.value = 'Updating app directory...'
	}
})
</script>

<style scoped lang="scss">
// Modrinth Studios addition: "modrinth" isn't real text at all — it's a
// hand-traced vector drawing of letters (see TextLogo.vue), not rendered
// with any font. There's no font setting on it to copy for "Studios"; the
// two are fundamentally different things (a picture of letters vs. actual
// text). Inter (the app's normal UI font, used everywhere else) is a
// grotesque sans with fairly narrow, rectangular letter bowls, which is
// most of why plain Inter text next to the wordmark looked off — the
// wordmark's 'o'/'d' bowls are drawn much rounder/more circular. Poppins is
// a genuinely geometric sans (near-circular bowls) that reads much closer
// to that style. It's still not a pixel match — there's no way to get one
// without hand-tracing new vector letters in the same style as the logo —
// but it's a real improvement over generic system sans-serif.
@import url('https://fonts.googleapis.com/css2?family=Poppins:wght@500;600&display=swap');

.splash-screen {
	position: fixed;
	inset: 0;
	z-index: 10000;

	--splash-cube-image: url('@/assets/loading/cube.png');

	&.light-mode {
		--splash-cube-image: url('@/assets/loading/cube-light.webp');
	}
}

// Modrinth Studios addition: this component hardcodes the `dark` class
// (see the template) so the splash always looks the same regardless of the
// user's light/dark theme setting. Problem: variables.scss's `.dark` theme
// block also redeclares --color-brand back to plain green on this same
// element, which shadows whatever accent color was inherited from <html>
// (a CSS custom property declared directly on an element always wins over
// an inherited one, regardless of how the ancestor's declaration compares
// in specificity). --studio-brand-override is a bridge variable set by
// use-studio-appearance.ts specifically so this component can restore the
// accent color after the theme block resets it. Needs .splash-screen.dark
// (not just .dark) so this is specific enough to actually win over that
// theme rule.
.splash-screen.dark {
	--color-brand: var(--studio-brand-override, var(--color-green));
	// See the comment on .gradient-bg's `background` above: keeps the top
	// tint following the accent color instead of upstream's plain green.
	--splash-tint-top: color-mix(in srgb, var(--color-brand) 45%, transparent);
}

.splash-fade-leave-active {
	transition: opacity 0.3s ease-in-out;
}

.splash-fade-leave-to {
	opacity: 0;
}

.app-logo-wrapper {
	position: absolute;
	top: 0;
	left: 0;
	height: 100vh;
	width: 100%;

	display: flex;
	flex-direction: column;
	justify-content: center;
	align-items: center;

	gap: 1rem;
	color: var(--color-contrast);

	z-index: 9998;
}

.app-logo-row {
	display: flex;
	justify-content: center;
	// Bottom-align the two boxes rather than center them: a lowercase
	// wordmark's actual letters sit near the bottom of its SVG's bounding
	// box (the icon mark above it is taller), so centering the boxes was
	// pushing "Studios" noticeably lower than "modrinth"'s letters. This
	// gets both baselines close without needing a hand-tuned offset.
	align-items: flex-end;
	gap: 0.5rem;
}

.app-logo {
	height: 2.25rem;
	width: fit-content;
}

// Modrinth Studios addition: the "Studios" suffix that replaces the old
// hand-drawn "app" wordmark letters spelling out "modrinth app". Sized to
// actually match "modrinth" — measured the wordmark's rendered artwork
// directly (its lowercase letters only fill about 70% of the logo's total
// height; the rest is the icon mark, which is taller) rather than eyeballing
// a font-size next to it, which is what made the first pass look tiny.
// Colored with the accent color like the original "app" text was (now that
// --studio-brand-override actually gets it right, see below).
.app-logo-suffix {
	font-family: 'Poppins', var(--font-standard);
	font-size: 2rem;
	font-weight: 500;
	color: var(--color-brand);
	line-height: 1;
	white-space: nowrap;
}

.loading-bar {
	max-width: 20rem;
}

.gradient-bg {
	position: absolute;
	height: 100vh;
	width: 100vw;
	// Modrinth Studios addition: --splash-tint-top used to be a hardcoded
	// green wash (rgba(66, 131, 92, ...)) regardless of the user's accent
	// color. Overridden below (see .splash-screen.dark) to derive from
	// --color-brand via color-mix so it follows the accent color like the
	// rest of the app. --color-brand is already applied to the document
	// root synchronously before this component ever mounts (see
	// applyStudioAppearance() in main.js), so this is correct on the very
	// first frame, not just after settings load.
	background:
		linear-gradient(180deg, var(--splash-tint-top) 0%, var(--splash-tint-bottom) 97.29%),
		linear-gradient(0deg, var(--splash-overlay), var(--splash-overlay));
	z-index: 9997;
}

.cube-bg {
	position: absolute;

	left: 50%;
	top: 50%;
	transform: translate(-50%, -50%);

	width: 180vw;
	height: 180vh;
	background-color: var(--color-bg);

	z-index: 9996;

	&::after {
		content: '';
		position: absolute;
		inset: 0;
		background: var(--splash-cube-image) center no-repeat;
		background-size: contain;
		opacity: var(--splash-cube-opacity);
		mix-blend-mode: var(--splash-cube-blend);
	}

	// Modrinth Studios addition: upstream's default cube artwork above is an
	// unconditional ::after overlay — it has no concept of a user-picked
	// background, so it was painting itself on top of any custom splash
	// image/video/GIF regardless. Suppress it whenever a custom background is
	// actually in effect (see `has-custom-bg`, toggled in the template based
	// on `appearance.splashBackgroundPath`).
	&.has-custom-bg::after {
		opacity: 0;
	}
}

// Modrinth Studios addition: overrides .cube-bg's sizing (above) for the
// video-background case. That base rule oversizes to 180vw/180vh and
// transform-centers itself — tuned for `background-size: contain` on the
// default cube artwork / a picked still image, not for a <video> element,
// which should just plainly cover the full screen edge-to-edge instead.
.splash-video-bg {
	position: absolute;
	inset: 0;
	left: 0;
	top: 0;
	transform: none;
	width: 100%;
	height: 100%;
	object-fit: cover;
	background: none;
}

.base-bg {
	position: absolute;
	top: 0;
	left: 0;
	width: 100%;
	height: 100%;
	background: var(--color-bg);
	z-index: 9995;
}
</style>
