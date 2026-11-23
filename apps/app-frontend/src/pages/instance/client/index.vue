<script setup lang="ts">
/**
 * Modrinth Studios: the client tabs of a client installation, laid out like
 * Modrinth's own instance tabs (toolbar on top, list underneath):
 * - Horizon: Home, Content (Horizon's registry) and Patch notes
 * - NoRisk: News and NoRisk mods (the pack's own mods)
 * - Lunar, LabyMod: News
 */
import {
	CheckIcon,
	DownloadIcon,
	ExternalIcon,
	FolderOpenIcon,
	LogInIcon,
	RefreshCwIcon,
	SearchIcon,
	SpinnerIcon,
} from '@modrinth/assets'
import {
	Admonition,
	Button,
	EmptyState,
	injectNotificationManager,
	Input,
	NewsArticleCard,
} from '@modrinth/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { openUrl } from '@/platform/studio-client-desktop'
import dayjs from 'dayjs'
import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'

import {
	findRunningClientTask,
	runClientPrepare,
} from '@/components/ui/download-manager/studio-client-tasks'
import {
	clientLogo,
	clientMeta,
	clientNews,
	getClientInstall,
	noriskSignedIn,
	noriskSignIn,
} from '@/helpers/studio-clients'
import HelpTip from '@/features/studio-clients/HelpTip.vue'
import { useClientAccount } from '@/features/studio-clients/use-client-account'
import { showInstanceInFolder } from '@/helpers/utils.js'
import { injectAppEvents } from '@/providers/app-events'

import { injectInstancePage } from '../instance-context'
import ClientCosmetics from './cosmetics.vue'
import HorizonContent from './horizon-content.vue'
import HorizonHome from './horizon-home.vue'
import { noteCounts, parseNotes, TONE_CLASSES } from './horizon-notes'
import NoriskMods from './norisk-mods.vue'

const { instanceId, instance, playing } = injectInstancePage()
const { handleError } = injectNotificationManager()
const queryClient = useQueryClient()
const route = useRoute()
const appEvents = injectAppEvents()

const installQuery = useQuery({
	queryKey: computed(() => ['studio-client', instanceId.value]),
	queryFn: () => getClientInstall(instanceId.value),
	networkMode: 'always',
})
const install = computed(() => installQuery.data.value ?? null)
const meta = computed(() => clientMeta(install.value?.client))
const kind = computed(() => install.value?.client ?? null)

/** Which tab this is. */
const view = computed<
	'home' | 'content' | 'patch-notes' | 'news' | 'pack-mods' | 'cosmetics'
>(() => {
	const section = String(route.params.section ?? '')
	if (section === 'cosmetics') return 'cosmetics'
	if (kind.value === 'horizon') {
		if (section === 'news') return 'patch-notes'
		return section === 'content' ? 'content' : 'home'
	}
	if (kind.value === 'norisk' && section === 'mods') return 'pack-mods'
	return 'news'
})

const search = ref('')
watch([instanceId, view], () => {
	search.value = ''
	openNotes.value = new Set<number>()
})
const canUpdate = computed(() => !playing.value && instance.value?.install_stage === 'installed')

// ---- download state ----
const runningTask = computed(() => findRunningClientTask(instanceId.value))
async function updateFiles() {
	if (!meta.value || !canUpdate.value) return
	const id = instanceId.value
	try {
		await runClientPrepare(appEvents, id, meta.value.name, clientLogo(meta.value))
		await queryClient.invalidateQueries({ queryKey: ['studio-client', id] })
		await queryClient.invalidateQueries({ queryKey: ['norisk-pack-mods', id] })
	} catch (e) {
		handleError(e as Error)
	}
}

