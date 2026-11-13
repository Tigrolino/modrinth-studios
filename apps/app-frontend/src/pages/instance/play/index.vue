<script setup lang="ts">
/**
 * Modrinth Studios: the "Play" tab of an instance, used as the home screen in
 * Play mode (features/play-mode). The big header with the Play button is
 * rendered by the instance layout; this page shows the overview below it and
 * the instance's recent worlds to jump back into.
 */
import { useQuery } from '@tanstack/vue-query'
import { computed } from 'vue'
import { useRouter } from 'vue-router'

import RecentWorldsList from '@/components/ui/world/RecentWorldsList.vue'
import {
	instanceContentQueryOptions,
	instancePlaytimeCorrectionQueryOptions,
	instanceScreenshotsQueryOptions,
	instanceWorldsQueryOptions,
} from '@/pages/instance/query-options'

import { injectInstancePage } from '../instance-context'

const { instance, instanceId } = injectInstancePage()

const contentQuery = useQuery(computed(() => instanceContentQueryOptions(instanceId.value)))
const worldsQuery = useQuery(computed(() => instanceWorldsQueryOptions(instanceId.value)))

const counts = computed(() => {
	const items = (contentQuery.data.value?.contentItems ?? []) as { project_type?: string }[]
	const by = (type: string) => items.filter((i) => (i.project_type ?? 'mod') === type).length
	return {
		mods: by('mod'),
		resourcepacks: by('resourcepack'),
		shaders: by('shader'),
		datapacks: by('datapack'),
		worlds: (worldsQuery.data.value as unknown[] | undefined)?.length ?? null,
	}
})

// Same total as the header: tracked playtime + the playtime correction.
const correctionQuery = useQuery(
	computed(() => ({
		...instancePlaytimeCorrectionQueryOptions(instanceId.value),
		enabled: !!instanceId.value,
	})),
)
const totalSeconds = computed(
	() =>
		instance.value.recent_time_played +
		instance.value.submitted_time_played +
		(correctionQuery.data.value ?? 0),
)

const recent = computed(() => [instance.value])

const router = useRouter()
const base = computed(() => `/play/${encodeURIComponent(instanceId.value)}`)

const screenshotsQuery = useQuery(computed(() => instanceScreenshotsQueryOptions(instanceId.value)))
const screenshots = computed(() =>
	[...(screenshotsQuery.data.value ?? [])].sort((a, b) => b.modified_at - a.modified_at),
)

const hours = computed(() => Math.floor(totalSeconds.value / 3600))
</script>

<template>
	<!--
		`contents`: these become items of the layout's column, so the layout can put
		the big header and its notices above them and push everything to the bottom.
	-->
	<div class="contents">
		<!-- Right of the big header: recent screenshots above the highlight numbers -->
		<Teleport defer to="#studio-play-side">
			<div class="flex w-[min(28rem,30vw)] flex-col items-end gap-5 pb-3 max-xl:hidden">
				<div class="flex items-end gap-8">
					<button class="highlight" @click="router.push(`${base}/content`)">
						<span class="highlight-value">{{ contentQuery.isPending.value ? '…' : counts.mods }}</span>
						<span class="highlight-label">mods</span>
					</button>
					<button class="highlight" @click="router.push(`${base}/worlds`)">
						<span class="highlight-value">{{ counts.worlds ?? '…' }}</span>
						<span class="highlight-label">worlds</span>
					</button>
					<div class="highlight">
						<span class="highlight-value">{{ hours.toLocaleString() }}</span>
						<span class="highlight-label">hours played</span>
					</div>
				</div>
				<div v-if="screenshots.length" class="w-full">
					<div class="mb-2 flex items-baseline justify-between gap-3">
						<span class="panel-title">Screenshots</span>
						<button
							class="cursor-pointer border-0 bg-transparent p-0 text-sm font-semibold text-secondary hover:text-contrast"
							@click="router.push(`${base}/screenshots`)"
						>
							View all {{ screenshots.length }}
						</button>
					</div>
					<div class="grid grid-cols-3 gap-2">
						<button
							v-for="shot in screenshots.slice(0, 6)"
							:key="shot.id"
							class="shot aspect-video cursor-pointer overflow-hidden rounded-xl border-0 p-0"
							@click="router.push(`${base}/screenshots`)"
						>
							<img
								:src="shot.url"
								alt=""
								loading="lazy"
								class="h-full w-full object-cover transition-transform duration-300 hover:scale-105"
							/>
						</button>
					</div>
				</div>
			</div>
		</Teleport>

		<!-- Bottom: just the worlds/servers to jump back into -->
		<section class="glass-panel order-3 shrink-0">
			<div class="mb-1 px-1">
				<span class="panel-title">Jump back in</span>
			</div>
			<RecentWorldsList :recent-instances="recent" hide-title worlds-only />
		</section>
	</div>
</template>

<style scoped>
/* Frosted panels along the bottom, like a game launcher's footer. */
.glass-panel {
	@apply min-w-0 rounded-3xl border border-solid border-surface-4 p-4;
	background: color-mix(in srgb, var(--color-bg) 72%, transparent);
	backdrop-filter: blur(16px);
}
.shot {
	box-shadow: 0 6px 20px rgb(0 0 0 / 0.35);
	outline: 1px solid color-mix(in srgb, white 10%, transparent);
}
.panel-title {
	@apply text-sm font-semibold uppercase tracking-wide text-secondary;
}
.highlight {
	@apply flex cursor-pointer flex-col items-end border-0 bg-transparent p-0 text-right;
}
div.highlight {
	@apply cursor-default;
}
.highlight-value {
	@apply font-extrabold leading-none text-contrast drop-shadow-lg;
	font-size: clamp(2rem, 3vw, 3.75rem);
}
button.highlight:hover .highlight-value {
	@apply text-brand;
}
.highlight-label {
	@apply mt-1 text-sm font-semibold uppercase tracking-wide text-secondary;
}
</style>
