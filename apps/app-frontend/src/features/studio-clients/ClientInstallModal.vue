<script setup lang="ts">
/**
 * Modrinth Studios: "Client installation" picker, opened from Create
 * instance. Built from the same pieces as Create instance's custom setup:
 * pick a client, then its version (and module/pack/channel), name it, and
 * optionally copy servers, settings and packs from the client's own launcher.
 */
import { ChevronLeftIcon, ChevronRightIcon, PlusIcon, SpinnerIcon } from '@modrinth/assets'
import {
	Button,
	Checkbox,
	Chips,
	Combobox,
	type ComboboxOption,
	injectNotificationManager,
	Input,
	NewModal,
	Toggle,
} from '@modrinth/ui'
import { useQueryClient } from '@tanstack/vue-query'
import { computed, ref, useTemplateRef, watch } from 'vue'
import { useRouter } from 'vue-router'

import { runClientPrepare } from '@/components/ui/download-manager/studio-client-tasks'
import { trackEvent } from '@/helpers/analytics'
import { install_create_instance, installJobInstanceId } from '@/helpers/install'
import { edit, get as getInstance } from '@/helpers/instance'
import {
	CLIENTS,
	type ClientMeta,
	type ClientVersion,
	clientIcon,
	clientImport,
	clientImportSources,
	clientLogo,
	clientVersions,
	type ImportSource,
	setupClient,
} from '@/helpers/studio-clients'
import { injectAppEvents } from '@/providers/app-events'

import HelpTip from './HelpTip.vue'

const { handleError } = injectNotificationManager()
const router = useRouter()
const queryClient = useQueryClient()
const appEvents = injectAppEvents()
const modal = useTemplateRef<InstanceType<typeof NewModal>>('modal')

const selected = ref<ClientMeta | null>(null)
const versions = ref<ClientVersion[]>([])
const loading = ref(false)
const loadError = ref<string | null>(null)
const gameVersion = ref<string | undefined>(undefined)
const variant = ref<string | null>(null)
const name = ref('')
const creating = ref(false)
const showAll = ref(false)
let versionRequest = 0



function show() {
	versionRequest++
	loading.value = false
	selected.value = null
	versions.value = []
	loadError.value = null
	name.value = ''
	modal.value?.show()
}

async function pick(meta: ClientMeta) {
	const request = ++versionRequest
	selected.value = meta
	versions.value = []
	loadError.value = null
	gameVersion.value = undefined
	variant.value = null
	showAll.value = false
	copyEnabled.value = false
	loading.value = true
	try {
		const result = await clientVersions(meta.kind)
		if (request !== versionRequest) return
		versions.value = result
		const best = versions.value.find((v) => v.recommended) ?? versions.value[0]
		gameVersion.value = best?.name
		variant.value = best?.detail ?? null
	} catch (e) {
		if (request === versionRequest) {
			loadError.value = (e as { message?: string })?.message ?? String(e)
		}
	} finally {
		if (request === versionRequest) loading.value = false
	}
}

/** Version names in the order the client lists them, once each. */
const versionNames = computed(() => [...new Set(versions.value.map((v) => v.name))])

/**
 * The few versions most people want: the recommended one, the newest ones,
 * and the classic PvP version when the client has it. Everything else is
 * one click away under "All versions".
 */
const featured = computed(() => {
	const names = versionNames.value
	const out: string[] = []
	const add = (n: string | undefined) => n && !out.includes(n) && out.push(n)
	add(versions.value.find((v) => v.recommended)?.name)
	for (const n of names) {
		if (out.length >= 5) break
		add(n)
	}
	add(names.find((n) => n === '1.8.9'))
	if (gameVersion.value && !out.includes(gameVersion.value)) out.push(gameVersion.value)
	return out
})
const hiddenCount = computed(() => versionNames.value.length - featured.value.length)

