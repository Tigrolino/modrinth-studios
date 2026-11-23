<script setup lang="ts">
/**
 * Modrinth Studios: the Capes / Cosmetics tab of a client installation,
 * laid out like Modrinth's Skins page (3D preview on the left).
 * - NoRisk: its cape library (browse, your capes, equip, upload), through
 *   NoRisk's cosmetics API like their launcher.
 * - Lunar, LabyMod, Horizon: their cosmetics are owned through their own
 *   accounts and worn in game, so this shows your skin and their stores.
 */
import { CheckIcon, ExternalIcon, SpinnerIcon, UploadIcon, XIcon } from '@modrinth/assets'
import {
	Admonition,
	Button,
	Chips,
	injectNotificationManager,
	SkinPreviewRenderer,
} from '@modrinth/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { open, openUrl } from '@/platform/studio-client-desktop'
import { computedAsync, useIntersectionObserver } from '@vueuse/core'
import { computed, ref, watch } from 'vue'

import { useClientAccount } from '@/features/studio-clients/use-client-account'
import { get_available_skins, get_normalized_skin_texture } from '@/helpers/skins'
import {
	type ClientKind,
	clientLogo,
	type NoRiskCape,
	noriskCapeEquip,
	noriskCapesBrowse,
	noriskCapesMine,
	noriskCapeTexture,
	noriskCapeUnequip,
	noriskCapeUpload,
	noriskCapeUrl,
} from '@/helpers/studio-clients'

const props = defineProps<{ kind: ClientKind; clientName: string }>()
const { handleError, addNotification } = injectNotificationManager()
const queryClient = useQueryClient()

// ---- your skin ----
const { account, loading: loadingAccount } = useClientAccount()
const skinsQuery = useQuery({
	queryKey: computed(() => ['minecraft-account', account.value?.id, 'studio-cosmetics-skin']),
	queryFn: get_available_skins,
	enabled: computed(() => !!account.value),
})
const equippedSkin = computed(
	() => (skinsQuery.data.value ?? []).find((s) => s.is_equipped) ?? null,
)
const skinTexture = computedAsync(async () =>
	equippedSkin.value ? await get_normalized_skin_texture(equippedSkin.value).catch(() => '') : '',
)
const playerName = computed(() => account.value?.name ?? '')

// ---- NoRisk capes ----
const tabs = ['Popular', 'This week', 'Newest', 'My capes']
const tab = ref('Popular')
const capes = ref<NoRiskCape[]>([])
const cursor = ref<string | null>(null)
const loadingCapes = ref(false)
const capeError = ref<string | null>(null)
const selected = ref<NoRiskCape | null>(null)
let capeRequest = 0

// NoRisk's API has no "which cape am I wearing" call, so the cape equipped
// from here is remembered per account and shown (and previewed) again the
// next time this tab opens.
const equippedKey = computed(() => `studio-norisk-equipped-cape:${account.value?.id ?? ''}`)
function readEquipped(): string | null {
	try {
		return localStorage.getItem(equippedKey.value)
	} catch {
		return null
	}
}
const equippedHash = ref<string | null>(null)
watch(equippedKey, () => (equippedHash.value = readEquipped()), { immediate: true })
function rememberEquipped(hash: string | null) {
	equippedHash.value = hash
	try {
		if (hash) localStorage.setItem(equippedKey.value, hash)
		else localStorage.removeItem(equippedKey.value)
	} catch {
		// Storage unavailable: it just won't be remembered.
	}
}
function equippedCape(): NoRiskCape | null {
	if (!equippedHash.value) return null
	return (
		capes.value.find((c) => c.hash === equippedHash.value) ?? {
			hash: equippedHash.value,
			uses: 0,
			elytra: true,
			accepted: true,
			creation_date: 0,
		}
	)
}
const capeSentinel = ref<HTMLElement | null>(null)
const capeEndVisible = ref(false)
useIntersectionObserver(
	capeSentinel,
	([entry]) => {
		capeEndVisible.value = !!entry?.isIntersecting
	},
	{ rootMargin: '400px' },
)
watch(
	[capeEndVisible, cursor, loadingCapes, capeError],
	() => {
		if (capeEndVisible.value && cursor.value && !loadingCapes.value && !capeError.value)
			void loadCapes(false)
	},
	{ flush: 'post' },
)

