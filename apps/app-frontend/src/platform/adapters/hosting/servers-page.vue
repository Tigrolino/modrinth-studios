<script setup lang="ts">
import type { Labrinth } from '@modrinth/api-client'
import { ServerStackIcon } from '@modrinth/assets'
import { injectModrinthClient, ServersManagePageIndex } from '@modrinth/ui'
import { useQuery } from '@tanstack/vue-query'
import { computed } from 'vue'

import { config } from '@/config'
import PterodactylServersSection from '@/platform/pterodactyl/PterodactylServersSection.vue'
import { listAllPteroServers } from '@/platform/pterodactyl/server'
import { useRootBreadcrumb } from '@/shared/breadcrumbs'

const stripePublishableKey = (config.stripePublishableKey as string) || ''

const client = injectModrinthClient()

useRootBreadcrumb({
	slot: 'root',
	id: 'servers',
	label: 'Hosting',
	to: '/hosting/manage/',
	visual: { type: 'icon', component: ServerStackIcon },
})

const { data: products } = useQuery({
	queryKey: ['billing', 'products'],
	queryFn: () => client.labrinth.billing_internal.getProducts(),
})

const resolvedProducts = computed<Labrinth.Billing.Internal.Product[]>(() => products.value ?? [])

// Modrinth Studios: Pterodactyl panels are listed alongside Modrinth servers.
const { data: pteroPanels } = useQuery({
	queryKey: ['pterodactyl', 'servers'],
	queryFn: listAllPteroServers,
	staleTime: 15_000,
})
const hasPteroPanels = computed(() => (pteroPanels.value?.length ?? 0) > 0)
</script>

<template>
	<ServersManagePageIndex
		:stripe-publishable-key="stripePublishableKey"
		:products="resolvedProducts"
		:has-external-servers="hasPteroPanels"
	>
		<!-- Modrinth Studios addition: self-hosted Pterodactyl servers -->
		<template #header-actions>
			<PterodactylServersSection part="actions" />
		</template>
		<template #sections="{ search }">
			<PterodactylServersSection part="list" :search="search" />
		</template>
	</ServersManagePageIndex>
</template>