/** Every version, for Modrinth's searchable dropdown ("1.8" finds 1.8.9). */
const allVersionOptions = computed<ComboboxOption<string>[]>(() =>
	versionNames.value.map((n) => ({
		value: n,
		label: n,
		subLabel: isRecommended(n) ? 'Recommended' : undefined,
	})),
)
const VARIANT_HELP: Record<string, string> = {
	lunar: 'Lunar + Fabric and Lunar + Forge also run mods from Modrinth.',
	norisk: 'Which NoRisk mod pack to use.',
	labymod: 'Which LabyMod release channel.',
}

function isRecommended(n: string) {
	return versions.value.some((v) => v.name === n && v.recommended)
}

const variants = computed(() =>
	versions.value.filter((v) => v.name === gameVersion.value).map((v) => v.detail ?? v.id),
)

watch(gameVersion, () => {
	if (!variants.value.includes(variant.value ?? '')) {
		const rec = versions.value.find((v) => v.name === gameVersion.value && v.recommended)
		variant.value = rec?.detail ?? variants.value[0] ?? null
	}
})

const pickedVersion = computed(
	() =>
		versions.value.find(
			(v) =>
				v.name === gameVersion.value && (v.detail ?? v.id) === (variant.value ?? v.detail ?? v.id),
		) ?? null,
)

const defaultName = computed(() => {
	if (!selected.value) return ''
	if (selected.value.kind === 'horizon') return 'Horizon'
	return pickedVersion.value
		? `${selected.value.name} ${pickedVersion.value.game_version}`
		: selected.value.name
})

// ---- copy from the client's own launcher ----
const sources = ref<ImportSource[]>([])
const copyEnabled = ref(false)
const sourcePath = ref<string | undefined>(undefined)
const copyItems = ref<string[]>([])
let sourceRequest = 0

// Refetch only when the client or game version really changes (a string key,
// so picking a module doesn't count), and keep the old list until the new
// one arrives so nothing jumps.
watch(
	() => `${selected.value?.kind ?? ''}|${pickedVersion.value?.game_version ?? ''}`,
	async () => {
		const request = ++sourceRequest
		const kind = selected.value?.kind
		const gv = pickedVersion.value?.game_version
		if (!kind || !gv) {
			sources.value = []
			return
		}
		try {
			const found = await clientImportSources(kind, gv)
			if (request !== sourceRequest) return
			sources.value = found
			// The folder for the picked version (e.g. Lunar's "1.8" profile for 1.8.9).
			const match = found.find((s) => s.matches_version)
			const keep = found.find((s) => s.path === sourcePath.value)
			sourcePath.value = (match ?? keep ?? found[0])?.path
		} catch {
			if (request === sourceRequest) sources.value = []
		}
	},
	{ immediate: true },
)
const source = computed(() => sources.value.find((s) => s.path === sourcePath.value) ?? null)

// Turning copy on picks the folder for the selected version and, for Lunar,
// the module that folder was played with. Once; after that it's your choice.
watch(copyEnabled, (on) => {
	if (!on) return
	const match = sources.value.find((s) => s.matches_version)
	if (match) sourcePath.value = match.path
	const module = source.value?.module
	if (!module) return
	const v = versions.value.find(
		(x) => x.name === gameVersion.value && x.id.endsWith(`|${module}`),
	)
	if (v) variant.value = v.detail ?? v.id
})

// Servers, settings, packs and mods by default; screenshots and worlds on request.
watch(sourcePath, () => {
	copyItems.value = (source.value?.items ?? [])
		.map((i) => i.id)
		.filter((id) =>
			['servers', 'options', 'resourcepacks', 'shaderpacks', 'mods'].includes(id),
		)
})
const sourceOptions = computed<ComboboxOption<string>[]>(() =>
	sources.value.map((s) => ({
		value: s.path,
		label: s.label,
		subLabel: s.matches_version ? 'Same version' : undefined,
	})),
)
function toggleItem(id: string, on: boolean) {
	copyItems.value = on ? [...copyItems.value, id] : copyItems.value.filter((x) => x !== id)
}