// ---- news / patch notes ----
const newsQuery = useQuery({
	queryKey: computed(() => ['studio-client-news', kind.value]),
	queryFn: () => clientNews(kind.value!),
	enabled: computed(() => !!kind.value && (view.value === 'news' || view.value === 'patch-notes')),
	staleTime: 10 * 60_000,
})
const news = computed(() => {
	const q = search.value.trim().toLowerCase()
	return (newsQuery.data.value ?? []).filter(
		(n) => !q || n.title.toLowerCase().includes(q) || (n.summary ?? '').toLowerCase().includes(q),
	)
})
const openNotes = ref<Set<number>>(new Set<number>())
// Patch notes: the newest is open unless closed; older ones open on click.
const closedFirst = ref(false)
function isOpen(i: number) {
	return i === 0 && !search.value.trim() ? !closedFirst.value : openNotes.value.has(i)
}
const expandedSections = ref<Set<string>>(new Set())
function expandSection(key: string) {
	expandedSections.value = new Set([...expandedSections.value, key])
}
const parsedNews = computed(() => news.value.map((n) => ({ ...n, notes: parseNotes(n.body) })))
function toggleNotes(i: number) {
	if (i === 0 && !search.value.trim()) {
		closedFirst.value = !closedFirst.value
		return
	}
	const next = new Set(openNotes.value)
	if (next.has(i)) next.delete(i)
	else next.add(i)
	openNotes.value = next
}

// ---- NoRisk sign-in ----
const { account } = useClientAccount()
const noriskQuery = useQuery({
	queryKey: computed(() => ['minecraft-account', account.value?.id, 'norisk-signed-in']),
	queryFn: noriskSignedIn,
	enabled: computed(() => kind.value === 'norisk' && !!account.value),
	refetchInterval: 60_000,
})
const signingIn = ref(false)
async function signInNoRisk() {
	signingIn.value = true
	try {
		await noriskSignIn()
		await queryClient.invalidateQueries({ queryKey: ['minecraft-account'] })
	} catch (e) {
		handleError(e as Error)
	} finally {
		signingIn.value = false
	}
}

const newsArticles = computed(() =>
	news.value
		.filter((n) => n.url)
		.map((n) => ({
			path: n.url!,
			thumbnail: n.image ?? '',
			title: n.title,
			summary: n.summary ?? '',
			date: n.date ?? '',
		})),
)

function refresh() {
	void queryClient.invalidateQueries({ queryKey: ['studio-client-news', kind.value] })
}

function formatDate(date: string | null) {
	if (!date) return ''
	const d = dayjs(date)
	return d.isValid() ? d.format('MMM D, YYYY') : date
}

const searchPlaceholder = computed(() => {
	switch (view.value) {
		case 'patch-notes':
			return 'Search patch notes...'
		default:
			return 'Search news...'
	}
})
</script>

