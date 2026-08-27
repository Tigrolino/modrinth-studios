<!--
	Modrinth Studios addition: sits alongside (not inside) the shared
	AppearanceSettingsLayout from @modrinth/ui, so upstream theme/appearance
	work in that shared package never conflicts with this file. New file.
-->
<script setup lang="ts">
import {
	Button,
	Combobox,
	type ComboboxOption,
	injectNotificationManager,
	Slider,
	Toggle,
} from '@modrinth/ui'
import { open } from '@tauri-apps/plugin-dialog'
import { computed } from 'vue'

import { previewSplashScreen } from '@/composables/use-splash-preview'
import {
	setStudioBackgroundFolder,
	setStudioBackgroundImages,
	setStudioBackgroundVideos,
	setStudioSplashBackground,
	setStudioWindowIcon,
	type StudioBackgroundMode,
	type StudioBackgroundRotationInterval,
	useStudioAppearance,
} from '@/composables/use-studio-appearance'

import StudioColorSwatch from './StudioColorSwatch.vue'

const { handleError } = injectNotificationManager()
const state = useStudioAppearance()

const backgroundModes: { id: StudioBackgroundMode; label: string }[] = [
	{ id: 'default', label: 'Default' },
	{ id: 'image', label: 'Image' },
	{ id: 'video', label: 'Video' },
	{ id: 'gradient', label: 'Gradient' },
]

function pillClass(active: boolean) {
	return [
		'cursor-pointer rounded-full border border-solid px-3 py-1.5 text-sm font-semibold leading-5 transition-all duration-100 active:scale-[0.97]',
		active
			? // Modrinth Studios addition: text color falls back to --color-brand
				// (unchanged look for a normal accent) but swaps to a safe,
				// always-readable color when the accent is dark enough that its
				// own color would be invisible against this translucent
				// highlight background — see --studio-accent-safe-text in
				// use-studio-appearance.ts.
				'border-brand bg-brand-highlight text-[color:var(--studio-accent-safe-text,var(--color-brand))]'
			: 'border-surface-5 bg-surface-4 text-primary hover:bg-surface-5',
	]
}

const accentColorModel = computed({
	get: () => state.accentColor ?? '#1bd96a',
	set: (value: string) => {
		state.accentColor = value
	},
})

function resetAccentColor() {
	state.accentColor = null
}

const BACKGROUND_IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp']

// Modrinth Studios addition: one or more files, or every image in a picked
// folder — either becomes the background "pool" that rotates over time (see
// use-studio-appearance.ts). A single picked file still works exactly like
// before, just via the same plural command under the hood.
async function pickBackgroundImages() {
	const result = await open({
		multiple: true,
		filters: [{ name: 'Images', extensions: BACKGROUND_IMAGE_EXTENSIONS }],
	}).catch(() => null)
	if (!result) return
	const paths = Array.isArray(result) ? result : [result]
	if (paths.length === 0) return
	await setStudioBackgroundImages(paths).catch(handleError)
}

async function pickBackgroundFolder() {
	const result = await open({ directory: true }).catch(() => null)
	if (!result || typeof result !== 'string') return
	await setStudioBackgroundFolder(result).catch(handleError)
}

const BACKGROUND_VIDEO_EXTENSIONS = ['mp4', 'webm', 'mov', 'm4v']

// Modrinth Studios addition: one or more background videos — its own pool,
// entirely separate from the image one above (see the comment on
// applyNewVideoPool()/fresh_video_batch_dir in use-studio-appearance.ts /
// studio.rs for why they can't share storage). The actual pixels are
// rendered by StudioVideoBackground.vue, a real <video> fixed behind the
// whole app — CSS background-image can't play video.
async function pickBackgroundVideos() {
	const result = await open({
		multiple: true,
		filters: [{ name: 'Videos', extensions: BACKGROUND_VIDEO_EXTENSIONS }],
	}).catch(() => null)
	if (!result) return
	const paths = Array.isArray(result) ? result : [result]
	if (paths.length === 0) return
	await setStudioBackgroundVideos(paths).catch(handleError)
}

function rotationIntervalLabel(interval: StudioBackgroundRotationInterval): string {
	switch (interval) {
		case 'session':
			return 'Once per session'
		case '5m':
			return 'Every 5 minutes'
		case '15m':
			return 'Every 15 minutes'
		case '30m':
			return 'Every 30 minutes'
		case '1h':
			return 'Every hour'
		case '6h':
			return 'Every 6 hours'
		case '1d':
			return 'Once per day'
	}
}