async function create() {
	if (!selected.value || !pickedVersion.value) return
	creating.value = true
	const meta = selected.value
	const version = pickedVersion.value
	try {
		const iconPath = await clientIcon(meta.kind).catch(() => null)
		const job = await install_create_instance({
			name: name.value.trim() || defaultName.value,
			gameVersion: version.game_version,
			loader: version.loader,
			loaderVersion: version.loader === 'vanilla' ? null : 'stable',
			iconPath,
		})
		const instanceId = installJobInstanceId(job)
		if (!instanceId) throw new Error('The instance could not be created')
		const install = await setupClient(instanceId, meta.kind, version)
		queryClient.setQueryData(['studio-client', instanceId], install)
		// Clients bring their own packs: keep the app's synced resource and data
		// packs out of them (they'd fill the Content tab). Settings > Syncing can
		// turn it back on per instance.
		await getInstance(instanceId)
			.then((created) =>
				created
					? edit(instanceId, {
							synced_options: {
								...created.synced_options,
								resource_packs: false,
								data_packs: false,
							},
						})
					: undefined,
			)
			.catch(() => {})
		if (copyEnabled.value && source.value && copyItems.value.length) {
			await clientImport(meta.kind, instanceId, source.value.path, copyItems.value).catch((e) =>
				handleError(e as Error),
			)
		}
		// Shows up in the downloads menu while it runs.
		void runClientPrepare(appEvents, instanceId, meta.name, clientLogo(meta), job.job_id)
			.catch((e) => handleError(e as Error))
			.finally(() => queryClient.invalidateQueries({ queryKey: ['studio-client', instanceId] }))
		trackEvent('InstanceCreate', { source: 'ClientInstallation' })
		modal.value?.hide()
		await router.push(`/instance/${encodeURIComponent(instanceId)}/client`)
	} catch (e) {
		handleError(e as Error)
	} finally {
		creating.value = false
	}
}

defineExpose({ show })
</script>

