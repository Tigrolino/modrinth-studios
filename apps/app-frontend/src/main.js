import 'floating-vue/dist/style.css'
import 'overlayscrollbars/overlayscrollbars.css'
// Modrinth Studios addition, see the file itself for why this is safe to
// import unconditionally (it's a no-op until the user changes a setting).
import '@/assets/styles/studio-overrides.css'

import { VueQueryPlugin } from '@tanstack/vue-query'
import FloatingVue from 'floating-vue'
import { createApp } from 'vue'

import App from '@/App.vue'
import {
	applyAccentIconTint,
	applyStudioAppearance,
	applyStudioWindowIcon,
	initializeStudioBackgroundRotation,
} from '@/composables/use-studio-appearance'
import { overlayScrollbarsDirective } from '@/directives/overlayScrollbars'
import { setupErrorReporting } from '@/helpers/error-reporting'
import i18nPlugin from '@/plugins/i18n'
import i18nDebugPlugin from '@/plugins/i18n-debug'
import router from '@/routes'

// Modrinth Studios addition: apply saved accent color / background / modal
// opacity / window icon as early as possible.
applyStudioAppearance()
void applyStudioWindowIcon()
// Only actually does anything if a custom accent is active and no fully
// custom icon has been picked — see the function's own comment.
void applyAccentIconTint()
// Separate from applyStudioAppearance() on purpose — that also re-runs on
// things like a theme change, which must never count as a new "session" for
// "once per session" background rotation. See the function's own comment.
initializeStudioBackgroundRotation()

const app = createApp(App)
setupErrorReporting(app, router)

app.use(VueQueryPlugin)
app.use(router)
app.use(FloatingVue, {
	themes: {
		'ribbit-popout': {
			$extend: 'dropdown',
			placement: 'bottom-end',
			instantMove: true,
			distance: 8,
		},
		'dismissable-prompt': {
			$extend: 'dropdown',
			placement: 'bottom-start',
		},
	},
})
app.use(i18nPlugin)
app.use(i18nDebugPlugin)
app.directive('overlay-scrollbars', overlayScrollbarsDirective)

async function mount() {
	if (import.meta.env.DEV && import.meta.env.VITE_VUE_SCAN === 'true') {
		const { VueScanPlugin } = await import('@taijased/vue-render-tracker')
		app.use(new VueScanPlugin({ enabled: true, showOverlay: true, log: false, playSound: false }))
	}
	app.mount('#app')
}

void mount()
