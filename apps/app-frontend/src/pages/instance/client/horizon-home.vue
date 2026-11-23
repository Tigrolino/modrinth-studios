<script setup lang="ts">
/**
 * Modrinth Studios: Horizon's Home tab. What Horizon's own launcher shows on
 * its Home page (the build you're on, the latest patch log, the registry),
 * laid out with Modrinth's cards and colours.
 */
import {
	BookOpenIcon,
	BoxesIcon,
	CheckIcon,
	ChevronRightIcon,
	CpuIcon,
	DownloadIcon,
	ExternalIcon,
	GithubIcon,
	GlobeIcon,
	InfoIcon,
	NewspaperIcon,
	PaintbrushIcon,
	PlusIcon,
	SparklesIcon,
	SpinnerIcon,
	UsersIcon,
} from '@modrinth/assets'
import { Button, injectNotificationManager, useCompactNumber } from '@modrinth/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import dayjs from 'dayjs'
import relativeTime from 'dayjs/plugin/relativeTime'
import { computed, ref } from 'vue'

import {
	type ClientInstall,
	clientNews,
	type HorizonProject,
	horizonStoreInstall,
	horizonStoreList,
} from '@/helpers/studio-clients'
import { openUrl } from '@/platform/studio-client-desktop'

import { noteCounts, parseNotes, TONE_CLASSES } from './horizon-notes'

const props = defineProps<{
	instanceId: string
	install: ClientInstall
	logo: string
	playing: boolean
}>()

dayjs.extend(relativeTime)

const { handleError } = injectNotificationManager()
const { formatCompactNumber } = useCompactNumber()
const queryClient = useQueryClient()

// A screenshot from Horizon's own README, rendered by Horizon.
const BANNER =
	'https://raw.githubusercontent.com/horizon-rust/horizon/main/docs/media/readme/hero-lookout.webp'
const bannerFailed = ref(false)
const buildLabel = computed(
	() => props.install.version_name?.trim() || props.install.version_id || 'Horizon',
)

const storeQuery = useQuery({
	queryKey: computed(() => ['horizon-store', props.instanceId]),
	queryFn: () => horizonStoreList(props.instanceId),
	staleTime: 60_000,
})
const notesQuery = useQuery({
	queryKey: ['studio-client-news', 'horizon'],
	queryFn: () => clientNews('horizon'),
	staleTime: 10 * 60_000,
})

const store = computed(() => storeQuery.data.value ?? [])
const browse = (type: string) => `/horizon-browse/${encodeURIComponent(props.instanceId)}/${type}`

const shelves = computed(() =>
	(
		[
			{ kind: 'mod', label: 'Mods', blurb: 'Blocks, items and recipes', icon: BoxesIcon },
			{
				kind: 'resourcepack',
				label: 'Resource Packs',
				blurb: 'Change how the world looks',
				icon: PaintbrushIcon,
			},
			{ kind: 'shader', label: 'Shaders', blurb: 'Light, water and sky', icon: SparklesIcon },
		] as const
	).map((s) => {
		const all = store.value.filter((p) => p.kind === s.kind)
		return { ...s, available: all.length, installed: all.filter((p) => p.installed).length }
	}),
)
const installedTotal = computed(() => store.value.filter((p) => p.installed).length)

/** Most downloaded registry projects that aren't installed yet. */
const popular = computed(() =>
	[...store.value]
		.filter((p) => !p.installed)
		.sort((a, b) => b.downloads - a.downloads)
		.slice(0, 5),
)
/** Newest releases in the registry. */
const fresh = computed(() =>
	[...store.value]
		.filter((p) => p.published)
		.sort((a, b) => (b.published ?? '').localeCompare(a.published ?? ''))
		.slice(0, 5),
)

const latest = computed(() => notesQuery.data.value?.[0] ?? null)
const latestNotes = computed(() => (latest.value ? parseNotes(latest.value.body) : null))

const busy = ref<string | null>(null)
async function install(p: HorizonProject) {
	if (busy.value) return
	busy.value = p.id
	try {
		await horizonStoreInstall(props.instanceId, p.id)
		await queryClient.invalidateQueries({ queryKey: ['horizon-store', props.instanceId] })
	} catch (e) {
		handleError(e as Error)
	} finally {
		busy.value = null
	}
}

function ago(date: string | null) {
	if (!date) return ''
	const d = dayjs(date)
	return d.isValid() ? d.fromNow() : date
}

const links = [
	{ label: 'Docs', url: 'https://playhorizon.dev/docs', icon: BookOpenIcon },
	{ label: 'Community hub', url: 'https://playhorizon.dev/hub', icon: UsersIcon },
	{ label: 'Registry', url: 'https://playhorizon.dev/mods', icon: GlobeIcon },
	{ label: 'GitHub', url: 'https://github.com/horizon-rust/horizon', icon: GithubIcon },
]
</script>