async function loadCapes(reset: boolean) {
	if (props.kind !== 'norisk' || !account.value) return
	if (!reset && loadingCapes.value) return
	const request = ++capeRequest
	const requestedTab = tab.value
	if (reset) {
		capes.value = []
		cursor.value = null
	}
	capeError.value = null
	loadingCapes.value = true
	try {
		if (requestedTab === 'My capes') {
			const result = await noriskCapesMine()
			if (request === capeRequest) {
				capes.value = result
				cursor.value = null
			}
			return
		}
		const page = await noriskCapesBrowse(
			requestedTab === 'Newest' ? 'newest' : 'mostUsed',
			requestedTab === 'This week' ? 'weekly' : null,
			reset ? null : cursor.value,
		)
		if (request !== capeRequest) return
		capes.value = [
			...new Map(
				[...(reset ? [] : capes.value), ...page.capes].map((cape) => [cape.hash, cape]),
			).values(),
		]
		cursor.value = page.next_cursor === cursor.value ? null : page.next_cursor
	} catch (e) {
		if (request === capeRequest) capeError.value = (e as Error)?.message ?? String(e)
	} finally {
		if (request === capeRequest) loadingCapes.value = false
	}
}
watch(
	[tab, () => props.kind, () => account.value?.id],
	() => {
		capeRequest++
		capes.value = []
		cursor.value = null
		selected.value = equippedCape()
		loadingCapes.value = false
		capeError.value = null
		void loadCapes(true)
	},
	{ immediate: true },
)

const selectedTexture = computedAsync(async () =>
	selected.value
		? await noriskCapeTexture(selected.value.hash, !selected.value.accepted).catch(() => undefined)
		: undefined,
)

const busy = ref(false)
async function equip() {
	if (!selected.value) return
	busy.value = true
	try {
		await noriskCapeEquip(selected.value.hash)
		rememberEquipped(selected.value.hash)
		addNotification({
			title: 'Cape equipped',
			text: 'Others with NoRisk will see it in game.',
			type: 'success',
		})
	} catch (e) {
		handleError(e as Error)
	} finally {
		busy.value = false
	}
}
async function unequip() {
	busy.value = true
	try {
		await noriskCapeUnequip()
		rememberEquipped(null)
		selected.value = null
		addNotification({ title: 'Cape removed', type: 'success' })
	} catch (e) {
		handleError(e as Error)
	} finally {
		busy.value = false
	}
}
async function upload() {
	const path = await open({ filters: [{ name: 'Cape', extensions: ['png'] }], multiple: false })
	if (!path || Array.isArray(path)) return
	busy.value = true
	try {
		await noriskCapeUpload(path)
		addNotification({
			title: 'Cape uploaded',
			text: 'NoRisk reviews new capes before anyone else can see them.',
			type: 'success',
		})
		if (tab.value === 'My capes') await loadCapes(true)
		else tab.value = 'My capes'
	} catch (e) {
		handleError(e as Error)
	} finally {
		busy.value = false
	}
}

// ---- stores for the others ----
const stores = computed(() => {
	const name = encodeURIComponent(playerName.value ?? '')
	switch (props.kind) {
		case 'lunar':
			return {
				text: "Lunar's cosmetics, emotes and Lunar+ belong to your Minecraft account. Buy them in Lunar's store, then wear them in game from the cosmetics menu (Right Shift).",
				primary: { label: 'Open Lunar store', url: 'https://store.lunarclient.com' },
				links: [{ label: 'Lunar+', url: 'https://store.lunarclient.com/category/plus/' }],
			}
		case 'labymod':
			return {
				text: "LabyMod's cosmetics and emotes belong to your Minecraft account. Get them in the LabyMod shop, then wear them from the LabyMod menu in game. Your laby.net profile shows what you own.",
				primary: { label: 'Open LabyMod shop', url: 'https://www.labymod.net/en/shop' },
				links: [{ label: 'Your laby.net profile', url: `https://laby.net/@${name}` }],
			}
		case 'horizon':
			return {
				text: "Horizon's cosmetics are seen by Horizon players. Try them on your own skin in Horizon's shop. Wearing them needs Horizon's own launcher (its Wardrobe) for now.",
				primary: {
					label: 'Open Horizon shop',
					url: `https://playhorizon.dev/shop${name ? `?name=${name}` : ''}`,
				},
				links: [{ label: 'Horizon launcher', url: 'https://playhorizon.dev/docs/install' }],
			}
		default:
			return null
	}
})

function refresh() {
	void queryClient.invalidateQueries({ queryKey: ['minecraft-account'] })
	void loadCapes(true)
}
defineExpose({ refresh })
</script>

