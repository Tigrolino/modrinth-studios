<script setup lang="ts">
/**
 * Modrinth Studios: the "Client" tab of a client installation's settings.
 * The client's own choices (build, module, pack, channel), its files, its
 * account, and copying things over from the client's own launcher.
 */
import {
	CheckIcon,
	CopyIcon,
	DownloadIcon,
	FolderOpenIcon,
	LogInIcon,
	SpinnerIcon,
} from '@modrinth/assets'
import {
	Button,
	Checkbox,
	Chips,
	Combobox,
	type ComboboxOption,
	injectNotificationManager,
} from '@modrinth/ui'
import { homeDir, join } from '@tauri-apps/api/path'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import dayjs from 'dayjs'
import { computed, ref, watch } from 'vue'

import {
	findRunningClientTask,
	runClientPrepare,
} from '@/components/ui/download-manager/studio-client-tasks'
import { syncClientLoader } from '@/features/studio-clients/loader-sync'
import { useClientAccount } from '@/features/studio-clients/use-client-account'
import {
	clientImport,
	clientImportSources,
	clientLogo,
	clientMeta,
	clientVersions,
	getClientInstall,
	noriskSignedIn,
	noriskSignIn,
	setupClient,
} from '@/helpers/studio-clients'
import { openPath } from '@/helpers/utils.js'
import { injectAppEvents } from '@/providers/app-events'

import { injectInstanceSettings } from './instance-settings-context'

const { handleError, addNotification } = injectNotificationManager()
const { instance } = injectInstanceSettings()
const queryClient = useQueryClient()
const appEvents = injectAppEvents()

const installQuery = useQuery({
	queryKey: computed(() => ['studio-client', instance.value.id]),
	queryFn: () => getClientInstall(instance.value.id),
	networkMode: 'always',
})
const install = computed(() => installQuery.data.value ?? null)
const meta = computed(() => clientMeta(install.value?.client))

// ---- build / module / pack / channel ----
const versionsQuery = useQuery({
	queryKey: computed(() => ['studio-client-versions', install.value?.client]),
	queryFn: () => clientVersions(install.value!.client),
	enabled: computed(() => !!install.value && !!meta.value?.variantLabel),
	staleTime: 10 * 60_000,
})
/** Other choices for the same Minecraft version (so the instance stays valid). */
const choices = computed(() =>
	(versionsQuery.data.value ?? []).filter(
		(v) =>
			v.game_version === install.value?.game_version &&
			(install.value?.client === 'lunar' || v.loader === instance.value.loader),
	),
)
const isHorizon = computed(() => install.value?.client === 'horizon')
const choiceLabel = (v: { name: string; detail: string | null; id: string }) =>
	isHorizon.value ? v.name : (v.detail ?? v.id)
const currentChoice = ref<string | null>(null)
watch(
	[install, choices],
	() => {
		const cur = choices.value.find((v) => v.id === install.value?.version_id)
		currentChoice.value = cur ? choiceLabel(cur) : null
	},
	{ immediate: true },
)
const buildOptions = computed<ComboboxOption<string>[]>(() =>
	choices.value.map((v) => ({
		value: choiceLabel(v),
		label: choiceLabel(v),
		subLabel: v.recommended ? 'Recommended' : isHorizon.value ? (v.detail ?? undefined) : undefined,
	})),
)
const switching = ref(false)
async function switchTo(label: string | null) {
	const v = choices.value.find((c) => choiceLabel(c) === label)
	if (!v || !install.value || v.id === install.value.version_id || !meta.value) return
	switching.value = true
	try {
		const updated = await setupClient(instance.value.id, install.value.client, v)
		queryClient.setQueryData(['studio-client', instance.value.id], updated)
		await syncClientLoader(instance.value, updated)
		await runClientPrepare(appEvents, instance.value.id, meta.value.name, clientLogo(meta.value))
		await queryClient.invalidateQueries({ queryKey: ['studio-client', instance.value.id] })
		await queryClient.invalidateQueries({ queryKey: ['norisk-pack-mods', instance.value.id] })
	} catch (e) {
		handleError(e as Error)
	} finally {
		switching.value = false
	}
}

// ---- files ----
const runningTask = computed(() => findRunningClientTask(instance.value.id))
async function updateFiles() {
	if (!meta.value) return
	try {
		await runClientPrepare(appEvents, instance.value.id, meta.value.name, clientLogo(meta.value))
		await queryClient.invalidateQueries({ queryKey: ['studio-client', instance.value.id] })
	} catch (e) {
		handleError(e as Error)
	}
}
const lastChecked = computed(() =>
	install.value?.checked_at ? dayjs.unix(install.value.checked_at).format('MMM D, YYYY h:mm A') : null,
)

