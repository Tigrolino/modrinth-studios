<!--
	Modrinth Studios addition: the actual pixels for Video background mode
	(Settings → Appearance → Background → Video). Rendered as a real,
	fixed, negative-z-index <video> sitting behind the entire app — a plain
	CSS `background-image` (how the Image/Gradient modes work, see
	--studio-bg-image in use-studio-appearance.ts) has no way to play video at
	all, so this needed its own element rather than fitting into that same
	mechanism. `.app-contents`'s own background is made transparent
	specifically in Video mode (--studio-app-bg-color, studio-overrides.css)
	so this shows through it; the existing "Darken/tint strength" overlay
	gradient still paints on top of that transparent layer regardless, so it
	keeps working unmodified for video too.

	Earlier version of this also supported a "boomerang" (forward, then
	reverse, then forward again) playback style, faked by manually
	seeking `currentTime` backward since no browser can actually decode video
	in reverse. Dropped it — every seek has to decode forward from the video's
	last keyframe first, so how smooth (or not) that ever looked depended
	entirely on the source file's own keyframe spacing, not anything this
	component could control, and it read as janky/broken more often than not.
	Plain looping only now.
-->
<template>
	<video
		v-if="show"
		ref="videoEl"
		class="studio-video-bg"
		:src="src"
		:loop="state.backgroundVideoPaths.length <= 1"
		preload="auto"
		autoplay
		muted
		disablepictureinpicture
		playsinline
		@ended="onEnded"
	></video>
</template>

<script setup>
import { convertFileSrc } from '@tauri-apps/api/core'
import { computed, useTemplateRef, watch } from 'vue'

import { advanceVideoOnEnded, useStudioAppearance } from '@/composables/use-studio-appearance'

const state = useStudioAppearance()
const videoEl = useTemplateRef('videoEl')

const show = computed(() => state.backgroundMode === 'video' && !!state.backgroundVideoPath)
const src = computed(() =>
	state.backgroundVideoPath ? convertFileSrc(state.backgroundVideoPath) : undefined,
)

// Modrinth Studios addition: with only one video picked, the native `loop`
// attribute above just handles everything and this never fires (a looping
// media element never emits `ended`). With more than one, `loop` is bound to
// `false` instead specifically so `ended` *does* fire once each clip
// finishes — that's the actual rotation trigger for Loop/Random mode (see
// advanceVideoOnEnded()), rather than any kind of timer. Once it advances,
// `src` changes below, which reloads and plays the next clip; since `loop`
// stays bound to the same (still > 1) pool length, that one will fire
// `ended` too, and so on indefinitely.
function onEnded() {
	advanceVideoOnEnded()
}

// Changing a <video>'s `src` alone doesn't reliably reload it in every
// engine — the HTML media-element load algorithm needs an explicit `load()`
// call to actually pick up the new source, otherwise it can keep showing (or
// trying to play) the previous file. Matters here once there's more than one
// video in the pool and it rotates.
watch(src, async () => {
	const el = videoEl.value
	if (!el) return
	el.load()
	try {
		await el.play()
	} catch {
		// Autoplay can be rejected in some contexts (e.g. no user interaction
		// yet) — nothing more to do here, it'll just sit on its first frame.
	}
})
</script>

<style scoped>
.studio-video-bg {
	position: fixed;
	inset: 0;
	width: 100%;
	height: 100%;
	object-fit: cover;
	z-index: -1;
	pointer-events: none;
}
</style>
