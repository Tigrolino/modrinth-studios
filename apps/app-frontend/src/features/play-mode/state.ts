/**
 * Modrinth Studios: "Play mode". For people who mostly play one instance:
 * the Play button in the sidebar opens that instance as a big play screen
 * (tabs on top, large Play button, jump back in at the bottom), and the
 * library moves to its own sidebar button.
 *
 * Stored per device in localStorage.
 */
import { computed, reactive, ref, watch } from 'vue'

const STORAGE_KEY = 'studio-play-mode:v1'

type PlayModeState = {
	enabled: boolean
	instanceId: string | null
}

function load(): PlayModeState {
	try {
		const raw = localStorage.getItem(STORAGE_KEY)
		if (raw) {
			const parsed = JSON.parse(raw) as Partial<PlayModeState>
			return { enabled: !!parsed.enabled, instanceId: parsed.instanceId ?? null }
		}
	} catch {
		// ignore
	}
	return { enabled: false, instanceId: null }
}

const state = reactive<PlayModeState>(load())

watch(
	() => ({ ...state }),
	(value) => {
		try {
			localStorage.setItem(STORAGE_KEY, JSON.stringify(value))
		} catch {
			// ignore
		}
	},
	{ deep: true },
)

/**
 * True while the play-mode instance is shown in its play view (entered via the
 * Play button). Opening the same instance any other way (sidebar, library)
 * shows the normal instance page instead.
 */
const inPlayView = ref(false)

/** Play mode is on and an instance has been picked. */
const active = computed(() => state.enabled && !!state.instanceId)

export function usePlayMode() {
	return {
		state,
		active,
		inPlayView,
		/** Route of the play screen, or null when play mode is off. */
		playPath: computed(() =>
			active.value ? `/play/${encodeURIComponent(state.instanceId!)}` : null,
		),
		isPlayModeInstance: (id: string | null | undefined) => active.value && !!id && id === state.instanceId,
	}
}