// ---- account ----
const { account } = useClientAccount()
const noriskQuery = useQuery({
	queryKey: computed(() => ['minecraft-account', account.value?.id, 'norisk-signed-in']),
	queryFn: noriskSignedIn,
	enabled: computed(() => install.value?.client === 'norisk' && !!account.value),
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

async function openLunarSettings() {
	try {
		await openPath(await join(await homeDir(), '.lunarclient', 'settings', 'game'))
	} catch (e) {
		handleError(e as Error)
	}
}

// ---- copy from the client's own launcher ----
const sourcesQuery = useQuery({
	queryKey: computed(() => ['studio-client-import', install.value?.client, install.value?.game_version]),
	queryFn: () => clientImportSources(install.value!.client, install.value!.game_version),
	enabled: computed(() => !!install.value),
})
const sources = computed(() => sourcesQuery.data.value ?? [])
const sourcePath = ref<string | undefined>(undefined)
watch(
	sources,
	(s) => {
		if (!s.some((x) => x.path === sourcePath.value)) sourcePath.value = s[0]?.path
	},
	{ immediate: true },
)
const source = computed(() => sources.value.find((s) => s.path === sourcePath.value) ?? null)
const copyItems = ref<string[]>([])
watch(source, () => (copyItems.value = []))
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
const copying = ref(false)
async function copyNow() {
	if (!install.value || !source.value || !copyItems.value.length) return
	copying.value = true
	try {
		await clientImport(install.value.client, instance.value.id, source.value.path, copyItems.value)
		addNotification({
			type: 'success',
			title: 'Copied',
			text: `Copied from ${source.value.label}. Files that were already here were kept.`,
		})
		copyItems.value = []
	} catch (e) {
		handleError(e as Error)
	} finally {
		copying.value = false
	}
}
</script>

<template>
	<div v-if="install && meta" class="flex flex-col gap-6">
		<!-- Which client -->
		<div class="flex items-center gap-4 border-0 border-b border-solid border-surface-5 pb-6">
			<img :src="clientLogo(meta)" alt="" class="size-14 rounded-2xl" />
			<div class="flex min-w-0 flex-col">
				<span class="text-lg font-semibold text-contrast">{{ meta.name }}</span>
				<span class="text-secondary">
					{{ install.version_name
					}}<template v-if="install.variant_name"> · {{ install.variant_name }}</template>
				</span>
			</div>
		</div>

		<!-- Build / module / pack / channel -->
		<div v-if="meta.variantLabel || isHorizon">
			<h2 class="m-0 text-lg font-semibold text-contrast">
				{{ isHorizon ? 'Build' : meta.variantLabel }}
			</h2>
			<div class="my-2.5">
				<div v-if="versionsQuery.isPending.value" class="flex items-center gap-2 text-secondary">
					<SpinnerIcon class="size-4 animate-spin" /> Loading…
				</div>
				<Combobox
					v-else-if="isHorizon"
					:model-value="currentChoice ?? undefined"
					:options="buildOptions"
					searchable
					:disabled="switching || !!runningTask"
					placeholder="Choose a build"
					@update:model-value="(v: string) => switchTo(v)"
				/>
				<Chips
					v-else-if="choices.length > 1"
					:model-value="currentChoice"
					:items="choices.map(choiceLabel)"
					:capitalize="false"
					@update:model-value="(v: string) => switchTo(v)"
				/>
				<span v-else class="text-secondary">This version has only one {{ meta.variantLabel?.toLowerCase() }}.</span>
			</div>
			<p class="m-0">
				<template v-if="isHorizon">
					Nightly updates itself; any other build stays exactly as it is.
				</template>
				<template v-else-if="install.client === 'lunar'">
					Lunar + Fabric and Lunar + Forge also run the mods in this instance's Content tab.
				</template>
				<template v-else-if="install.client === 'norisk'">
					NoRisk's packs pick which of their mods are included.
				</template>
				<template v-else>Which LabyMod release channel to play.</template>
				Switching downloads the new files right away.
			</p>
		</div>

		<!-- Files -->
		<div>
			<h2 class="m-0 text-lg font-semibold text-contrast">Client files</h2>
			<div class="my-2.5 flex flex-wrap items-center gap-2">
				<Button :disabled="!!runningTask || switching" @click="updateFiles">
					<SpinnerIcon v-if="runningTask" class="animate-spin" />
					<DownloadIcon v-else />
					{{ runningTask ? runningTask.message : 'Check for updates' }}
				</Button>
				<Button v-if="install.client === 'lunar'" @click="openLunarSettings">
					<FolderOpenIcon /> Lunar's settings folder
				</Button>
			</div>
			<p class="m-0">
				<template v-if="lastChecked">Last checked {{ lastChecked }}. </template>
				Updates are also checked when you press Play.
				<template v-if="install.client === 'lunar'">
					Lunar keeps its in-game settings, mods layout and cosmetics choices in its own folder,
					shared with Lunar's launcher.
				</template>
			</p>
		</div>

		<!-- Account -->
		<div v-if="install.client === 'norisk'">
			<h2 class="m-0 text-lg font-semibold text-contrast">NoRisk account</h2>
			<div class="my-2.5">
				<Button :disabled="signingIn || !account" @click="signInNoRisk">
					<SpinnerIcon v-if="signingIn" class="animate-spin" />
					<CheckIcon v-else-if="noriskQuery.data.value" class="text-brand" />
					<LogInIcon v-else />
					{{ noriskQuery.data.value ? `Signed in as ${noriskQuery.data.value}` : 'Sign in to NoRisk' }}
				</Button>
			</div>
			<p class="m-0">
				NoRisk signs in with your selected Minecraft account, for capes, friends and its other
				online features.
			</p>
		</div>

		<!-- Copy from the client's own launcher -->
		<div>
			<h2 class="m-0 text-lg font-semibold text-contrast">Copy from {{ meta.name }}</h2>
			<template v-if="sources.length">
				<div class="my-2.5 flex flex-col gap-3">
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
					<Button class="w-fit" :disabled="!copyItems.length || copying" @click="copyNow">
						<SpinnerIcon v-if="copying" class="animate-spin" />
						<CopyIcon v-else />
						Copy
					</Button>
				</div>
				<p class="m-0">
					Copies from {{ meta.name }}'s own launcher on this computer. Files already in this
					instance are kept; the server list and settings are replaced.
				</p>
			</template>
			<p v-else class="m-0 mt-2.5">
				{{ sourcesQuery.isPending.value
					? 'Looking for it…'
					: `${meta.name}'s own launcher wasn't found on this computer.` }}
			</p>
		</div>
	</div>
</template>
