<!--
	Modrinth Studios addition: sits alongside (not inside) the shared
	AppearanceSettingsLayout from @modrinth/ui, so upstream theme/appearance
	work in that shared package never conflicts with this file. New file.
-->
<script setup lang="ts">
import { Button, injectNotificationManager, Slider, Toggle } from '@modrinth/ui'
import { open } from '@tauri-apps/plugin-dialog'
import { computed } from 'vue'

import {
	setStudioBackgroundImage,
	setStudioWindowIcon,
	type StudioBackgroundMode,
	useStudioAppearance,
} from '@/composables/use-studio-appearance'

import StudioColorSwatch from './StudioColorSwatch.vue'

const { handleError } = injectNotificationManager()
const state = useStudioAppearance()

const backgroundModes: { id: StudioBackgroundMode; label: string }[] = [
	{ id: 'default', label: 'Default' },
	{ id: 'image', label: 'Image' },
	{ id: 'gradient', label: 'Gradient' },
]

function pillClass(active: boolean) {
	return [
		'cursor-pointer rounded-full border border-solid px-3 py-1.5 text-sm font-semibold leading-5 transition-all duration-100 active:scale-[0.97]',
		active
			? 'border-brand bg-brand-highlight text-brand'
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

async function pickBackgroundImage() {
	const path = await open({
		multiple: false,
		filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp'] }],
	}).catch(() => null)
	if (!path || typeof path !== 'string') return
	await setStudioBackgroundImage(path).catch(handleError)
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
				<div>
					<h3 class="m-0 text-lg font-semibold text-contrast">Accent color</h3>
					<p class="m-0 mt-1">Replaces Modrinth's green accent throughout the app.</p>
				</div>
				<div class="flex items-center gap-2">
					<StudioColorSwatch v-model="accentColorModel" label="Accent" />
					<Button type="outlined" :disabled="!state.accentColor" @click="resetAccentColor">
						Reset
					</Button>
				</div>
			</div>

			<div v-if="state.accentColor" class="flex items-center justify-between gap-4">
				<div>
					<h3 class="m-0 text-lg font-semibold text-contrast">Keep update buttons green</h3>
					<p class="m-0 mt-1">
						Update buttons, the Beta tag, and other green "success" elements stay their original
						green instead of switching to your accent color.
					</p>
				</div>
				<Toggle id="studio-preserve-update-green" v-model="state.preserveUpdateGreen" />
			</div>

			<div>
				<h3 class="m-0 text-lg font-semibold text-contrast">Background</h3>
				<p class="m-0 mt-1">Show an image or gradient behind the app instead of a flat color.</p>
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

				<div v-if="state.backgroundMode === 'image'" class="flex items-center gap-2 mt-3">
					<Button type="outlined" @click="pickBackgroundImage">Choose image...</Button>
					<span v-if="state.backgroundImagePath" class="text-sm text-secondary truncate">
						{{ state.backgroundImagePath.split(/[/\\]/).pop() }}
					</span>
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

				<div v-if="state.backgroundMode !== 'default'" class="mt-3">
					<p class="m-0 mb-1 text-sm text-secondary">
						Darken/tint strength (higher fades the image toward the app's normal background, for
						readability)
					</p>
					<Slider v-model="state.backgroundOverlay" :min="0" :max="100" :step="5" unit="%" />
				</div>
			</div>

			<div>
				<h3 class="m-0 text-lg font-semibold text-contrast">Background transparency</h3>
				<p class="m-0 mt-1">
					Controls how see-through modal/popup backgrounds are, and — whenever a custom image or
					gradient background is active above — how much the rest of the app's panels and cards
					fade out so that background shows through everywhere, not just behind the main page. Has
					no effect while the background is set to Default.
				</p>
				<div class="mt-3 max-w-sm">
					<Slider v-model="state.modalOpacity" :min="10" :max="100" :step="5" unit="%" />
				</div>
			</div>

			<div>
				<h3 class="m-0 text-lg font-semibold text-contrast">App icon</h3>
				<p class="m-0 mt-1">
					Changes the window/taskbar icon while the app is running. Windows' own icon for the
					shortcut/.exe (what you see in File Explorer before opening the app) can only be changed
					by rebuilding the installer with new icon files.
				</p>
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
