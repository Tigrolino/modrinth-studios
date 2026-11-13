<template>
	<!-- Modrinth Studios: compact title-bar variant (shown while the sidebar is closed) -->
	<div v-if="compact" ref="compactRoot" class="relative">
		<button
			type="button"
			v-tooltip="selectedAccount ? selectedAccount.profile.name : undefined"
			class="flex h-8 cursor-pointer items-center gap-2 rounded-full border border-solid border-surface-4 bg-button-bg py-0 pl-1 pr-2.5 text-contrast hover:brightness-110 max-lg:gap-1 max-lg:pr-1.5"
			:aria-expanded="compactOpen"
			@click="accounts.length === 0 ? login() : (compactOpen = !compactOpen)"
		>
			<Avatar
				size="24px"
				:src="
					selectedAccount ? avatarUrl : 'https://launcher-files.modrinth.com/assets/steve_head.png'
				"
			/>
			<!-- Name only on wider windows; narrow ones show just the head (+ chevron). -->
			<span class="max-w-[9rem] truncate text-sm font-semibold max-lg:hidden">
				{{
					accounts.length === 0
						? formatMessage(messages.signInToMinecraft)
						: selectedAccount
							? selectedAccount.profile.name
							: formatMessage(messages.selectAccount)
				}}
			</span>
			<ChevronDownIcon
				v-if="accounts.length > 0"
				class="size-4 text-secondary transition-transform"
				:class="{ 'rotate-180': compactOpen }"
			/>
		</button>
		<div
			v-if="compactOpen"
			class="absolute right-0 top-[calc(100%+0.5rem)] z-[60] w-72 overflow-hidden rounded-xl border border-solid border-surface-5 bg-bg-raised py-1 shadow-xl"
		>
			<div v-for="account in accounts" :key="account.profile.id" class="flex items-center gap-1">
				<button
					class="button-base flex min-w-0 flex-grow cursor-pointer items-center gap-2 border-0 bg-transparent p-2"
					@click="setAccount(account).then(() => (compactOpen = false))"
				>
					<RadioButtonCheckedIcon
						v-if="selectedAccount && selectedAccount.profile.id === account.profile.id"
						class="h-5 w-5 shrink-0 text-brand"
					/>
					<RadioButtonIcon v-else class="h-5 w-5 shrink-0 text-secondary" />
					<Avatar :src="getAccountAvatarUrl(account)" size="24px" />
					<span
						class="truncate"
						:class="
							selectedAccount && selectedAccount.profile.id === account.profile.id
								? 'font-semibold text-contrast'
								: 'text-primary'
						"
					>
						{{ account.profile.name }}
					</span>
				</button>
				<IconButton
					v-tooltip="formatMessage(messages.removeAccount)"
					type="quiet"
					color="red"
					:label="formatMessage(messages.removeAccount)"
					class="mr-2"
					@click="logout(account.profile.id)"
				>
					<TrashIcon />
				</IconButton>
			</div>
			<div class="px-2 pb-1 pt-2">
				<Button class="w-full" :disabled="loginDisabled" @click="login()">
					<PlusIcon />
					{{ formatMessage(messages.addAccount) }}
				</Button>
			</div>
		</div>
	</div>
	<div
		v-else-if="accounts.length === 0"
		class="flex flex-col gap-3 bg-button-bg border border-solid border-surface-5 rounded-xl p-3 mt-2"
	>
		<span>{{ formatMessage(messages.notSignedIn) }}</span>
		<Button type="colored" color="brand" :disabled="loginDisabled" @click="login()">
			<LogInIcon v-if="!loginDisabled" />
			<SpinnerIcon v-else class="animate-spin" />
			{{ formatMessage(messages.signInToMinecraft) }}
		</Button>
	</div>
	<Accordion
		v-else
		class="w-full mt-2 bg-button-bg border border-solid border-surface-5 rounded-xl overflow-clip"
		button-class="button-base w-full bg-transparent px-3 py-2 border-0 cursor-pointer"
		:open-by-default="false"
	>
		<template #title>
			<div class="flex gap-2 w-full min-w-0">
				<Avatar
					size="36px"
					:src="
						selectedAccount
							? avatarUrl
							: 'https://launcher-files.modrinth.com/assets/steve_head.png'
					"
				/>
				<div class="flex flex-col items-start w-full min-w-0">
					<span class="truncate w-full text-left">{{
						selectedAccount ? selectedAccount.profile.name : formatMessage(messages.selectAccount)
					}}</span>
					<span class="text-secondary text-xs">{{ formatMessage(messages.minecraftAccount) }}</span>
				</div>
			</div>
		</template>
		<div class="bg-button-bg pt-1 pb-2 border-0 border-t border-solid border-surface-5">
			<template v-if="accounts.length > 0">
				<div v-for="account in accounts" :key="account.profile.id" class="flex gap-1 items-center">
					<button
						class="flex items-center flex-shrink flex-grow overflow-clip gap-2 p-2 border-0 bg-transparent cursor-pointer button-base min-w-0"
						@click="setAccount(account)"
					>
						<RadioButtonCheckedIcon
							v-if="selectedAccount && selectedAccount.profile.id === account.profile.id"
							class="w-5 h-5 text-brand shrink-0"
						/>
						<RadioButtonIcon v-else class="w-5 h-5 text-secondary shrink-0" />
						<Avatar :src="getAccountAvatarUrl(account)" size="24px" />
						<p
							class="m-0 truncate min-w-0"
							:class="
								selectedAccount && selectedAccount.profile.id === account.profile.id
									? 'text-contrast font-semibold'
									: 'text-primary'
							"
						>
							{{ account.profile.name }}
						</p>
					</button>
					<IconButton
						v-tooltip="formatMessage(messages.removeAccount)"
						type="quiet"
						color="red"
						:label="formatMessage(messages.removeAccount)"
						class="mr-2 !bg-button-bg !text-primary ![box-shadow:var(--shadow-button)] hover:!bg-red focus-visible:!bg-red hover:!text-[var(--color-accent-contrast)] focus-visible:!text-[var(--color-accent-contrast)]"
						@click="logout(account.profile.id)"
					>
						<TrashIcon />
					</IconButton>
				</div>
			</template>
			<div class="flex flex-col gap-2 px-2 pt-2">
				<Button
					v-if="accounts.length > 0"
					class="w-full !bg-button-bg !text-primary ![box-shadow:var(--shadow-button)]"
					:disabled="loginDisabled"
					@click="login()"
				>
					<PlusIcon />
					{{ formatMessage(messages.addAccount) }}
				</Button>
			</div>
		</div>
	</Accordion>