const rotationIntervalOptions: ComboboxOption<StudioBackgroundRotationInterval>[] = (
	['session', '5m', '15m', '30m', '1h', '6h', '1d'] as const
).map((interval) => ({ value: interval, label: rotationIntervalLabel(interval) }))

// Modrinth Studios addition: lets a custom image, GIF, or video replace the
// default cube artwork shown while the app is loading (SplashScreen.vue),
// same "choose then reset" pattern as the app icon below. A GIF needs no
// special handling on top of a still image (an animated GIF used as a CSS
// background-image already plays/loops on its own) — only video is actually
// a distinct case, handled by SplashScreen.vue picking a <video> element
// instead of a background-image once it sees one of these extensions.
const SPLASH_BACKGROUND_EXTENSIONS = [
	...BACKGROUND_IMAGE_EXTENSIONS,
	'mp4',
	'webm',
	'mov',
	'm4v',
]

async function pickSplashBackground() {
	const path = await open({
		multiple: false,
		filters: [{ name: 'Images & videos', extensions: SPLASH_BACKGROUND_EXTENSIONS }],
	}).catch(() => null)
	if (!path || typeof path !== 'string') return
	await setStudioSplashBackground(path).catch(handleError)
}

async function resetSplashBackground() {
	await setStudioSplashBackground(null)
}

async function pickAppIcon() {
	const path = await open({
		multiple: false,
		filters: [{ name: 'Icons', extensions: ['png', 'ico'] }],
	}).catch(() => null)
	if (!path || typeof path !== 'string') return
	await setStudioWindowIcon(path).catch(handleError)
}

async function resetAppIcon() {
	await setStudioWindowIcon(null)
	// There's no API to "unset" a custom window icon back to the exe's
	// default at runtime, so this just stops us from re-applying our custom
	// one on the next launch — a full restart brings back the normal icon.
}
</script>

