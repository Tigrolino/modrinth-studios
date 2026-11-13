<script setup lang="ts">
import { LibraryIcon, PlayIcon, PlusIcon } from '@modrinth/assets'
import {
	ContextMenu,
	defineMessages,
	injectAuth,
	injectModrinthClient,
	injectNotificationManager,
	useVIntl,
} from '@modrinth/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import dayjs from 'dayjs'
import { computed, inject, ref, watch } from 'vue'
import { onBeforeRouteLeave, useRouter } from 'vue-router'

import LibrarySection from '@/components/ui/library/index.vue'
import { libraryScrollTop } from '@/components/ui/library/view-state'
import WelcomeScreen from '@/components/ui/WelcomeScreen.vue'
import RecentWorldsList from '@/components/ui/world/RecentWorldsList.vue'
import { useAppSettings } from '@/composables/use-app-settings.ts'
import { useHiddenInstances } from '@/features/library/hidden-instances/use-hidden-instances'
import { usePlayMode } from '@/features/play-mode/state'
import { prefetchSharedInstanceLibrary } from '@/features/library/shared-instances/queries'
import { traceStartupStep } from '@/helpers/startup-debug'
import { instanceListQueryOptions } from '@/pages/instance/query-options'
import { skipCreateInstanceOnboarding } from '@/platform/app-lib/onboarding-checklist/commands'
import { injectOnboardingChecklist } from '@/providers/onboarding-checklist'
import { useRootBreadcrumb } from '@/shared/breadcrumbs'

defineOptions({
	name: 'LibraryPage',
})

// Modrinth Studios: `/library` shows only the library (Play mode moves it here).
const props = withDefaults(defineProps<{ libraryOnly?: boolean }>(), { libraryOnly: false })
const router = useRouter()
const playMode = usePlayMode()

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()
const { hasCreatedInstance, isReady } = injectOnboardingChecklist()
const showCreationModal = inject<() => void>('showCreationModal')
const pageOptions = ref<InstanceType<typeof ContextMenu>>()
const appSettings = useAppSettings()
const { hiddenIds: hiddenInstanceIds } = useHiddenInstances()
const auth = injectAuth()
const modrinthClient = injectModrinthClient()
const queryClient = useQueryClient()
onBeforeRouteLeave(() => {
	libraryScrollTop.value = document.querySelector('.app-viewport')?.scrollTop ?? 0
})

const messages = defineMessages({
	home: {
		id: 'app.navigation.home',
		defaultMessage: 'Home',
	},
	newInstance: {
		id: 'app.library.context-menu.create-instance',
		defaultMessage: 'New instance',
	},
	libraryActionsLabel: {
		id: 'app.library.actions.label',
		defaultMessage: 'Library actions',
	},
})

useRootBreadcrumb({
	slot: 'root',
	id: () => (props.libraryOnly ? 'library' : 'home'),
	label: () => (props.libraryOnly ? 'Library' : formatMessage(messages.home)),
	to: () => (props.libraryOnly ? '/library' : '/'),
	visual: () => ({ type: 'icon', component: props.libraryOnly ? LibraryIcon : PlayIcon }),
})

watch(
	() => (isReady.value && !hasCreatedInstance.value ? auth.user.value?.id : null),
	(userId) => {
		if (!userId) return
		prefetchSharedInstanceLibrary(queryClient, modrinthClient, userId).catch((error) =>
			console.warn('Failed to prefetch shared instance library:', error),
		)
	},
	{ immediate: true },
)

function skipWelcomeScreen() {
	skipCreateInstanceOnboarding().catch(handleError)
}

const instancesQuery = useQuery(instanceListQueryOptions())
const instances = computed(() => instancesQuery.data.value ?? [])
if (hasCreatedInstance.value) {
	await traceStartupStep('Load library instances', () => instancesQuery.suspense()).catch(
		handleError,
	)
}

// Play mode: the home screen *is* the chosen instance.
const redirectingToPlayMode = computed(
	() =>
		!props.libraryOnly &&
		!!playMode.playPath.value &&
		instances.value.some((i) => i.id === playMode.state.instanceId),
)
watch(
	redirectingToPlayMode,
	(redirect) => {
		if (redirect && playMode.playPath.value) void router.replace(playMode.playPath.value)
	},
	{ immediate: true },
)

const recentInstances = computed(() =>
	instances.value
		.filter((instance) => !hiddenInstanceIds.value.has(instance.id))
		.sort((a, b) => dayjs(b.last_played ?? b.created).diff(dayjs(a.last_played ?? a.created))),
)

function openPageContextMenu(event: MouseEvent) {
	if (
		!(event.target instanceof HTMLElement) ||
		!event.target.hasAttribute('data-library-page-background')
	) {
		return
	}

	event.preventDefault()
	event.stopPropagation()
	pageOptions.value?.open(event, [
		{
			id: 'new_instance',
			label: formatMessage(messages.newInstance),
			icon: PlusIcon,
			action: () => showCreationModal?.(),
		},
	])
}
</script>

<template>
	<div v-if="redirectingToPlayMode" />
	<WelcomeScreen v-else-if="isReady && !hasCreatedInstance" @skip="skipWelcomeScreen" />
	<div
		v-else-if="isReady"
		data-library-page-background
		class="flex flex-col gap-3 p-6"
		@contextmenu="openPageContextMenu"
	>
		<RecentWorldsList
			v-if="!libraryOnly && recentInstances?.length > 0 && appSettings.showJumpIn"
			:recent-instances="recentInstances"
		/>
		<LibrarySection :instances="instances" />
		<ContextMenu ref="pageOptions" :label="formatMessage(messages.libraryActionsLabel)" />
	</div>
</template>
