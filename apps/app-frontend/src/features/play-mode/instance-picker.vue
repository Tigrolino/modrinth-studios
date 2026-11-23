<script setup lang="ts">
/**
 * Modrinth Studios: Play mode's instance switcher. Favourites (the same
 * Favorites as the Library) sit at the top as big tiles, so the two or three
 * instances you play are one click away; everything else is listed by when
 * you last played it, with a search.
 */
import { CheckIcon, DropdownIcon, SearchIcon, StarIcon } from '@modrinth/assets'
import { Avatar, injectNotificationManager, Input, TeleportPopoutMenu } from '@modrinth/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import dayjs from 'dayjs'
import relativeTime from 'dayjs/plugin/relativeTime'
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'

import { useHiddenInstances } from '@/features/library/hidden-instances/use-hidden-instances'
import { edit, getInstanceIconUrl } from '@/helpers/instance'
import { FAVORITES_GROUP_ID } from '@/helpers/instance-groups'
import type { GameInstance } from '@/helpers/types'
import { instanceListQueryOptions } from '@/pages/instance/query-options'

import { usePlayMode } from './state'

dayjs.extend(relativeTime)

defineProps<{ instanceId: string; name: string; icon?: string }>()
const router = useRouter()
const queryClient = useQueryClient()
const { handleError } = injectNotificationManager()
const { state } = usePlayMode()
const { hiddenIds } = useHiddenInstances()
const query = useQuery(instanceListQueryOptions())
const search = ref('')
const switching = ref(false)

const all = computed(() =>
	((query.data.value ?? []) as GameInstance[]).filter((i) => !hiddenIds.value.has(i.id)),
)
const isFavorite = (i: GameInstance) => i.group_ids?.includes(FAVORITES_GROUP_ID)
const lastActive = (i: GameInstance) =>
	Math.max(dayjs(i.last_played ?? 0).valueOf(), dayjs(i.created ?? 0).valueOf())

const favorites = computed(() =>
	all.value.filter(isFavorite).sort((a, b) => a.name.localeCompare(b.name)),
)
const recent = computed(() =>
	all.value.filter((i) => !isFavorite(i)).sort((a, b) => lastActive(b) - lastActive(a)),
)
const results = computed(() => {
	const term = search.value.trim().toLowerCase()
	return all.value
		.filter((i) => `${i.name} ${i.loader} ${i.game_version}`.toLowerCase().includes(term))
		.sort((a, b) => Number(isFavorite(b)) - Number(isFavorite(a)) || lastActive(b) - lastActive(a))
})

function subtitle(i: GameInstance) {
	const loader = i.loader === 'vanilla' ? 'Vanilla' : i.loader.charAt(0).toUpperCase() + i.loader.slice(1)
	const played = i.last_played ? ` · ${dayjs(i.last_played).fromNow()}` : ''
	return `${loader} ${i.game_version}${played}`
}

async function toggleFavorite(i: GameInstance) {
	const groups = i.group_ids ?? []
	const next = isFavorite(i)
		? groups.filter((g) => g !== FAVORITES_GROUP_ID)
		: [...new Set([...groups, FAVORITES_GROUP_ID])]
	// Show it right away; the list refreshes from the app after.
	queryClient.setQueryData(instanceListQueryOptions().queryKey, (old: GameInstance[] | undefined) =>
		old?.map((x) => (x.id === i.id ? { ...x, group_ids: next } : x)),
	)
	try {
		await edit(i.id, { group_ids: next })
	} catch (e) {
		handleError(e as Error)
	} finally {
		void queryClient.invalidateQueries({ queryKey: instanceListQueryOptions().queryKey })
	}
}

async function selectInstance(id: string, close: () => void) {
	if (switching.value) return
	switching.value = true
	try {
		const failure = await router.push(`/play/${encodeURIComponent(id)}`)
		if (!failure) {
			state.instanceId = id
			close()
		}
	} finally {
		switching.value = false
	}
}
</script>