<template>
	<section class="mt-8 border-0 border-t border-solid border-divider pt-6">
		<h2 class="m-0 text-xl font-semibold text-contrast">Studio customization</h2>
		<p class="m-0 mt-1 text-secondary">
			Extra appearance options added in this fork. These live entirely outside Modrinth's own
			settings, so an upstream update won't touch them.
		</p>

		<div class="flex flex-col gap-6 mt-6">
			<div class="flex items-center justify-between gap-4">
				<h3 class="m-0 text-lg font-semibold text-contrast">Accent color</h3>
				<div class="flex items-center gap-2">
					<StudioColorSwatch v-model="accentColorModel" label="Accent" />
					<Button type="outlined" :disabled="!state.accentColor" @click="resetAccentColor">
						Reset
					</Button>
				</div>
			</div>

			<div v-if="state.accentColor" class="flex items-center justify-between gap-4">
				<h3 class="m-0 text-lg font-semibold text-contrast">Keep update buttons green</h3>
				<Toggle id="studio-preserve-update-green" v-model="state.preserveUpdateGreen" />
			</div>

			<div>
				<h3 class="m-0 text-lg font-semibold text-contrast">Background</h3>
				<p v-if="state.backgroundMode !== 'default'" class="m-0 mb-2 text-sm text-secondary">
					Color theme is locked to Dark while a custom background is active — switch back to
					Default to pick a different one.
				</p>
				<div class="flex items-center gap-1.5 mt-3">
					<button
						v-for="mode in backgroundModes"
						:key="mode.id"
						:class="pillClass(state.backgroundMode === mode.id)"
						@click="state.backgroundMode = mode.id"
					>
						{{ mode.label }}
					</button>
				</div>

				<div v-if="state.backgroundMode === 'image'" class="flex flex-col gap-3 mt-3">
					<div class="flex items-center gap-2 flex-wrap">
						<Button type="outlined" @click="pickBackgroundImages">Choose image(s)...</Button>
						<Button type="outlined" @click="pickBackgroundFolder">Choose folder...</Button>
						<span
							v-if="state.backgroundImagePaths.length === 1"
							class="text-sm text-secondary truncate"
						>
							{{ state.backgroundImagePaths[0].split(/[/\\]/).pop() }}
						</span>
						<span v-else-if="state.backgroundImagePaths.length > 1" class="text-sm text-secondary">
							{{ state.backgroundImagePaths.length }} images selected
						</span>
					</div>

					<div
						v-if="state.backgroundImagePaths.length > 1"
						class="flex flex-wrap items-center gap-4"
					>
						<div class="flex items-center gap-1.5">
							<button
								:class="pillClass(state.backgroundRotationMode === 'loop')"
								@click="state.backgroundRotationMode = 'loop'"
							>
								Loop
							</button>
							<button
								:class="pillClass(state.backgroundRotationMode === 'random')"
								@click="state.backgroundRotationMode = 'random'"
							>
								Random
							</button>
						</div>
						<Combobox
							:model-value="state.backgroundRotationInterval"
							:options="rotationIntervalOptions"
							trigger-type="base"
							class="!w-[12rem]"
							@update:model-value="
								(value: StudioBackgroundRotationInterval) =>
									(state.backgroundRotationInterval = value)
							"
						>
							<template #prefix>
								<span class="font-semibold text-secondary">Change</span>
							</template>
						</Combobox>
					</div>
				</div>

				<div
					v-else-if="state.backgroundMode === 'gradient'"
					class="flex flex-wrap items-center gap-3 mt-3"
				>
					<StudioColorSwatch v-model="state.gradientFrom" label="From" />
					<StudioColorSwatch v-model="state.gradientTo" label="To" />
					<label class="flex items-center gap-2 text-sm flex-1 min-w-[160px]">
						Angle
						<Slider v-model="state.gradientAngle" :min="0" :max="360" :step="5" unit="°" />
					</label>
				</div>

				<div v-else-if="state.backgroundMode === 'video'" class="flex flex-col gap-3 mt-3">
					<div class="flex items-center gap-2 flex-wrap">
						<Button type="outlined" @click="pickBackgroundVideos">Choose video(s)...</Button>
						<span
							v-if="state.backgroundVideoPaths.length === 1"
							class="text-sm text-secondary truncate"
						>
							{{ state.backgroundVideoPaths[0].split(/[/\\]/).pop() }}
						</span>
						<span v-else-if="state.backgroundVideoPaths.length > 1" class="text-sm text-secondary">
							{{ state.backgroundVideoPaths.length }} videos selected
						</span>
					</div>

					<div v-if="state.backgroundVideoPaths.length > 1" class="flex flex-col gap-1.5">
						<div class="flex items-center gap-1.5">
							<button
								:class="pillClass(state.backgroundRotationMode === 'loop')"
								@click="state.backgroundRotationMode = 'loop'"
							>
								Loop
							</button>
							<button
								:class="pillClass(state.backgroundRotationMode === 'random')"
								@click="state.backgroundRotationMode = 'random'"
							>
								Random
							</button>
						</div>
						<p class="m-0 text-sm text-secondary">
							{{
								state.backgroundRotationMode === 'loop'
									? 'Plays each video in order, moving to the next one as soon as the current one ends.'
									: 'Plays a random video, then picks another at random each time one ends.'
							}}
						</p>
					</div>
				</div>

				<div v-if="state.backgroundMode !== 'default'" class="mt-3">
					<p class="m-0 mb-1 text-sm text-secondary">Darken/tint strength</p>
					<Slider v-model="state.backgroundOverlay" :min="0" :max="100" :step="5" unit="%" />
				</div>
			</div>

			<div>
				<h3 class="m-0 text-lg font-semibold text-contrast">Popup transparency</h3>
				<div class="mt-3 max-w-sm">
					<Slider v-model="state.popupOpacity" :min="10" :max="100" :step="5" unit="%" />
				</div>
			</div>

			<div v-if="state.backgroundMode !== 'default'">
				<h3 class="m-0 text-lg font-semibold text-contrast">Surface darkness</h3>
				<div class="mt-3 max-w-sm">
					<Slider v-model="state.surfaceDarkness" :min="0" :max="100" :step="5" unit="%" />
				</div>
			</div>

			<div>
				<h3 class="m-0 text-lg font-semibold text-contrast">Startup screen background</h3>
				<p class="m-0 mb-3 text-sm text-secondary">
					Replaces the cube artwork shown while the app is loading. Accepts an image, a GIF, or a
					video (which loops for as long as loading takes).
				</p>
				<div class="flex items-center gap-2 flex-wrap">
					<Button type="outlined" @click="pickSplashBackground">Choose image or video...</Button>
					<Button
						type="outlined"
						:disabled="!state.splashBackgroundPath"
						@click="resetSplashBackground"
					>
						Reset to default
					</Button>
					<Button type="outlined" @click="previewSplashScreen">Replay startup animation</Button>
				</div>
			</div>

			<div>
				<h3 class="m-0 text-lg font-semibold text-contrast">App icon</h3>
				<div class="flex items-center gap-2 mt-3">
					<Button type="outlined" @click="pickAppIcon">Choose icon...</Button>
					<Button type="outlined" :disabled="!state.windowIconPath" @click="resetAppIcon">
						Reset (on next launch)
					</Button>
				</div>
			</div>
		</div>
	</section>
</template>