<template>
	<div class="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(24rem,30%)_1fr]">
		<!-- Preview, like the Skins page -->
		<div
			class="flex flex-col items-center gap-4 rounded-2xl border border-solid border-surface-4 bg-bg-raised p-4 lg:sticky lg:top-4 lg:self-start lg:h-[calc(100dvh-13rem)] lg:min-h-[38rem]"
		>
			<div class="h-[30rem] w-full min-h-0 lg:h-auto lg:flex-1">
				<SkinPreviewRenderer
					v-if="skinTexture"
					:texture-src="skinTexture"
					:cape-src="selectedTexture"
					:variant="equippedSkin?.variant ?? 'CLASSIC'"
					:nametag="playerName || undefined"
					:initial-rotation="Math.PI + Math.PI / 6"
				/>
				<div v-else class="flex h-full items-center justify-center text-secondary">
					<SpinnerIcon
						v-if="loadingAccount || skinsQuery.isFetching.value"
						class="size-6 animate-spin"
					/>
					<span v-else>{{
						account ? 'Skin preview unavailable' : 'Select a Minecraft account to preview your skin'
					}}</span>
				</div>
			</div>
			<template v-if="kind === 'norisk'">
				<p class="m-0 text-center text-sm text-secondary">
					Selecting a cape previews it. Use Equip to wear it in game.
				</p>
				<div class="flex w-full flex-col gap-2">
					<Button
						type="colored"
						color="brand"
						size="lg"
						class="w-full"
						:disabled="!selected || busy || !account || selected.hash === equippedHash"
						@click="equip"
					>
						<SpinnerIcon v-if="busy" class="animate-spin" />
						<CheckIcon v-else />
						{{
							!selected
								? 'Pick a cape to preview it'
								: selected.hash === equippedHash
									? 'Equipped'
									: 'Equip this cape'
						}}
					</Button>
					<div class="flex gap-2">
						<Button class="flex-1" :disabled="busy || !account" @click="upload">
							<UploadIcon />
							Upload cape
						</Button>
						<Button class="flex-1" :disabled="busy || !account" @click="unequip">
							<XIcon />
							Remove cape
						</Button>
					</div>
				</div>
			</template>
		</div>

		<!-- NoRisk's cape library -->
		<div v-if="kind === 'norisk'" class="flex min-w-0 flex-col gap-4">
			<Admonition
				v-if="!account && !loadingAccount"
				type="info"
				header="Select a Minecraft account"
				body="Use the account menu in the title bar to sign in and manage your NoRisk capes."
			/>
			<Admonition v-if="capeError" type="critical" header="Could not load capes" :body="capeError">
				<template #actions><Button @click="loadCapes(true)">Retry</Button></template>
			</Admonition>
			<Chips v-model="tab" :items="tabs" :capitalize="false" />
			<div v-if="loadingCapes && !capes.length" class="flex items-center gap-2 py-8 text-secondary">
				<SpinnerIcon class="size-5 animate-spin" /> Loading capes…
			</div>
			<p
				v-else-if="account && !capeError && !capes.length"
				class="m-0 py-8 text-center text-secondary"
			>
				{{ tab === 'My capes' ? "You haven't made any capes yet." : 'No capes found.' }}
			</p>
			<div v-else class="grid grid-cols-[repeat(auto-fill,minmax(13rem,1fr))] gap-3">
				<button
					v-for="cape in capes"
					:key="cape.hash"
					class="group flex cursor-pointer flex-col items-center gap-2 rounded-2xl border border-solid p-3 transition-[filter,transform] hover:brightness-110 active:scale-[0.97]"
					:class="
						selected?.hash === cape.hash
							? 'border-brand bg-brand-highlight'
							: 'border-surface-4 bg-bg-raised'
					"
					@click="selected = cape"
				>
					<!-- The cape's front, cut from its 64×32 texture -->
					<div
						class="cape-front"
						:style="{
							backgroundImage: `url(${noriskCapeUrl(cape.hash, !cape.accepted)})`,
						}"
					/>
					<span
							v-if="cape.hash === equippedHash"
							class="rounded-full bg-brand px-2 py-0.5 text-xs font-bold text-brand-inverted"
						>
							Equipped
						</span>
					<span v-else class="text-xs font-semibold text-secondary">
							{{ cape.accepted ? `${cape.uses.toLocaleString()} uses` : 'In review' }}
					</span>
				</button>
			</div>
			<div ref="capeSentinel" class="h-px" aria-hidden="true" />
			<Button
				v-if="cursor && tab !== 'My capes'"
				class="self-center"
				:disabled="loadingCapes"
				@click="loadCapes(false)"
			>
				<SpinnerIcon v-if="loadingCapes" class="animate-spin" />
				Load more
			</Button>
		</div>

		<!-- Other clients: their stores -->
		<div v-else-if="stores" class="flex min-w-0 flex-col gap-4">
			<div
				class="flex items-center gap-4 rounded-2xl border border-solid border-surface-4 bg-bg-raised p-5"
			>
				<img :src="clientLogo(kind)" alt="" class="size-14 rounded-xl" />
				<div class="flex min-w-0 flex-col gap-1">
					<span class="text-lg font-bold text-contrast">{{ clientName }} cosmetics</span>
					<span class="text-secondary">{{ stores.text }}</span>
				</div>
			</div>
			<div class="flex flex-wrap gap-2">
				<Button type="colored" color="brand" size="lg" @click="openUrl(stores.primary.url)">
					<ExternalIcon />
					{{ stores.primary.label }}
				</Button>
				<Button v-for="link in stores.links" :key="link.url" size="lg" @click="openUrl(link.url)">
					<ExternalIcon />
					{{ link.label }}
				</Button>
			</div>
		</div>
	</div>
</template>

<style scoped>
/* Front of a cape: 10×16 pixels at (1,1) of a 64×32 texture, scaled 12x. */
.cape-front {
	width: 120px;
	height: 192px;
	background-size: 768px 384px;
	background-position: -12px -12px;
	background-repeat: no-repeat;
	image-rendering: pixelated;
	border-radius: 4px;
	box-shadow: 0 4px 14px rgb(0 0 0 / 0.35);
}
</style>
