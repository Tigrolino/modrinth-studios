// Modrinth Studios addition: lets Settings → Appearance replay the startup
// splash screen on demand (to preview a custom accent color / startup
// background pick without restarting the whole app), entirely separate from
// the real app-loading state machinery in @modrinth/ui's loading-state
// provider (LoadingStateProvider). Reusing that instead would mean firing
// fake begin()/end() load tokens through the app's real "is anything
// loading" tracker just to show an animation — keeping this isolated means
// the preview can never affect (or be affected by) genuine loading state.
import { ref } from 'vue'

/** SplashScreen.vue shows itself whenever this is true, in addition to its
 * normal "app hasn't finished loading yet" condition. */
export const splashPreviewActive = ref(false)

export const SPLASH_PREVIEW_DURATION_MS = 2500

let hideTimeout: ReturnType<typeof setTimeout> | undefined

/** Shows the splash screen again for a few seconds. */
export function previewSplashScreen() {
	if (hideTimeout) clearTimeout(hideTimeout)
	// Force it off first, even if a preview is already running — flipping
	// true again next tick restarts the fade-in and progress animation from
	// scratch instead of just extending whatever was already in progress.
	splashPreviewActive.value = false
	requestAnimationFrame(() => {
		splashPreviewActive.value = true
		hideTimeout = setTimeout(() => {
			splashPreviewActive.value = false
			hideTimeout = undefined
		}, SPLASH_PREVIEW_DURATION_MS)
	})
}