<template>
	<div class="flex flex-col gap-4 pb-6">
		<div v-if="installQuery.isPending.value" class="flex items-center gap-2 text-secondary">
			<SpinnerIcon class="size-5 animate-spin" /> Loading…
		</div>

		<Admonition
			v-else-if="installQuery.isError.value"
			type="critical"
			header="Could not load this client installation"
			:body="String(installQuery.error.value)"
		/>
		<template v-else-if="install && meta">
			<HorizonHome
				v-if="view === 'home'"
				:instance-id="instanceId"
				:install="install"
				:logo="clientLogo(meta)"
				:playing="!!playing"
			/>
			<!-- Content-style tabs: Modrinth's own content page -->
			<HorizonContent v-else-if="view === 'content'" :instance-id="instanceId" />
			<ClientCosmetics
				v-else-if="view === 'cosmetics'"
				:kind="install.client"
				:client-name="meta.name"
			/>
			<NoriskMods
				v-else-if="view === 'pack-mods'"
				:instance-id="instanceId"
				:pack-name="install.variant_name ?? install.version_id.split('|')[0]"
				:running="!!runningTask"
				:can-update="canUpdate"
				:on-update="updateFiles"
			/>

			<template v-else>
				<!-- Toolbar, like the Worlds tab -->
				<div class="flex flex-wrap items-center gap-2">
					<Input
						v-model="search"
						:icon="SearchIcon"
						type="text"
						autocomplete="off"
						size="medium"
						wrapper-class="flex-1 min-w-0"
						clearable
						:placeholder="searchPlaceholder"
					/>
					<div class="flex gap-2">
						<Button
							v-if="kind === 'norisk'"
							v-tooltip="
								noriskQuery.data.value
									? `Signed in to NoRisk as ${noriskQuery.data.value}`
									: 'Sign in to NoRisk with your Minecraft account'
							"
							type="outlined"
							size="lg"
							:disabled="signingIn || !account"
							@click="signInNoRisk"
						>
							<SpinnerIcon v-if="signingIn" class="size-5 animate-spin" />
							<CheckIcon v-else-if="noriskQuery.data.value" class="size-5 text-brand" />
							<LogInIcon v-else class="size-5" />
							{{ noriskQuery.data.value ? noriskQuery.data.value : 'Sign in to NoRisk' }}
						</Button>
						<Button type="outlined" size="lg" @click="showInstanceInFolder(instanceId)">
							<FolderOpenIcon class="size-5" />
							Open folder
						</Button>
						<Button
							v-if="meta.links[0]"
							type="colored"
							color="brand"
							size="lg"
							@click="openUrl(meta.links[0].url)"
						>
							<ExternalIcon class="size-5" />
							{{ meta.links[0].label }}
						</Button>
					</div>
				</div>
				<div class="flex flex-wrap items-center gap-2">
					<span class="flex items-center gap-2 text-sm text-secondary">
						{{ meta.name }} {{ install.version_name
						}}<template v-if="install.variant_name"> · {{ install.variant_name }}</template>
						<HelpTip :text="meta.notes.join(' ')" />
					</span>
					<span v-if="runningTask" class="flex items-center gap-1.5 text-sm text-secondary">
						<SpinnerIcon class="size-4 animate-spin" />
						{{ runningTask.message }}
					</span>
					<div class="ml-auto flex items-center gap-2">
						<Button
							type="quiet"
							class="!text-sm !font-medium"
							:disabled="!!runningTask || !canUpdate"
							@click="updateFiles"
						>
							<DownloadIcon />
							Update files
						</Button>
						<Button type="quiet" class="!text-sm !font-medium" @click="refresh">
							<RefreshCwIcon />
							Refresh
						</Button>
					</div>
				</div>

				<Admonition
					v-if="newsQuery.isError.value"
					type="critical"
					header="Could not load the feed"
					body="Check your connection and try Refresh. Your installed client is still available."
				/>
				<!-- News: Modrinth's own news cards -->
				<template v-if="view === 'news'">
					<div v-if="newsQuery.isPending.value" class="flex items-center gap-2 text-secondary">
						<SpinnerIcon class="size-5 animate-spin" /> Loading news…
					</div>
					<EmptyState
						v-else-if="!newsQuery.isError.value && !newsArticles.length"
						type="empty-inbox"
					>
						<template #heading>{{ search.trim() ? 'No matching news' : 'No news yet' }}</template>
						<template #description>
							{{
								search.trim()
									? 'Try a different search.'
									: 'New posts will appear here when available.'
							}}
						</template>
					</EmptyState>
					<div
						v-else
						class="mx-auto grid w-full max-w-[100rem] grid-cols-[repeat(auto-fit,minmax(min(100%,22rem),1fr))] gap-6"
					>
						<NewsArticleCard
							v-for="(article, index) in newsArticles"
							:key="article.path"
							:class="{
								'col-span-full [&_h3]:text-2xl [&_img]:max-h-80 [&_p]:text-base':
									index === 0 && !search.trim() && !!article.thumbnail,
							}"
							class="rounded-2xl border border-solid border-surface-4 bg-surface-2 p-5"
							:article="article"
						/>
					</div>
				</template>

				<!-- Patch notes (Horizon): newest build open, older ones as a timeline -->
				<template v-if="view === 'patch-notes'">
					<div v-if="newsQuery.isPending.value" class="flex items-center gap-2 text-secondary">
						<SpinnerIcon class="size-5 animate-spin" /> Loading patch notes…
					</div>
					<EmptyState v-else-if="!newsQuery.isError.value && !news.length" type="empty-inbox">
						<template #heading>{{
							search.trim() ? 'No matching patch notes' : 'No patch notes yet'
						}}</template>
					</EmptyState>
					<ol v-else class="m-0 mx-auto flex w-full max-w-4xl list-none flex-col p-0">
						<li
							v-for="(item, i) in parsedNews"
							:key="item.title + i"
							class="relative grid grid-cols-[1.5rem_minmax(0,1fr)] gap-4 pb-6 last:pb-0"
						>
							<!-- timeline rail -->
							<div class="relative flex justify-center">
								<span
									class="relative z-[1] mt-5 size-3 rounded-full"
									:class="i === 0 ? 'bg-brand ring-4 ring-brand-highlight' : 'bg-surface-5'"
								/>
								<span
									v-if="i < parsedNews.length - 1"
									class="absolute bottom-[-1.5rem] top-8 w-0.5 bg-surface-4"
								/>
							</div>
							<article class="rounded-2xl border border-solid border-surface-4 bg-surface-3">
								<button
									class="flex w-full cursor-pointer flex-wrap items-center gap-x-3 gap-y-2 border-0 bg-transparent p-4 text-left"
									@click="toggleNotes(i)"
								>
									<div class="flex min-w-0 flex-1 flex-col">
										<span class="flex items-center gap-2">
											<span class="truncate text-base font-bold text-contrast">{{ item.title }}</span>
											<span
												v-if="i === 0 && !search.trim()"
												class="rounded-full bg-brand-highlight px-2 py-0.5 text-xs font-semibold text-brand"
												>Latest</span
											>
										</span>
										<span class="text-sm text-secondary">{{ formatDate(item.date) }}</span>
									</div>
									<div class="flex flex-wrap gap-1.5">
										<span
											v-for="c in noteCounts(item.notes)"
											:key="c.label"
											class="rounded-full px-2.5 py-0.5 text-xs font-semibold"
											:class="TONE_CLASSES[c.tone]"
											>{{ c.label }}</span
										>
									</div>
								</button>
								<div
									v-if="isOpen(i)"
									class="flex flex-col gap-4 border-0 border-t border-solid border-surface-4 px-5 py-4"
								>
									<section v-for="section in item.notes.sections" :key="section.title">
										<h4 class="m-0 mb-2 flex items-center gap-2 text-sm font-bold text-contrast">
											<span
												class="size-2 rounded-full"
												:class="{
													'bg-brand': section.tone === 'new',
													'bg-blue': section.tone === 'fixed',
													'bg-orange': section.tone === 'changed',
													'bg-secondary': section.tone === 'other',
												}"
											/>
											{{ section.title }}
										</h4>
										<ul class="m-0 flex list-none flex-col gap-1.5 p-0">
											<li
												v-for="(entry, j) in expandedSections.has(`${i}:${section.title}`)
													? section.items
													: section.items.slice(0, 12)"
												:key="j"
												class="flex flex-wrap items-baseline gap-x-2 text-sm leading-relaxed text-primary"
											>
												<span>{{ entry.text }}</span>
												<button
													v-for="ref in entry.refs"
													:key="ref"
													class="cursor-pointer border-0 bg-transparent p-0 text-xs font-semibold text-secondary hover:text-brand hover:underline"
													@click="openUrl(`https://github.com/horizon-rust/horizon/issues/${ref.slice(1)}`)"
												>
													{{ ref }}
												</button>
											</li>
										</ul>
										<button
											v-if="section.items.length > 12 && !expandedSections.has(`${i}:${section.title}`)"
											class="mt-2 cursor-pointer border-0 bg-transparent p-0 text-sm font-semibold text-brand hover:underline"
											@click="expandSection(`${i}:${section.title}`)"
										>
											Show all {{ section.items.length }}
										</button>
									</section>
									<p v-if="item.notes.contributors" class="m-0 text-xs text-secondary">
										Thanks to {{ item.notes.contributors }}
									</p>
									<button
										v-if="item.url"
										class="flex w-fit cursor-pointer items-center gap-1 border-0 bg-transparent p-0 text-sm font-semibold text-brand hover:underline"
										@click="openUrl(item.url)"
									>
										View on GitHub <ExternalIcon class="size-4" />
									</button>
								</div>
							</article>
						</li>
					</ol>
				</template>
			</template>
		</template>
	</div>
</template>