</template>

<script setup lang="ts">
import {
	ChevronDownIcon,
	LogInIcon,
	PlusIcon,
	RadioButtonCheckedIcon,
	RadioButtonIcon,
	SpinnerIcon,
	TrashIcon,
} from '@modrinth/assets'
import {
	Accordion,
	Avatar,
	Button,
	defineMessages,
	IconButton,
	injectNotificationManager,
	useVIntl,
} from '@modrinth/ui'
import type { Ref } from 'vue'
import { onClickOutside } from '@vueuse/core'
import { computed, onMounted, onUnmounted, ref } from 'vue'

import { useAppEvent } from '@/composables/use-app-event'
import { handleSevereError } from '@/composables/use-error.js'
import { trackEvent } from '@/helpers/analytics'
import {
	get_default_user,
	login as login_flow,
	remove_user,
	set_default_user,
	users,
} from '@/helpers/auth'
import { getPlayerHeadUrl } from '@/helpers/rendering/player-head'
import type { Skin } from '@/helpers/skins'
import { get_available_skins } from '@/helpers/skins'

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()

const emit = defineEmits<{
	change: []
}>()

/** Modrinth Studios: compact pill + dropdown for the title bar. */
const props = withDefaults(defineProps<{ compact?: boolean }>(), { compact: false })
const compactOpen = ref(false)
const compactRoot = ref<HTMLElement | null>(null)
onClickOutside(compactRoot, () => (compactOpen.value = false))

// Keep the sidebar card and the title-bar pill in sync when either changes accounts.
const ACCOUNT_CHANGED_EVENT = 'studio:minecraft-account-changed'
function announceChange() {
	window.dispatchEvent(new CustomEvent(ACCOUNT_CHANGED_EVENT, { detail: props.compact }))
}
function onAccountChanged(event: Event) {
	if ((event as CustomEvent).detail !== props.compact) void refreshValues()
}
onMounted(() => window.addEventListener(ACCOUNT_CHANGED_EVENT, onAccountChanged))
onUnmounted(() => window.removeEventListener(ACCOUNT_CHANGED_EVENT, onAccountChanged))

type MinecraftCredential = {
	profile: {
		id: string
		name: string
	}
}