<template>
	<NewModal ref="modal" header="Client installation" width="38rem" :animate-resize="false">
		<div class="flex w-[34rem] max-w-full flex-col gap-6">
			<!-- Step 1: which client -->
			<template v-if="!selected">
				<span class="font-semibold text-contrast">Choose a client</span>
				<div class="grid grid-cols-2 gap-3">
					<button
						v-for="meta in CLIENTS"
						:key="meta.kind"
						class="group flex cursor-pointer flex-col items-start gap-3 rounded-[20px] border border-solid border-surface-5 bg-surface-3 p-4 text-left transition-all hover:border-brand hover:bg-surface-4 active:scale-[0.98]"
						@click="pick(meta)"
					>
						<div class="flex w-full items-start justify-between gap-2">
							<img
								:src="clientLogo(meta)"
								alt=""
								class="size-12 rounded-2xl border border-solid border-surface-5 bg-surface-2 object-contain"
							/>
							<HelpTip :text="meta.notes.join(' ')" />
						</div>
						<div class="flex flex-col gap-1">
							<span class="flex items-center gap-1 font-semibold text-contrast">
								{{ meta.name }}
								<ChevronRightIcon
									class="size-4 text-secondary opacity-0 transition-opacity group-hover:opacity-100"
								/>
							</span>
							<span class="text-sm leading-snug text-secondary">{{ meta.tagline }}</span>
						</div>
					</button>
				</div>
			</template>

			<!-- Step 2: version, name, copy -->
			<template v-else>
				<!-- The picked client, like Modrinth's "installing this project" card -->
				<div class="flex items-center gap-3 rounded-[20px] bg-surface-2 p-3">
					<img
						:src="clientLogo(selected)"
						alt=""
						class="size-14 shrink-0 rounded-2xl border border-solid border-surface-5 object-contain"
					/>
					<div class="flex min-w-0 flex-1 flex-col gap-0.5">
						<span class="flex items-center gap-2 font-semibold text-contrast">
							{{ selected.name }}
							<HelpTip :text="selected.notes.join(' ')" />
						</span>
						<span class="truncate text-sm text-secondary">{{ selected.tagline }}</span>
					</div>
					<Button type="quiet" @click="selected = null">
						<ChevronLeftIcon /> Change
					</Button>
				</div>

				<div v-if="loading" class="flex items-center gap-2 py-6 text-secondary">
					<SpinnerIcon class="size-5 animate-spin" /> Loading {{ selected.name }}'s versions…
				</div>
				<div
					v-else-if="loadError"
					class="flex flex-col gap-2 rounded-2xl border border-solid border-red bg-highlight-red p-3 text-sm"
				>
					<span class="font-semibold text-contrast">
						Couldn't load {{ selected.name }}'s versions
					</span>
					<span class="break-words text-secondary">{{ loadError }}</span>
					<Button class="w-fit" @click="pick(selected)">Try again</Button>
				</div>
				<template v-else>
					<div class="flex flex-col gap-2">
						<span class="font-semibold text-contrast">Name</span>
						<Input v-model="name" :placeholder="defaultName" />
					</div>

					<!-- Version: the common ones as chips, everything in Modrinth's searchable dropdown -->
					<div class="flex flex-col gap-2">
						<div class="flex items-center justify-between">
							<span class="font-semibold text-contrast">
								{{ selected.kind === 'horizon' ? 'Build' : 'Game version' }}
							</span>
							<button
								v-if="hiddenCount > 0"
								class="cursor-pointer border-0 bg-transparent p-0 text-sm font-semibold text-secondary transition-colors hover:text-contrast"
								@click="showAll = !showAll"
							>
								{{ showAll ? 'Show fewer' : `All ${versionNames.length} versions` }}
							</button>
						</div>
						<Combobox
							v-if="showAll"
							v-model="gameVersion"
							:options="allVersionOptions"
							searchable
							sync-with-selection
							placeholder="Select a version"
							search-placeholder="Search versions, e.g. 1.8"
						/>
						<Chips
							v-else
							v-model="gameVersion"
							:items="featured"
							:capitalize="false"
							:format-label="(n: string) => (isRecommended(n) ? `${n} · Recommended` : n)"
						/>
					</div>

					<div v-if="variants.length > 1 && selected.variantLabel" class="flex flex-col gap-2">
						<span class="flex items-center gap-2 font-semibold text-contrast">
							{{ selected.variantLabel }}
							<HelpTip v-if="VARIANT_HELP[selected.kind]" :text="VARIANT_HELP[selected.kind]" />
						</span>
						<Chips v-model="variant" :items="variants" :capitalize="false" />
					</div>

					<!-- Copy from the client's own launcher -->
					<div
						v-if="sources.length"
						class="flex flex-col gap-3 rounded-[20px] bg-surface-2 p-4"
					>
						<div class="flex items-center gap-3">
							<div class="flex min-w-0 flex-1 flex-col gap-0.5">
								<span class="font-semibold text-contrast">Copy from {{ selected.name }}</span>
								<span class="text-sm text-secondary">
									Bring servers, settings and packs over from {{ selected.name }}'s own launcher.
								</span>
							</div>
							<Toggle v-model="copyEnabled" />
						</div>
						<template v-if="copyEnabled">
							<Combobox
								v-if="sources.length > 1"
								v-model="sourcePath"
								:options="sourceOptions"
								placeholder="Choose a folder"
							/>
							<div v-if="source" class="grid grid-cols-2 gap-2">
								<Checkbox
									v-for="item in source.items"
									:key="item.id"
									:model-value="copyItems.includes(item.id)"
									:label="item.detail ? `${item.label} (${item.detail})` : item.label"
									@update:model-value="(v: boolean) => toggleItem(item.id, v)"
								/>
							</div>
						</template>
					</div>
				</template>

				<div class="flex justify-between gap-2">
					<Button type="outlined" @click="selected = null">
						<ChevronLeftIcon /> Back
					</Button>
					<Button
						type="colored"
						color="brand"
						:disabled="loading || !pickedVersion || creating"
						@click="create"
					>
						<SpinnerIcon v-if="creating" class="animate-spin" />
						<PlusIcon v-else />
						Create instance
					</Button>
				</div>
			</template>
		</div>
	</NewModal>
</template>