<template>
	<TeleportPopoutMenu
		label="Switch instance"
		type="quiet"
		placement="bottom-start"
		class="min-w-0 max-w-full justify-self-start"
		@open="search = ''"
	>
		<template #trigger>
			<Avatar :src="icon" :alt="name" size="32px" :tint-by="instanceId" />
			<span class="truncate text-lg font-extrabold">{{ name }}</span>
			<DropdownIcon class="size-4 shrink-0 opacity-70" />
		</template>
		<template #panel="{ close }">
			<div class="flex w-[27rem] max-w-[calc(100vw-4rem)] flex-col gap-4 p-1">
				<Input
					v-model="search"
					:icon="SearchIcon"
					placeholder="Search instances"
					aria-label="Search instances"
					clearable
				/>

				<div class="flex max-h-[min(32rem,65vh)] flex-col gap-4 overflow-y-auto pr-1">
					<template v-if="!search.trim()">
						<!-- Favourites: big tiles, always on top -->
						<section class="flex flex-col gap-2">
							<span class="section-title"> <StarIcon class="size-3.5" /> Favorites </span>
							<div v-if="favorites.length" class="grid grid-cols-3 gap-2">
								<div v-for="i in favorites" :key="i.id" class="group relative">
									<button
										type="button"
										:disabled="switching"
										class="flex w-full cursor-pointer flex-col items-center gap-2 rounded-2xl border-2 border-solid p-3 pt-4 text-center transition-all hover:bg-surface-5 active:scale-[0.97] disabled:opacity-50"
										:class="
											i.id === instanceId
												? 'border-brand bg-brand-highlight'
												: 'border-transparent bg-surface-4'
										"
										@click="selectInstance(i.id, close)"
									>
										<Avatar
											:src="getInstanceIconUrl(i.icon_path)"
											:alt="i.name"
											size="56px"
											:tint-by="i.id"
										/>
										<span class="w-full truncate text-sm font-bold text-contrast">{{ i.name }}</span>
										<span class="-mt-1.5 w-full truncate text-xs text-secondary">{{
											i.game_version
										}}</span>
									</button>
									<button
										v-tooltip="'Remove from favorites'"
										type="button"
										class="star absolute right-1.5 top-1.5 opacity-0 group-hover:opacity-100"
										@click.stop="toggleFavorite(i)"
									>
										<StarIcon class="size-4 fill-current text-orange" />
									</button>
								</div>
							</div>
							<p v-else class="m-0 rounded-2xl bg-surface-4 p-3 text-sm text-secondary">
								Star the instances you play most and they'll stay up here.
							</p>
						</section>

						<!-- Everything else, last played first -->
						<section class="flex flex-col gap-1">
							<span class="section-title">Recent</span>
							<div v-for="i in recent" :key="i.id" class="group flex items-center gap-1">
								<button
									type="button"
									:disabled="switching"
									class="row"
									:class="i.id === instanceId ? 'bg-brand-highlight' : 'bg-transparent'"
									@click="selectInstance(i.id, close)"
								>
									<Avatar
										:src="getInstanceIconUrl(i.icon_path)"
										:alt="i.name"
										size="36px"
										:tint-by="i.id"
									/>
									<span class="flex min-w-0 flex-1 flex-col">
										<span class="truncate font-semibold text-contrast">{{ i.name }}</span>
										<span class="truncate text-xs text-secondary">{{ subtitle(i) }}</span>
									</span>
									<CheckIcon v-if="i.id === instanceId" class="size-5 shrink-0 text-brand" />
								</button>
								<button
									v-tooltip="'Add to favorites'"
									type="button"
									class="star opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
									@click.stop="toggleFavorite(i)"
								>
									<StarIcon class="size-4" />
								</button>
							</div>
						</section>
					</template>

					<!-- Search results -->
					<section v-else class="flex flex-col gap-1">
						<div v-for="i in results" :key="i.id" class="group flex items-center gap-1">
							<button
								type="button"
								:disabled="switching"
								class="row"
								:class="i.id === instanceId ? 'bg-brand-highlight' : 'bg-transparent'"
								@click="selectInstance(i.id, close)"
							>
								<Avatar
									:src="getInstanceIconUrl(i.icon_path)"
									:alt="i.name"
									size="36px"
									:tint-by="i.id"
								/>
								<span class="flex min-w-0 flex-1 flex-col">
									<span class="truncate font-semibold text-contrast">{{ i.name }}</span>
									<span class="truncate text-xs text-secondary">{{ subtitle(i) }}</span>
								</span>
								<CheckIcon v-if="i.id === instanceId" class="size-5 shrink-0 text-brand" />
							</button>
							<button
								v-tooltip="isFavorite(i) ? 'Remove from favorites' : 'Add to favorites'"
								type="button"
								class="star"
								:class="isFavorite(i) ? '' : 'opacity-0 group-hover:opacity-100'"
								@click.stop="toggleFavorite(i)"
							>
								<StarIcon class="size-4" :class="isFavorite(i) ? 'fill-current text-orange' : ''" />
							</button>
						</div>
						<p v-if="!results.length" class="m-0 p-3 text-secondary">No instances found.</p>
					</section>

					<p v-if="query.isPending.value" class="m-0 p-3 text-secondary">Loading instances…</p>
					<p v-else-if="query.isError.value" class="m-0 p-3 text-secondary">
						Could not load instances.
					</p>
				</div>
			</div>
		</template>
	</TeleportPopoutMenu>
</template>

<style scoped>
.section-title {
	@apply flex items-center gap-1.5 px-1 text-xs font-bold uppercase tracking-wider text-secondary;
}
.row {
	@apply flex min-w-0 flex-1 cursor-pointer items-center gap-3 rounded-xl border-0 p-2 text-left transition-colors hover:bg-surface-5 disabled:opacity-50;
}
.star {
	@apply grid size-8 shrink-0 cursor-pointer place-items-center rounded-full border-0 bg-transparent text-secondary transition-all hover:bg-surface-5 hover:text-orange;
}
</style>
