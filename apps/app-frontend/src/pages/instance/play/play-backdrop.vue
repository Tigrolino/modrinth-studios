<script setup lang="ts">
/**
 * Modrinth Studios: artwork behind the Play mode screen. Cycles through the
 * instance's screenshots (or all screenshots when it has none) with a slow
 * crossfade; falls back to a blown-up blurred icon. Only used when no custom
 * app background is set (see layout.vue).
 */
import { useQuery } from '@tanstack/vue-query'
import { computed, onBeforeUnmount, ref, watch } from 'vue'

import { list_all_screenshots } from '@/helpers/instance'
import { instanceScreenshotsQueryOptions } from '@/pages/instance/query-options'

const props = defineProps<{ instanceId: string; iconSrc?: string | null }>()

const ROTATE_MS = 12_000

const instanceShots = useQuery(computed(() => instanceScreenshotsQueryOptions(props.instanceId)))
const allShots = useQuery({
	queryKey: ['screenshots', 'all-for-play-backdrop'],
	queryFn: list_all_screenshots,
	staleTime: 60_000,
	enabled: computed(() => instanceShots.isFetched.value && !(instanceShots.data.value ?? []).length),
})

const urls = computed(() => {
	const own = instanceShots.data.value ?? []
	const list = own.length ? own : (allShots.data.value ?? [])
	return [...list].sort((a, b) => b.modified_at - a.modified_at).map((s) => s.url)
})

const index = ref(0)
let timer: number | undefined
watch(
	urls,
	(list) => {
		window.clearInterval(timer)
		index.value = 0
		if (list.length > 1) {
			timer = window.setInterval(() => {
				index.value = (index.value + 1) % list.length
			}, ROTATE_MS)
		}
	},
	{ immediate: true },
)
onBeforeUnmount(() => window.clearInterval(timer))

const current = computed(() => urls.value[index.value] ?? null)
// Preload the next image so the crossfade doesn't flash.
watch(index, (i) => {
	const next = urls.value[(i + 1) % urls.value.length]
	if (next) new Image().src = next
})
</script>

<template>
	<div class="pointer-events-none absolute inset-0 -z-10 overflow-hidden" aria-hidden="true">
		<Transition name="backdrop-fade">
			<img
				v-if="current"
				:key="current"
				:src="current"
				alt=""
				class="backdrop-img absolute inset-0 h-full w-full object-cover opacity-90"
			/>
			<img
				v-else-if="iconSrc"
				key="icon"
				:src="iconSrc"
				alt=""
				class="absolute inset-0 h-full w-full scale-150 object-cover opacity-40 blur-3xl"
			/>
		</Transition>
		<div class="backdrop-shade absolute inset-0" />
	</div>
</template>

<style scoped>
.backdrop-img {
	animation: backdrop-drift 24s ease-in-out infinite alternate;
}
@keyframes backdrop-drift {
	from {
		transform: scale(1.06) translate(0, 0);
	}
	to {
		transform: scale(1.1) translate(-1%, -1%);
	}
}
.backdrop-fade-enter-active,
.backdrop-fade-leave-active {
	transition: opacity 1.5s ease;
}
.backdrop-fade-enter-from,
.backdrop-fade-leave-to {
	opacity: 0 !important;
}
.backdrop-shade {
	background:
		linear-gradient(
			to bottom,
			color-mix(in srgb, var(--color-bg) 55%, transparent) 0%,
			transparent 14%,
			transparent 40%,
			color-mix(in srgb, var(--color-bg) 55%, transparent) 62%,
			color-mix(in srgb, var(--color-bg) 92%, transparent) 80%,
			var(--color-bg) 95%
		),
		linear-gradient(
			to right,
			color-mix(in srgb, var(--color-bg) 70%, transparent) 0%,
			color-mix(in srgb, var(--color-bg) 15%, transparent) 40%,
			transparent 60%
		);
}
</style>
