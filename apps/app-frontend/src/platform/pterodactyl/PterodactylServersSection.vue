<script setup lang="ts">
/**
 * Modrinth Studios addition: Pterodactyl panels on the Hosting page.
 *
 * Rendered twice through slots of ServersManagePageIndex so it blends in
 * with the Modrinth list:
 *  - part="actions": the "Connect panel" button (next to "New server") + its modal
 *  - part="list":    one section per panel, styled like "Your servers"
 */
import { PlusIcon, RefreshCwIcon, TrashIcon } from '@modrinth/assets'
import {
	Button,
	IconButton,
	injectNotificationManager,
	Input,
	NewModal,
	ServerListing,
} from '@modrinth/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import { addPanel, removePanel } from './bridge'
import { listAllPteroServers } from './server'

const props = withDefaults(defineProps<{ part: 'actions' | 'list'; search?: string }>(), {
	search: '',
})

const queryClient = useQueryClient()
const { addNotification } = injectNotificationManager()

const { data, isLoading, isFetching, refetch } = useQuery({
	queryKey: ['pterodactyl', 'servers'],
	queryFn: listAllPteroServers,
	staleTime: 15_000,
	refetchInterval: 30_000,
})

const panels = computed(() => {
	const query = props.search.trim().toLowerCase()
	return (data.value ?? []).map((entry) => ({
		...entry,
		filtered: query
			? entry.servers.filter((s) => s.name.toLowerCase().includes(query))
			: entry.servers,
	}))
})

const addModal = ref<InstanceType<typeof NewModal>>()
const panelName = ref('')
const panelUrl = ref('')
const apiKey = ref('')
const connecting = ref(false)
const connectError = ref<string | null>(null)

function openAddModal() {
	panelName.value = ''
	panelUrl.value = ''
	apiKey.value = ''
	connectError.value = null
	addModal.value?.show()
}

// Opened from Modrinth's "new server" dialog for people without a Modrinth account.
function onConnectRequest() {
	if (props.part === 'actions') openAddModal()
}
onMounted(() => window.addEventListener('studio:connect-pterodactyl', onConnectRequest))
onBeforeUnmount(() => window.removeEventListener('studio:connect-pterodactyl', onConnectRequest))

async function connect() {
	if (!panelUrl.value.trim() || !apiKey.value.trim()) {
		connectError.value = 'Enter the panel URL and a client API key.'
		return
	}
	connecting.value = true
	connectError.value = null
	try {
		const panel = await addPanel(panelName.value, panelUrl.value, apiKey.value)
		addModal.value?.hide()
		addNotification({
			type: 'success',
			title: 'Panel connected',
			text: `${panel.name} was added to Hosting.`,
		})
		await queryClient.invalidateQueries({ queryKey: ['pterodactyl'] })
	} catch (error) {
		connectError.value =
			(error as { message?: string })?.message ??
			(error instanceof Error ? error.message : String(error))
	} finally {
		connecting.value = false
	}
}

async function disconnect(panelId: string, name: string) {
	if (!window.confirm(`Remove ${name} from Studio? Nothing on the panel is changed.`)) return
	await removePanel(panelId)
	await queryClient.invalidateQueries({ queryKey: ['pterodactyl'] })
}
</script>

<template>
	<template v-if="part === 'actions'">
		<Button class="!shrink-0" @click="openAddModal">
			<PlusIcon />
			Connect panel
		</Button>

		<NewModal
			ref="addModal"
			header="Connect Pterodactyl panel"
			width="34rem"
			max-width="calc(100vw - 2rem)"
		>
			<div class="flex flex-col gap-4">
				<p class="m-0 text-secondary">
					On your panel, open
					<span class="font-semibold text-contrast">Account → API Credentials</span> and create a
					key. Use a client key (it starts with <code>ptlc_</code>), not an application key.
				</p>
				<div class="flex flex-col gap-1">
					<span class="font-semibold text-contrast">Panel URL</span>
					<Input
						v-model="panelUrl"
						type="text"
						placeholder="http://192.168.1.10"
						wrapper-class="w-full"
					/>
				</div>
				<div class="flex flex-col gap-1">
					<span class="font-semibold text-contrast">Client API key</span>
					<Input v-model="apiKey" type="password" placeholder="ptlc_…" wrapper-class="w-full" />
				</div>
				<div class="flex flex-col gap-1">
					<span class="font-semibold text-contrast">
						Name <span class="font-normal text-secondary">(optional)</span>
					</span>
					<Input v-model="panelName" type="text" placeholder="Home server" wrapper-class="w-full" />
				</div>
				<p v-if="connectError" class="m-0 text-red">{{ connectError }}</p>
				<div class="flex justify-end gap-2">
					<Button type="outlined" @click="addModal?.hide()">Cancel</Button>
					<Button type="colored" color="brand" :disabled="connecting" @click="connect">
						{{ connecting ? 'Connecting…' : 'Connect' }}
					</Button>
				</div>
			</div>
		</NewModal>
	</template>

	<template v-else>
		<div
			v-if="isLoading"
			class="flex animate-pulse flex-row items-center gap-4 rounded-2xl border-[1px] border-solid border-button-bg bg-bg-raised p-4"
		>
			<div class="size-16 rounded-xl bg-button-bg"></div>
			<div class="flex flex-1 flex-col gap-2">
				<div class="h-6 w-48 rounded bg-button-bg"></div>
				<div class="h-4 w-64 rounded bg-button-bg opacity-75"></div>
			</div>
		</div>

		<section v-for="entry in panels" :key="entry.panel.id" class="flex flex-col gap-3">
			<div class="group flex items-center justify-between gap-2">
				<h2 class="m-0 flex min-w-0 items-baseline gap-2 text-xl font-semibold text-primary">
					<span class="truncate">{{ entry.panel.name }}</span>
					<span class="truncate text-sm font-normal text-secondary">{{ entry.panel.url }}</span>
				</h2>
				<div
					class="flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100"
				>
					<IconButton
						v-tooltip="'Refresh'"
						label="Refresh"
						type="quiet"
						size="sm"
						:disabled="isFetching"
						@click="refetch()"
					>
						<RefreshCwIcon :class="isFetching ? 'animate-spin' : ''" />
					</IconButton>
					<IconButton
						v-tooltip="'Remove panel'"
						label="Remove panel"
						type="quiet"
						color="red"
						size="sm"
						@click="disconnect(entry.panel.id, entry.panel.name)"
					>
						<TrashIcon />
					</IconButton>
				</div>
			</div>
			<div
				v-if="entry.error"
				class="rounded-2xl border border-solid border-red bg-highlight-red p-4 text-sm text-red"
			>
				Couldn't load servers from this panel: {{ entry.error }}
			</div>
			<div v-else-if="entry.servers.length === 0" class="text-secondary">
				This API key can't see any servers on the panel.
			</div>
			<div v-else-if="entry.filtered.length === 0" class="text-secondary">
				No servers on this panel match your search.
			</div>
			<ul v-else class="m-0 flex flex-col gap-3 p-0">
				<ServerListing v-for="server in entry.filtered" :key="server.server_id" v-bind="server" />
			</ul>
		</section>
	</template>
</template>
