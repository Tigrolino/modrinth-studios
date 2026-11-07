<script setup lang="ts">
import { defineMessages, useScrollIndicator, useVIntl } from '@modrinth/ui'
import { OverlayScrollbars } from 'overlayscrollbars'
import { computed, onBeforeUnmount, onMounted, onScopeDispose, ref } from 'vue'

import { useTheme } from '@/composables/use-theme'
import PrideFundraiserBanner from '@/features/campaigns/pride-fundraiser-banner.vue'
import FriendsList from '@/features/friends/friends-list/index.vue'
import AccountsCard from '@/features/minecraft-accounts/accounts-card.vue'
import type { MinecraftAccountsActions } from '@/features/minecraft-accounts/context'
import NewsSidebar from '@/features/news/news-sidebar.vue'
import OnboardingChecklist from '@/features/onboarding/checklist.vue'
import type { ModrinthCredentials } from '@/helpers/mr_auth'
import { injectOnboardingChecklist } from '@/providers/onboarding-checklist'

import Promotion from './promotion.vue'

const props = defineProps<{
	credentials: ModrinthCredentials | null | undefined
	hasPlus: boolean
	showAd: boolean
	visible: boolean
	forceSidebar?: boolean
	requestSignIn: () => void
}>()

const emit = defineEmits<{
	'create-instance': []
	'login-modrinth': []
}>()

const { formatMessage } = useVIntl()
const appTheme = useTheme()
const { hasLoggedIntoMinecraft, hasLoggedIntoModrinth, showChecklist } = injectOnboardingChecklist()
const showFriendsList = computed(() => !showChecklist.value || hasLoggedIntoModrinth.value)
const accounts = ref<InstanceType<typeof AccountsCard> | null>(null)
const friends = ref<InstanceType<typeof FriendsList> | null>(null)
const scrollContainer = ref<HTMLElement | null>(null)
const scrollViewport = ref<HTMLElement | null>(null)
const upgradeHeight = ref(0)
const promotionPaddingStyle = computed(() =>
	props.showAd ? { paddingBottom: `calc(1rem + ${upgradeHeight.value}px)` } : undefined,
)
const { showTopFade, showBottomFade, checkScrollState } = useScrollIndicator(scrollViewport)

let removeScrollbarListener: (() => void) | undefined

// Modrinth Studios addition: show the default panel (news/friends) only when
// the page-teleported sidebar content is genuinely absent. Between route
// changes the teleport target is briefly empty, so we debounce and also
// suppress while Suspense reports a page is loading.
const teleportTarget = ref<HTMLElement | null>(null)
const teleportEmpty = ref(false)
const teleportSuppressed = ref(false)
let teleportTimeout: ReturnType<typeof setTimeout> | undefined
let teleportObserver: MutationObserver | undefined

function updateTeleportEmpty() {
	clearTimeout(teleportTimeout)
	const el = teleportTarget.value
	const empty = !el || el.childNodes.length === 0
	if (!empty) {
		teleportEmpty.value = false
		return
	}
	teleportTimeout = setTimeout(() => {
		teleportEmpty.value = !teleportTarget.value || teleportTarget.value.childNodes.length === 0
	}, 100)
}

function suppressTeleportEmpty() {
	teleportSuppressed.value = true
	clearTimeout(teleportTimeout)
	teleportEmpty.value = false
}

function resolveTeleportEmpty() {
	teleportSuppressed.value = false
	updateTeleportEmpty()
}

const showDefaultContent = computed(
	() => teleportEmpty.value && !teleportSuppressed.value && !props.forceSidebar,
)

onMounted(() => {
	if (teleportTarget.value) {
		teleportObserver = new MutationObserver(updateTeleportEmpty)
		teleportObserver.observe(teleportTarget.value, { childList: true })
		updateTeleportEmpty()
	}
})

onBeforeUnmount(() => {
	clearTimeout(teleportTimeout)
	teleportObserver?.disconnect()
})

onMounted(() => {
	if (!scrollContainer.value) return
	const scrollbars = OverlayScrollbars(scrollContainer.value)
	if (!scrollbars) return
	scrollViewport.value = scrollbars.elements().viewport
	removeScrollbarListener = scrollbars.on('updated', checkScrollState)
})

onScopeDispose(() => removeScrollbarListener?.())

