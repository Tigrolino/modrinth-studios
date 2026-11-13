<script setup lang="ts">
/**
 * Modrinth Studios: settings page for Play mode (see features/play-mode/state.ts).
 */
import { CheckIcon, PlayIcon, SearchIcon } from '@modrinth/assets'
import { Avatar, Input, Toggle } from '@modrinth/ui'
import { useQuery } from '@tanstack/vue-query'
import dayjs from 'dayjs'
import { computed, ref, watch } from 'vue'

import { usePlayMode } from '@/features/play-mode/state'
import { getInstanceIconUrl } from '@/helpers/instance'
import { instanceListQueryOptions } from '@/pages/instance/query-options'

const { state } = usePlayMode()

const instancesQuery = useQuery(instanceListQueryOptions())
const search = ref('')

const instances = computed(() =>
	[...(instancesQuery.data.value ?? [])].sort((a, b) =>
		dayjs(b.last_played ?? b.created).diff(dayjs(a.last_played ?? a.created)),
	),
)
const filtered = computed(() => {
	const q = search.value.trim().toLowerCase()
	return q ? instances.value.filter((i) => i.name.toLowerCase().includes(q)) : instances.value
})

const selected = computed(() => instances.value.find((i) => i.id === state.instanceId) ?? null)

// Turning it on without a choice: default to the most recently played instance.
watch(
	() => state.enabled,
	(enabled) => {
		if (enabled && !selected.value && instances.value.length) {
			state.instanceId = instances.value[0].id
		}
	},
)

function loaderLabel(instance: { loader: string; game_version: string }) {
	const loader = instance.loader === 'vanilla' ? 'Vanilla' : instance.loader
	return `${loader.charAt(0).toUpperCase()}${loader.slice(1)} ${instance.game_version}`
}
</script>

<template>
	<section class="border-0 border-b border-solid border-surface-4 pb-6">
		<div class="flex items-center justify-between gap-4">
			<div>
				<h2 id="play-mode-label" class="m-0 flex items-center gap-2 text-lg font-semibold text-contrast">
					<PlayIcon class="size-5 text-brand" />
					Play mode
				</h2>
				<p class="m-0 mt-1 text-secondary">
					For when you mostly play one instance. The Play button in the sidebar opens that
					instance as a big play screen with its tabs on top and your recent worlds at the
					bottom. Your library moves to its own button underneath.
				</p>
			</div>
			<Toggle id="play-mode" v-model="state.enabled" aria-labelledby="play-mode-label" />
		</div>
	</section>

	<section
		v-if="state.enabled"
		class="border-0 border-b border-solid border-surface-4 py-6"
	>
		<h2 class="m-0 text-xl font-semibold text-contrast">Default instance</h2>
		<p class="m-0 mt-1 text-secondary">
			<template v-if="selected">
				Opening <span class="font-semibold text-contrast">{{ selected.name }}</span> when you press
				Play.
			</template>
			<template v-else>Pick the instance to open when you press Play.</template>
		</p>

		<Input
			v-if="instances.length > 6"
			v-model="search"
			:icon="SearchIcon"
			type="search"
			placeholder="Search instances"
			wrapper-class="mt-4 w-full"
		/>

		<div class="mt-4 flex max-h-[22rem] flex-col gap-2 overflow-y-auto pr-1">
			<button
				v-for="instance in filtered"
				:key="instance.id"
				type="button"
				class="flex w-full cursor-pointer items-center gap-3 rounded-2xl border border-solid p-3 text-left transition-colors"
				:class="
					state.instanceId === instance.id
						? 'border-brand bg-brand-highlight'
						: 'border-transparent bg-surface-4 hover:brightness-110'
				"
				:disabled="!state.enabled"
				@click="state.instanceId = instance.id"
			>
				<Avatar
					:src="getInstanceIconUrl(instance.icon_path)"
					:alt="instance.name"
					size="40px"
					:tint-by="instance.id"
				/>
				<span class="flex min-w-0 flex-1 flex-col">
					<span class="truncate font-semibold text-contrast">{{ instance.name }}</span>
					<span class="truncate text-sm text-secondary">{{ loaderLabel(instance) }}</span>
				</span>
				<CheckIcon v-if="state.instanceId === instance.id" class="size-5 shrink-0 text-brand" />
			</button>
			<p v-if="!filtered.length" class="m-0 text-secondary">No instances found.</p>
		</div>
	</section>
</template>