const accounts: Ref<MinecraftCredential[]> = ref([])
const loginDisabled = ref(false)
const defaultUser = ref<string | undefined>()
const equippedSkin = ref<Skin | null>(null)
const equippedHeadUrl = ref<string>()
let headRequest = 0

async function updateHeadUrl(skin: Skin | null) {
	const request = ++headRequest
	if (equippedHeadUrl.value) URL.revokeObjectURL(equippedHeadUrl.value)
	equippedHeadUrl.value = undefined
	if (!skin) return
	const url = await getPlayerHeadUrl(skin)
	if (request !== headRequest) URL.revokeObjectURL(url)
	else equippedHeadUrl.value = url
}

onUnmounted(() => {
	headRequest++
	if (equippedHeadUrl.value) URL.revokeObjectURL(equippedHeadUrl.value)
})

async function refreshValues() {
	defaultUser.value = await get_default_user().catch(handleError)
	const userList = await users().catch(handleError)
	accounts.value = Array.isArray(userList) ? [...userList] : []
	accounts.value.sort((a, b) => (a.profile?.name ?? '').localeCompare(b.profile?.name ?? ''))

	try {
		const skins = await get_available_skins()
		equippedSkin.value = skins.find((skin) => skin.is_equipped) ?? null

		await updateHeadUrl(equippedSkin.value)
	} catch {
		equippedSkin.value = null
		void updateHeadUrl(null)
	}
}

async function setEquippedSkin(skin: Skin) {
	equippedSkin.value = skin

	try {
		await updateHeadUrl(skin)
	} catch (error) {
		console.warn('Failed to get head render for equipped skin:', error)
	}
}

function setLoginDisabled(value: boolean) {
	loginDisabled.value = value
}

defineExpose({
	refreshValues,
	setEquippedSkin,
	setLoginDisabled,
	login,
	loginDisabled,
})

await refreshValues()

const selectedAccount = computed(() =>
	accounts.value.find((account) => account.profile.id === defaultUser.value),
)

const avatarUrl = computed(() => {
	if (equippedSkin.value?.texture_key) {
		const cachedUrl = equippedHeadUrl.value
		if (cachedUrl) {
			return cachedUrl
		}
		return `https://mc-heads.net/avatar/${equippedSkin.value.texture_key}/128`
	}
	if (selectedAccount.value?.profile?.id) {
		return `https://mc-heads.net/avatar/${selectedAccount.value.profile.id}/128`
	}
	return 'https://launcher-files.modrinth.com/assets/steve_head.png'
})

function getAccountAvatarUrl(account: MinecraftCredential) {
	if (
		account.profile.id === selectedAccount.value?.profile?.id &&
		equippedSkin.value?.texture_key
	) {
		const cachedUrl = equippedHeadUrl.value
		if (cachedUrl) {
			return cachedUrl
		}
	}
	return `https://mc-heads.net/avatar/${account.profile.id}/128`
}

async function setAccount(account: MinecraftCredential) {
	defaultUser.value = account.profile.id
	await set_default_user(account.profile.id).catch(handleError)
	await refreshValues()
	announceChange()
	emit('change')
}

async function login() {
	loginDisabled.value = true
	const loggedIn = await login_flow().catch(handleSevereError)

	if (loggedIn) {
		await setAccount(loggedIn)
	}

	trackEvent('AccountLogIn')
	loginDisabled.value = false
}

async function logout(id: string) {
	await remove_user(id).catch(handleError)
	await refreshValues()
	if (!selectedAccount.value && accounts.value.length > 0) {
		await setAccount(accounts.value[0])
	} else {
		announceChange()
		emit('change')
	}
	trackEvent('AccountLogOut')
}

useAppEvent('process', async (e) => {
	if (e.event === 'launched') {
		await refreshValues()
	}
})

const messages = defineMessages({
	notSignedIn: {
		id: 'minecraft-account.not-signed-in',
		defaultMessage: 'Not signed in',
	},
	addAccount: {
		id: 'minecraft-account.add-account',
		defaultMessage: 'Add account',
	},
	removeAccount: {
		id: 'minecraft-account.remove-account',
		defaultMessage: 'Remove account',
	},
	selectAccount: {
		id: 'minecraft-account.select-account',
		defaultMessage: 'Select account',
	},
	minecraftAccount: {
		id: 'minecraft-account.label',
		defaultMessage: 'Minecraft account',
	},
	signInToMinecraft: {
		id: 'minecraft-account.sign-in',
		defaultMessage: 'Sign in to Minecraft',
	},
})
</script>