const minecraftAccounts = computed<MinecraftAccountsActions | null>(() => {
	const card = accounts.value
	if (!card) return null
	return {
		loginDisabled: card.loginDisabled,
		refreshValues: card.refreshValues,
		setEquippedSkin: card.setEquippedSkin,
		setLoginDisabled: card.setLoginDisabled,
		login: card.login,
	}
})

function showAddFriendModal() {
	friends.value?.showAddFriendModal()
}

defineExpose({
	minecraftAccounts,
	showAddFriendModal,
	suppressTeleportEmpty,
	resolveTeleportEmpty,
})

const scrollbarOptions = Object.freeze({
	overflow: {
		x: 'hidden',
		y: 'scroll',
	},
})

const messages = defineMessages({
	playingAs: {
		id: 'app.sidebar.playing-as',
		defaultMessage: 'Playing as',
	},
})
</script>

<template>
	<div
		class="app-sidebar mt-px relative flex h-[calc(100vh-var(--top-bar-height))] w-[300px] shrink-0 flex-col overflow-visible border-0 border-l-[1px] border-solid border-[--brand-gradient-border] before:pointer-events-none before:absolute before:inset-y-0 before:-left-8 before:w-8 before:content-['']"
		:class="{
			'before:shadow-[-15px_0_15px_-15px_rgba(0,0,0,0.1)_inset]': appTheme.advancedRendering,
		}"
	>
		<div class="relative flex min-h-0 grow shrink flex-col">
			<div
				ref="scrollContainer"
				v-overlay-scrollbars="scrollbarOptions"
				class="app-sidebar-scrollable relative min-h-0 grow shrink"
				data-overlayscrollbars-initialize
			>
				<OnboardingChecklist
					@create-instance="emit('create-instance')"
					@login-minecraft="accounts?.login()"
					@login-modrinth="emit('login-modrinth')"
				/>
				<div
					id="sidebar-teleport-target"
					ref="teleportTarget"
					class="sidebar-teleport-content empty:hidden"
					:style="promotionPaddingStyle"
				></div>
				<div
					class="sidebar-default-content hidden"
					:class="{
						'sidebar-enabled': visible,
						'sidebar-default-content--shown': showDefaultContent,
					}"
				>
					<div
						v-show="hasLoggedIntoMinecraft"
						class="p-4 border-0 border-b-[1px] border-[--brand-gradient-border] border-solid"
					>
						<h3 class="text-base text-primary font-medium m-0">
							{{ formatMessage(messages.playingAs) }}
						</h3>
						<Suspense>
							<AccountsCard ref="accounts" />
						</Suspense>
					</div>
					<div
						v-show="showFriendsList"
						class="p-4 border-0 border-b-[1px] border-[--brand-gradient-border] border-solid"
					>
						<Suspense>
							<FriendsList
								ref="friends"
								:credentials="credentials ?? null"
								:sign-in="requestSignIn"
							/>
						</Suspense>
					</div>
					<PrideFundraiserBanner
						class="p-4 border-0 border-b-[1px] border-[--brand-gradient-border] border-solid"
					/>
					<NewsSidebar :style="promotionPaddingStyle" />
				</div>
			</div>
			<div
				class="sidebar-scroll-fade pointer-events-none absolute inset-x-0 top-0 z-10 h-8 rotate-180 transition-opacity duration-200 motion-reduce:transition-none"
				:class="showTopFade ? 'opacity-100' : 'opacity-0'"
				aria-hidden="true"
			/>
			<div
				class="sidebar-scroll-fade pointer-events-none absolute inset-x-0 bottom-0 z-10 h-8 transition-opacity duration-200 motion-reduce:transition-none"
				:class="showAd || showBottomFade ? 'opacity-100' : 'opacity-0'"
				aria-hidden="true"
			/>
		</div>
		<Promotion v-if="showAd" @upgrade-height="upgradeHeight = $event" />
	</div>
</template>

<style scoped>
.app-sidebar {
	background: var(--brand-gradient-bg);
	--color-button-bg: var(--brand-gradient-button);
	--surface-4: var(--brand-gradient-button);
	--color-button-bg-hover: var(--brand-gradient-border);
	--surface-5: var(--brand-gradient-border);
	--color-divider: var(--brand-gradient-border);
	--color-divider-dark: var(--brand-gradient-border);
}

.sidebar-default-content--shown.sidebar-enabled {
	display: contents;
}

.sidebar-scroll-fade {
	background: var(--brand-gradient-fade-out-color);
}
</style>
