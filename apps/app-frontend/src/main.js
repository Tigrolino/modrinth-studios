import 'floating-vue/dist/style.css'
import 'overlayscrollbars/overlayscrollbars.css'
// Modrinth Studios addition, see the file itself for why this is safe to
// import unconditionally (it's a no-op until the user changes a setting).
import '@/assets/styles/studio-overrides.css'

import * as Sentry from '@sentry/vue'
import { VueScanPlugin } from '@taijased/vue-render-tracker'
import { VueQueryPlugin } from '@tanstack/vue-query'
import FloatingVue from 'floating-vue'
import { createApp } from 'vue'

import App from '@/App.vue'
import {
	applyStudioAppearance,
	applyStudioWindowIcon,
	initializeStudioBackgroundRotation,
} from '@/composables/use-studio-appearance'
import { overlayScrollbarsDirective } from '@/directives/overlayScrollbars'
import i18nPlugin from '@/plugins/i18n'
import i18nDebugPlugin from '@/plugins/i18n-debug'
import router from '@/routes'

// Modrinth Studios addition: apply saved accent color / background / modal
// opacity / window icon as early as possible.
applyStudioAppearance()
void applyStudioWindowIcon()
// Separate from applyStudioAppearance() on purpose — that also re-runs on
// things like a theme change, which must never count as a new "session" for
// "once per session" background rotation. See the function's own comment.
initializeStudioBackgroundRotation()

const vueScan = new VueScanPlugin({
	enabled: false, // Enable or disable the tracker
	showOverlay: true, // Show overlay to visualize renders
	log: false, // Log render events to the console
	playSound: false, // Play sound on each render
})

let app = createApp(App)

Sentry.init({
	app,
	dsn: 'https://9508775ee5034536bc70433f5f531dd4@o485889.ingest.us.sentry.io/4504579615227904',
	integrations: [Sentry.browserTracingIntegration({ router })],
	tracesSampleRate: 0.1,
})

app.use(VueQueryPlugin)
app.use(vueScan)
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

app.mount('#app')