<template>
	<div class="flex flex-col gap-6">
		<!-- Hero: the build this instance runs -->
		<section
			class="relative overflow-hidden rounded-2xl border border-solid border-surface-4 bg-surface-2"
		>
			<img
				v-if="!bannerFailed"
				:src="BANNER"
				alt=""
				class="absolute inset-0 h-full w-full object-cover opacity-60"
				@error="bannerFailed = true"
			/>
			<div
				class="absolute inset-0 bg-gradient-to-r from-[#101216] via-[#101216e6] to-transparent"
			/>
			<div class="relative flex flex-wrap items-end gap-6 p-6 sm:p-8">
				<div class="flex min-w-0 flex-1 flex-col gap-3">
					<div class="flex items-center gap-3">
						<img :src="logo" alt="" class="size-12 rounded-xl" />
						<div class="flex flex-col">
							<span class="text-xs font-semibold uppercase tracking-wider text-secondary">
								Minecraft 26.3, rewritten in Rust
							</span>
							<span class="text-2xl font-extrabold text-contrast">Horizon</span>
						</div>
					</div>
					<div class="flex flex-wrap gap-2 text-sm">
						<span
							class="flex items-center gap-1.5 rounded-full bg-surface-4 px-3 py-1 font-semibold text-contrast"
						>
							<CpuIcon class="size-4" />
							<span>{{ buildLabel }}</span>
						</span>
						<span
							v-if="install.prepared"
							class="flex items-center gap-1.5 rounded-full bg-brand-highlight px-3 py-1 font-semibold text-brand"
						>
							<CheckIcon class="size-4" /> Ready to play
						</span>
						<span
							class="flex items-center gap-1.5 rounded-full bg-surface-4 px-3 py-1 font-semibold text-contrast"
						>
							<BoxesIcon class="size-4" />
							{{ installedTotal }} {{ installedTotal === 1 ? 'project' : 'projects' }} installed
						</span>
					</div>
				</div>
				<div class="flex flex-wrap gap-2">
					<RouterLink :to="browse('mod')">
						<Button type="colored" color="brand" size="lg">
							<PlusIcon class="size-5" /> Add content
						</Button>
					</RouterLink>
				</div>
			</div>
		</section>

		<!-- Shelves: one per kind of registry content -->
		<section class="grid grid-cols-1 gap-4 md:grid-cols-3">
			<RouterLink
				v-for="shelf in shelves"
				:key="shelf.kind"
				:to="browse(shelf.kind)"
				class="group flex items-center gap-4 rounded-2xl border border-solid border-surface-4 bg-surface-3 p-4 transition-colors hover:bg-surface-4"
			>
				<div class="grid size-12 shrink-0 place-items-center rounded-xl bg-surface-5 text-contrast">
					<component :is="shelf.icon" class="size-6" />
				</div>
				<div class="flex min-w-0 flex-1 flex-col">
					<span class="font-bold text-contrast">{{ shelf.label }}</span>
					<span class="truncate text-sm text-secondary">{{ shelf.blurb }}</span>
					<span class="mt-1 text-xs font-semibold text-secondary">
						<template v-if="storeQuery.isPending.value">Loading…</template>
						<template v-else>
							{{ shelf.available }} available<template v-if="shelf.installed">
								· <span class="text-brand">{{ shelf.installed }} installed</span></template
							>
						</template>
					</span>
				</div>
				<ChevronRightIcon class="size-5 text-secondary transition-transform group-hover:translate-x-0.5" />
			</RouterLink>
		</section>

		<div class="grid grid-cols-1 gap-6 xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
			<!-- Registry: popular + new -->
			<section class="flex flex-col gap-4">
				<div class="rounded-2xl border border-solid border-surface-4 bg-surface-3 p-5">
					<div class="mb-3 flex items-center justify-between">
						<h2 class="m-0 text-lg font-bold text-contrast">Popular on Horizon</h2>
						<RouterLink :to="browse('mod')" class="text-sm font-semibold text-brand hover:underline">
							See all
						</RouterLink>
					</div>
					<div v-if="storeQuery.isPending.value" class="flex items-center gap-2 text-secondary">
						<SpinnerIcon class="size-5 animate-spin" /> Loading the registry…
					</div>
					<div v-else-if="!popular.length" class="text-secondary">
						You have everything that's in the registry.
					</div>
					<div v-else class="flex flex-col">
						<div
							v-for="p in popular"
							:key="p.id"
							class="flex items-center gap-3 border-0 border-t border-solid border-surface-4 py-3 first:border-t-0 first:pt-0 last:pb-0"
						>
							<img
								v-if="p.icon"
								:src="p.icon"
								alt=""
								class="size-12 shrink-0 rounded-xl bg-surface-5 object-cover"
							/>
							<div v-else class="grid size-12 shrink-0 place-items-center rounded-xl bg-surface-5">
								<BoxesIcon class="size-6 text-secondary" />
							</div>
							<div class="flex min-w-0 flex-1 flex-col">
								<span class="truncate font-bold text-contrast">{{ p.name }}</span>
								<span class="truncate text-sm text-secondary">{{ p.summary }}</span>
							</div>
							<span class="hidden items-center gap-1 text-sm text-secondary sm:flex">
								<DownloadIcon class="size-4" /> {{ formatCompactNumber(p.downloads) }}
							</span>
							<Button
								type="outlined"
								color="brand"
								:disabled="!!busy || playing"
								@click="install(p)"
							>
								<SpinnerIcon v-if="busy === p.id" class="animate-spin" />
								<PlusIcon v-else />
								Install
							</Button>
						</div>
					</div>
				</div>

				<div class="rounded-2xl border border-solid border-surface-4 bg-surface-3 p-5">
					<h2 class="m-0 mb-3 text-lg font-bold text-contrast">New in the registry</h2>
					<div class="flex flex-wrap gap-2">
						<RouterLink
							v-for="p in fresh"
							:key="p.id"
							:to="browse(p.kind)"
							class="flex items-center gap-2 rounded-full bg-surface-4 py-1 pl-1 pr-3 text-sm transition-colors hover:bg-surface-5"
						>
							<img v-if="p.icon" :src="p.icon" alt="" class="size-6 rounded-full object-cover" />
							<span class="font-semibold text-contrast">{{ p.name }}</span>
							<span class="text-secondary">{{ ago(p.published) }}</span>
						</RouterLink>
					</div>
				</div>
			</section>

			<!-- Latest build + good to know -->
			<section class="flex flex-col gap-4">
				<div class="rounded-2xl border border-solid border-surface-4 bg-surface-3 p-5">
					<div class="mb-1 flex items-center justify-between gap-2">
						<span class="flex items-center gap-2 text-sm font-semibold text-secondary">
							<NewspaperIcon class="size-4" /> Latest patch log
						</span>
						<RouterLink
							:to="`/instance/${encodeURIComponent(instanceId)}/client/news`"
							class="text-sm font-semibold text-brand hover:underline"
						>
							All patch notes
						</RouterLink>
					</div>
					<div v-if="notesQuery.isPending.value" class="flex items-center gap-2 text-secondary">
						<SpinnerIcon class="size-5 animate-spin" /> Loading…
					</div>
					<template v-else-if="latest && latestNotes">
						<h3 class="m-0 text-lg font-bold text-contrast">{{ latest.title }}</h3>
						<span class="text-sm text-secondary">{{ ago(latest.date) }}</span>
						<div class="mt-3 flex flex-wrap gap-1.5">
							<span
								v-for="c in noteCounts(latestNotes)"
								:key="c.label"
								class="rounded-full px-2.5 py-0.5 text-xs font-semibold"
								:class="TONE_CLASSES[c.tone]"
							>
								{{ c.label }}
							</span>
						</div>
						<ul class="m-0 mt-3 flex list-none flex-col gap-2 p-0">
							<li
								v-for="(item, i) in latestNotes.sections.flatMap((s) =>
									s.items.map((it) => ({ ...it, tone: s.tone })),
								).slice(0, 5)"
								:key="i"
								class="flex gap-2 text-sm text-primary"
							>
								<span
									class="mt-1.5 size-2 shrink-0 rounded-full"
									:class="{
										'bg-brand': item.tone === 'new',
										'bg-blue': item.tone === 'fixed',
										'bg-orange': item.tone === 'changed',
										'bg-secondary': item.tone === 'other',
									}"
								/>
								<span>{{ item.text }}</span>
							</li>
						</ul>
					</template>
					<span v-else class="text-secondary">No patch notes right now.</span>
				</div>

				<div class="rounded-2xl border border-solid border-surface-4 bg-surface-3 p-5">
					<h2 class="m-0 mb-3 flex items-center gap-2 text-lg font-bold text-contrast">
						<InfoIcon class="size-5" /> Good to know
					</h2>
					<ul class="m-0 flex flex-col gap-2 pl-5 text-sm leading-relaxed text-secondary">
						<li>
							Java mods (Fabric, Forge, NeoForge) don't run in Horizon. Content mods,
							resource packs and shader packs from Horizon's registry do.
						</li>
						<li>
							Native mods (the ones with code, like Zoom or JEI) are compiled into the game by
							Horizon's own launcher, so they aren't offered here.
						</li>
						<li>
							You play with your Minecraft account, skin and cape included, on singleplayer,
							LAN and online servers. Chat on servers that require signed chat doesn't work in
							Horizon yet.
						</li>
					</ul>
				</div>

				<div class="grid grid-cols-2 gap-2">
					<button
						v-for="link in links"
						:key="link.url"
						class="flex cursor-pointer items-center gap-2 rounded-xl border border-solid border-surface-4 bg-surface-3 px-3 py-2.5 text-sm font-semibold text-contrast transition-colors hover:bg-surface-4"
						@click="openUrl(link.url)"
					>
						<component :is="link.icon" class="size-4" />
						{{ link.label }}
						<ExternalIcon class="ml-auto size-4 text-secondary" />
					</button>
				</div>
			</section>
		</div>
	</div>
</template>
