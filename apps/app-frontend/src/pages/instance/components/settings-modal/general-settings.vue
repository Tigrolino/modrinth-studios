<script setup lang="ts">
import {
	ClockIcon,
	CopyIcon,
	EditIcon,
	FolderOpenIcon,
	PaletteIcon,
	PlusIcon,
	SettingsIcon,
	SpinnerIcon,
	TrashIcon,
	TriangleAlertIcon,
	UploadIcon,
} from '@modrinth/assets'
import {
	Avatar,
	Button,
	Checkbox,
	Chips,
	commonMessages,
	defineMessages,
	injectNotificationManager,
	Input,
	NewModal,
	TeleportOverflowMenu,
	Toggle,
	useVIntl,
} from '@modrinth/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { open } from '@tauri-apps/plugin-dialog'
import { computed, onBeforeUnmount, onMounted, reactive, type Ref, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import IconEditorModal from '@/components/ui/instance_settings/icon-editor-modal/index.vue'
import PlaytimeCorrectionModal from '@/components/ui/instance_settings/PlaytimeCorrectionModal.vue'
import ConfirmDeleteInstanceModal from '@/components/ui/modal/ConfirmDeleteInstanceModal.vue'
import { useAppEvent } from '@/composables/use-app-event'
import { trackEvent } from '@/helpers/analytics'
import { install_duplicate_instance } from '@/helpers/install'
import { edit, edit_icon, getInstanceIconUrl, remove } from '@/helpers/instance'
import { get_by_instance_id as getInstanceProcesses } from '@/helpers/process'
// Modrinth Studios addition, see helpers/shared-profiles.ts
import {
	createSharedProfile,
	deleteSharedProfile,
	getInstanceSharedProfile,
	listSharedProfiles,
	setInstanceSharedProfile,
	type SharedProfile,
	type SharedProfileItemFlags,
	updateSharedProfileItems,
} from '@/helpers/shared-profiles'
import type { GameInstance, InstanceIconConfig } from '@/helpers/types'

import { instancePlaytimeCorrectionQueryOptions } from '../../query-options'
import { injectInstanceSettings } from './instance-settings-context'

const { handleError } = injectNotificationManager()
const { formatMessage } = useVIntl()
const router = useRouter()
const queryClient = useQueryClient()

const deleteConfirmModal = ref()
const iconEditorModal = ref<InstanceType<typeof IconEditorModal> | null>(null)
const playtimeCorrectionModal = ref<InstanceType<typeof PlaytimeCorrectionModal> | null>(null)

const { instance } = injectInstanceSettings()

// Modrinth Studios addition: manual playtime correction (see
// PlaytimeCorrectionModal.vue). Kept as a separate value from
// `submitted_time_played`/`recent_time_played` — the real Modrinth-tracked
// total — and only combined with it for display. Read through the shared
// vue-query cache (rather than fetching independently) so saving a new value
// in the modal updates this display immediately, instead of only on the next
// time this component happens to mount.
const playtimeCorrectionQuery = useQuery(
	computed(() => instancePlaytimeCorrectionQueryOptions(instance.value.id)),
)
const correctionSeconds = computed(() => playtimeCorrectionQuery.data.value ?? 0)

const modrinthPlaytimeSeconds = computed(
	() => instance.value.recent_time_played + instance.value.submitted_time_played,
)

function formatTotalPlaytime(seconds: number): string {
	const totalMinutes = Math.floor(seconds / 60)
	const hours = Math.floor(totalMinutes / 60)
	const minutes = totalMinutes % 60
	if (hours <= 0 && minutes <= 0) return '0h'
	return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`
}

function formatCorrection(seconds: number): string {
	const hours = Math.round((seconds / 3600) * 10) / 10
	if (hours === 0) return '0h'
	return hours > 0 ? `+${hours}h` : `${hours}h`
}

type ReleaseChannel = GameInstance['update_channel']
const releaseChannelOptions: ReleaseChannel[] = ['release', 'beta', 'alpha']

const title = ref(instance.value.name)
const icon: Ref<string | undefined> = ref(instance.value.icon_path)
const iconConfig = ref<InstanceIconConfig | null>(instance.value.icon_config ?? null)
const savingReleaseChannel = ref(false)
const selectedReleaseChannel = ref<ReleaseChannel>(instance.value.update_channel)
const releaseChannelDisabledItems = computed<ReleaseChannel[]>(() =>
	savingReleaseChannel.value ? [...releaseChannelOptions] : [],
)

const installing = computed(() => instance.value.install_stage !== 'installed')

watch(
	() => [instance.value.id, instance.value.icon_path, instance.value.icon_config] as const,
	() => {
		icon.value = instance.value.icon_path
		iconConfig.value = instance.value.icon_config ?? null
	},
)

async function duplicateInstance() {
	await install_duplicate_instance(instance.value.id).catch(handleError)
	trackEvent('InstanceDuplicate', {
		loader: instance.value.loader,
		game_version: instance.value.game_version,
	})
}

function formatReleaseChannelLabel(channel: ReleaseChannel) {
	switch (channel) {
		case 'release':
			return formatMessage(messages.updateChannelRelease)
		case 'beta':
			return formatMessage(messages.updateChannelBeta)
		case 'alpha':
			return formatMessage(messages.updateChannelAlpha)
	}
}

function formatReleaseChannelDescription(channel: ReleaseChannel) {
	switch (channel) {
		case 'release':
			return formatMessage(messages.updateChannelReleaseDescription)
		case 'beta':
			return formatMessage(messages.updateChannelBetaDescription)
		case 'alpha':
			return formatMessage(messages.updateChannelAlphaDescription)
	}
}

watch(
	() => [instance.value.id, instance.value.update_channel] as const,
	() => {
		if (!savingReleaseChannel.value) {
			selectedReleaseChannel.value = instance.value.update_channel
		}
	},
)

watch(selectedReleaseChannel, async (channel, previousChannel) => {
	const previousReleaseChannel = previousChannel ?? instance.value.update_channel
	if (channel === instance.value.update_channel) return

	savingReleaseChannel.value = true
	const instanceId = instance.value.id
	await edit(instanceId, { update_channel: channel })
		.then(() => queryClient.invalidateQueries({ queryKey: ['linkedModpackInfo', instanceId] }))
		.catch((error) => {
			selectedReleaseChannel.value = previousReleaseChannel
			handleError(error)
		})
	savingReleaseChannel.value = false
})

async function resetIcon() {
	try {
		await edit_icon(instance.value.id, null)
		icon.value = undefined
		iconConfig.value = null
	} catch (error) {
		handleError(error)
		return
	}
	trackEvent('InstanceRemoveIcon')
}

async function setIcon() {
	const value = await open({
		multiple: false,
		filters: [
			{
				name: 'Image',
				extensions: ['png', 'jpeg', 'svg', 'webp', 'gif', 'jpg'],
			},
		],
	})

	if (!value) return

	try {
		await edit_icon(instance.value.id, value)
		icon.value = value
		iconConfig.value = null
	} catch (error) {
		handleError(error)
		return
	}

	trackEvent('InstanceSetIcon')
}

function openIconEditor() {
	iconEditorModal.value?.show()
	trackEvent(iconConfig.value ? 'InstanceEditCreatedIcon' : 'InstanceCreateIcon')
}

function onGeneratedIconSaved(iconPath: string, config: InstanceIconConfig) {
	icon.value = iconPath
	iconConfig.value = config
	trackEvent('InstanceSaveCreatedIcon')
}

const editInstanceObject = computed(() => ({
	name: title.value.trim().substring(0, 80) ?? 'Instance',
}))

// Modrinth Studios addition: renaming an instance here now also renames its
// on-disk folder to match (see rename_instance_folder.rs) — that's a real
// filesystem rename, not just a DB write, so it needs debouncing now in a
// way the old plain-DB-name-update never did (this used to fire on every
// single keystroke, which was harmless for a DB column but would otherwise
// rename the folder dozens of times while someone is mid-typing a new name).
let saveNameTimeout: ReturnType<typeof setTimeout> | null = null
function saveName() {
	if (saveNameTimeout) clearTimeout(saveNameTimeout)
	saveNameTimeout = null
	if (removing.value) return
	edit(instance.value.id, editInstanceObject.value).catch(handleError)
}
watch(
	title,
	() => {
		if (removing.value) return
		if (saveNameTimeout) clearTimeout(saveNameTimeout)
		saveNameTimeout = setTimeout(saveName, 1000)
	},
	{ deep: true },
)
// Flush immediately if the settings modal closes (or this tab is switched
// away from) mid-debounce, rather than silently dropping a typed rename.
onBeforeUnmount(() => {
	if (saveNameTimeout) saveName()
})

// Modrinth Studios addition: shared Minecraft folders — see
// helpers/shared-profiles.ts and packages/app-lib/src/api/shared_profile.rs.
// `currentSharedProfile` is the source of truth for whether this instance is
// currently sharing (fetched from the backend); `selectedSharedProfileId`
// only drives the Chips UI and is kept in sync with it via the watcher
// below, mirroring how `selectedReleaseChannel` above saves on change and
// rolls back on failure.
const sharedProfiles = ref<SharedProfile[]>([])
const currentSharedProfile = ref<SharedProfile | null>(null)
const selectedSharedProfileId = ref<string | null>(null)
const applyingSharedFolder = ref(false)
const creatingSharedFolder = ref(false)
const newSharedFolderName = ref('')
const sharedFolderEnabled = computed(() => currentSharedProfile.value != null)
// Drives the Toggle's visual position. Kept separate from
// `sharedFolderEnabled` so switching it on can reveal the pick-a-folder UI
// below immediately, without the toggle snapping back to "off" until a
// folder is actually chosen and applied.
const toggleOn = ref(false)
watch(sharedFolderEnabled, (enabled) => (toggleOn.value = enabled), { immediate: true })

// Modrinth Studios addition: a shared-folder change is refused by the
// backend while this instance is running (editing the underlying files out
// from under a live game would be unsafe). Tracked separately from
// `applyingSharedFolder` so the UI can proactively disable the toggle with an
// explanation, instead of the change silently failing on click.
const runningProcessCount = ref(0)
const instanceRunning = computed(() => runningProcessCount.value > 0)
async function refreshInstanceRunning() {
	try {
		const processes = await getInstanceProcesses(instance.value.id)
		runningProcessCount.value = Array.isArray(processes) ? processes.length : 0
	} catch {
		// Non-fatal: worst case the toggle is briefly enabled when it
		// shouldn't be, and the backend's own running-check still refuses it.
	}
}
onMounted(refreshInstanceRunning)
watch(() => instance.value.id, refreshInstanceRunning)
useAppEvent('process', refreshInstanceRunning)

// Wraps a shared-folder backend call so its "busy" flag can never get stuck
// `true` for the rest of the session (previously reported: picking a folder
// greyed out every chip until switching settings tabs). A hung/failed call
// now always resets the flag after at most 20s even if something unexpected
// happens on the backend, no matter which busy flag it's guarding.
async function withSharedFolderBusyGuard<T>(
	busy: Ref<boolean>,
	action: () => Promise<T>,
): Promise<T> {
	busy.value = true
	const watchdog = setTimeout(() => {
		busy.value = false
	}, 20_000)
	try {
		return await action()
	} finally {
		clearTimeout(watchdog)
		busy.value = false
	}
}

function runSharedFolderChange<T>(action: () => Promise<T>): Promise<T> {
	return withSharedFolderBusyGuard(applyingSharedFolder, action)
}

async function loadSharedFolderState() {
	try {
		const [profiles, current] = await Promise.all([
			listSharedProfiles(),
			getInstanceSharedProfile(instance.value.id),
		])
		sharedProfiles.value = profiles
		currentSharedProfile.value = current
		selectedSharedProfileId.value = current?.id ?? null
	} catch (error) {
		handleError(error)
	}
}
onMounted(loadSharedFolderState)
watch(() => instance.value.id, loadSharedFolderState)

async function toggleSharedFolder(enabled: boolean) {
	toggleOn.value = enabled

	if (enabled) {
		if (sharedProfiles.value.length === 0) creatingSharedFolder.value = true
		return
	}

	creatingSharedFolder.value = false
	newSharedFolderName.value = ''
	if (!currentSharedProfile.value) return

	try {
		await runSharedFolderChange(() => setInstanceSharedProfile(instance.value.id, null))
		currentSharedProfile.value = null
		selectedSharedProfileId.value = null
	} catch (error) {
		toggleOn.value = true
		handleError(error)
	}
}

watch(selectedSharedProfileId, async (id, previousId) => {
	if (id == null || id === (currentSharedProfile.value?.id ?? null)) return

	try {
		await runSharedFolderChange(() => setInstanceSharedProfile(instance.value.id, id))
		currentSharedProfile.value = sharedProfiles.value.find((profile) => profile.id === id) ?? null
	} catch (error) {
		selectedSharedProfileId.value = previousId ?? null
		handleError(error)
	}
})

async function confirmCreateSharedFolder() {
	const name = newSharedFolderName.value.trim()
	if (!name) return

	try {
		const profile = await runSharedFolderChange(() => createSharedProfile(name, instance.value.id))
		sharedProfiles.value = [...sharedProfiles.value, profile].sort((a, b) =>
			a.name.localeCompare(b.name),
		)
		creatingSharedFolder.value = false
		newSharedFolderName.value = ''
		selectedSharedProfileId.value = profile.id
	} catch (error) {
		handleError(error)
	}
}

function cancelCreateSharedFolder() {
	creatingSharedFolder.value = false
	newSharedFolderName.value = ''
}

// Modrinth Studios addition: "Manage shared folder" — lets the user pick
// exactly which items a shared folder shares (e.g. worlds but not options),
// and delete the shared folder entirely. See helpers/shared-profiles.ts.
const manageModal = ref<InstanceType<typeof NewModal> | null>(null)
const savingManagedItems = ref(false)
const deletingSharedFolder = ref(false)
const confirmingDeleteSharedFolder = ref(false)
const managedItems = reactive<SharedProfileItemFlags>({
	share_saves: true,
	share_config: true,
	share_resourcepacks: true,
	share_options: true,
	share_servers: true,
})

const isOwnerOfCurrentSharedFolder = computed(
	() => currentSharedProfile.value?.owner_instance_id === instance.value.id,
)

function openManageSharedFolder() {
	if (!currentSharedProfile.value) return
	managedItems.share_saves = currentSharedProfile.value.share_saves
	managedItems.share_config = currentSharedProfile.value.share_config
	managedItems.share_resourcepacks = currentSharedProfile.value.share_resourcepacks
	managedItems.share_options = currentSharedProfile.value.share_options
	managedItems.share_servers = currentSharedProfile.value.share_servers
	confirmingDeleteSharedFolder.value = false
	manageModal.value?.show()
}

async function saveManagedItems() {
	if (!currentSharedProfile.value) return
	try {
		const updated = await withSharedFolderBusyGuard(savingManagedItems, () =>
			updateSharedProfileItems(currentSharedProfile.value!.id, { ...managedItems }),
		)
		currentSharedProfile.value = updated
		sharedProfiles.value = sharedProfiles.value.map((profile) =>
			profile.id === updated.id ? updated : profile,
		)
		manageModal.value?.hide()
	} catch (error) {
		handleError(error)
	}
}

async function deleteCurrentSharedFolder() {
	if (!currentSharedProfile.value) return
	if (!confirmingDeleteSharedFolder.value) {
		confirmingDeleteSharedFolder.value = true
		return
	}

	const id = currentSharedProfile.value.id
	try {
		await withSharedFolderBusyGuard(deletingSharedFolder, () => deleteSharedProfile(id))
		sharedProfiles.value = sharedProfiles.value.filter((profile) => profile.id !== id)
		currentSharedProfile.value = null
		selectedSharedProfileId.value = null
		toggleOn.value = false
		manageModal.value?.hide()
	} catch (error) {
		handleError(error)
	} finally {
		confirmingDeleteSharedFolder.value = false
	}
}

const removing = ref(false)
async function removeInstance() {
	removing.value = true
	const path = instance.value.id

	trackEvent('InstanceRemove', {
		loader: instance.value.loader,
		game_version: instance.value.game_version,
	})

	await router.push({ path: '/' })
	await remove(path).catch(handleError)
}

const messages = defineMessages({
	name: {
		id: 'instance.settings.tabs.general.name',
		defaultMessage: 'Name',
	},
	icon: {
		id: 'instance.settings.tabs.general.icon',
		defaultMessage: 'Icon',
	},
	editIcon: {
		id: 'instance.settings.tabs.general.edit-icon',
		defaultMessage: 'Edit icon',
	},
	selectIcon: {
		id: 'instance.settings.tabs.general.edit-icon.select',
		defaultMessage: 'Select icon',
	},
	replaceIcon: {
		id: 'instance.settings.tabs.general.edit-icon.replace',
		defaultMessage: 'Replace icon',
	},
	createIcon: {
		id: 'instance.settings.tabs.general.edit-icon.create',
		defaultMessage: 'Create an icon',
	},
	editCreatedIcon: {
		id: 'instance.settings.tabs.general.edit-icon.edit-created',
		defaultMessage: 'Edit icon',
	},
	removeIcon: {
		id: 'instance.settings.tabs.general.edit-icon.remove',
		defaultMessage: 'Remove icon',
	},
	duplicateInstance: {
		id: 'instance.settings.tabs.general.duplicate-instance',
		defaultMessage: 'Duplicate instance',
	},
	duplicateInstanceDescription: {
		id: 'instance.settings.tabs.general.duplicate-instance.description',
		defaultMessage: 'Creates a copy of this instance, including worlds, configs, mods, etc.',
	},
	duplicateButtonTooltipInstalling: {
		id: 'instance.settings.tabs.general.duplicate-button.tooltip.installing',
		defaultMessage: 'Cannot duplicate while installing.',
	},
	duplicateButton: {
		id: 'instance.settings.tabs.general.duplicate-button',
		defaultMessage: 'Duplicate',
	},
	updateChannel: {
		id: 'instance.settings.tabs.general.update-channel',
		defaultMessage: 'Update channel',
	},
	updateChannelReleaseDescription: {
		id: 'instance.settings.tabs.general.update-channel.release.description',
		defaultMessage: 'Only release versions will be shown as available updates.',
	},
	updateChannelBetaDescription: {
		id: 'instance.settings.tabs.general.update-channel.beta.description',
		defaultMessage: 'Release and beta versions will be shown as available updates.',
	},
	updateChannelAlphaDescription: {
		id: 'instance.settings.tabs.general.update-channel.alpha.description',
		defaultMessage: 'Release, beta, and alpha versions will be shown as available updates.',
	},
	updateChannelRelease: {
		id: 'instance.settings.tabs.general.update-channel.release',
		defaultMessage: 'Release',
	},
	updateChannelBeta: {
		id: 'instance.settings.tabs.general.update-channel.beta',
		defaultMessage: 'Beta',
	},
	updateChannelAlpha: {
		id: 'instance.settings.tabs.general.update-channel.alpha',
		defaultMessage: 'Alpha',
	},
	selectUpdateChannelAriaLabel: {
		id: 'instance.settings.tabs.general.update-channel.select',
		defaultMessage: 'Select update channel',
	},
	deleteInstance: {
		id: 'instance.settings.tabs.general.delete',
		defaultMessage: 'Delete instance',
	},
	deleteInstanceDescription: {
		id: 'instance.settings.tabs.general.delete.description',
		defaultMessage:
			'Permanently deletes an instance from your device, including your worlds, configs, and all installed content. Be careful, as once you delete a instance there is no way to recover it.',
	},
	deleteInstanceButton: {
		id: 'instance.settings.tabs.general.delete.button',
		defaultMessage: 'Delete instance',
	},
	deletingInstanceButton: {
		id: 'instance.settings.tabs.general.deleting.button',
		defaultMessage: 'Deleting...',
	},
	playtimeCorrectionHeading: {
		id: 'instance.settings.tabs.general.playtime-correction.heading',
		defaultMessage: 'Playtime correction',
	},
	playtimeCorrectionSectionDescription: {
		id: 'instance.settings.tabs.general.playtime-correction.section-description',
		defaultMessage:
			"Credit hours from another launcher (like Prism) that Modrinth never tracked. Kept separate from Modrinth's own count, and only added to it for display.",
	},
	modrinthPlaytime: {
		id: 'instance.settings.tabs.general.playtime-correction.modrinth-playtime',
		defaultMessage: 'Modrinth playtime: {hours}',
	},
	correctedPlaytime: {
		id: 'instance.settings.tabs.general.playtime-correction.corrected-playtime',
		defaultMessage: 'Corrected playtime: {hours}',
	},
	playtimeCorrectionButton: {
		id: 'instance.settings.tabs.general.playtime-correction.button',
		defaultMessage: 'Playtime correction',
	},
	sharedFolderExperimentalNotice: {
		id: 'instance.settings.tabs.general.shared-folder.experimental-notice',
		defaultMessage:
			'Shared folders is an experimental setting. Please back up important worlds, configs, resource packs, and other files before using it.',
	},
	sharedFolderHeading: {
		id: 'instance.settings.tabs.general.shared-folder.heading',
		defaultMessage: 'Use shared Minecraft folder',
	},
	sharedFolderDescription: {
		id: 'instance.settings.tabs.general.shared-folder.description',
		defaultMessage:
			'When enabled, this instance shares its worlds, configs, and resource packs with other instances in the selected folder. Changes apply to all of them, while mods and the Minecraft version stay separate. Turning it off removes the shared content from this instance without affecting the shared folder. ',
	},
	sharedFolderNewButton: {
		id: 'instance.settings.tabs.general.shared-folder.new-button',
		defaultMessage: 'New shared folder',
	},
	sharedFolderNamePlaceholder: {
		id: 'instance.settings.tabs.general.shared-folder.name-placeholder',
		defaultMessage: 'Shared folder name',
	},
	sharedFolderCreateButton: {
		id: 'instance.settings.tabs.general.shared-folder.create-button',
		defaultMessage: 'Create',
	},
	sharedFolderRunningNotice: {
		id: 'instance.settings.tabs.general.shared-folder.running-notice',
		defaultMessage: 'This instance is running. stop it to change its shared folder.',
	},
	sharedFolderApplyingTooltip: {
		id: 'instance.settings.tabs.general.shared-folder.applying-tooltip',
		defaultMessage: 'Applying…',
	},
	sharedFolderRunningTooltip: {
		id: 'instance.settings.tabs.general.shared-folder.running-tooltip',
		defaultMessage: 'Stop this instance first.',
	},
	sharedFolderManageButton: {
		id: 'instance.settings.tabs.general.shared-folder.manage-button',
		defaultMessage: 'Manage',
	},
	sharedFolderManageHeading: {
		id: 'instance.settings.tabs.general.shared-folder.manage.heading',
		defaultMessage: 'Manage "{name}"',
	},
	sharedFolderManageDescription: {
		id: 'instance.settings.tabs.general.shared-folder.manage.description',
		defaultMessage:
			'Choose what this shared folder shares between instances. Unchecking an item removes it from each instance instead of keeping a private copy.',
	},
	sharedFolderItemSaves: {
		id: 'instance.settings.tabs.general.shared-folder.item.saves',
		defaultMessage: 'Worlds',
	},
	sharedFolderItemConfig: {
		id: 'instance.settings.tabs.general.shared-folder.item.config',
		defaultMessage: 'Mod configs',
	},
	sharedFolderItemResourcepacks: {
		id: 'instance.settings.tabs.general.shared-folder.item.resourcepacks',
		defaultMessage: 'Resource packs',
	},
	sharedFolderItemOptions: {
		id: 'instance.settings.tabs.general.shared-folder.item.options',
		defaultMessage: 'Options',
	},
	sharedFolderItemOptionsDescription: {
		id: 'instance.settings.tabs.general.shared-folder.item.options.description',
		defaultMessage:
			'Video, sound, language, and keybinds are all stored in Minecraft’s options.txt, so keybinds can’t be shared separately.',
	},
	sharedFolderItemServers: {
		id: 'instance.settings.tabs.general.shared-folder.item.servers',
		defaultMessage: 'Server list',
	},
	sharedFolderManageSaveButton: {
		id: 'instance.settings.tabs.general.shared-folder.manage.save-button',
		defaultMessage: 'Save changes',
	},
	sharedFolderManageSavingButton: {
		id: 'instance.settings.tabs.general.shared-folder.manage.saving-button',
		defaultMessage: 'Saving…',
	},
	sharedFolderDeleteButton: {
		id: 'instance.settings.tabs.general.shared-folder.delete-button',
		defaultMessage: 'Delete shared folder',
	},
	sharedFolderDeleteConfirmButton: {
		id: 'instance.settings.tabs.general.shared-folder.delete-confirm-button',
		defaultMessage: 'Click again to confirm delete',
	},
	sharedFolderDeletingButton: {
		id: 'instance.settings.tabs.general.shared-folder.deleting-button',
		defaultMessage: 'Deleting…',
	},
	sharedFolderDeleteDescription: {
		id: 'instance.settings.tabs.general.shared-folder.delete-description',
		defaultMessage:
			'This permanently deletes the shared data, worlds, configs, resource packs, and everything else being shared. All other instances lose it, with no private copies kept. The instance that created the shared folder gets its own copy back automatically. This can’t be undone for other instances.  ',
	},
	sharedFolderOwnerNotice: {
		id: 'instance.settings.tabs.general.shared-folder.owner-notice',
		defaultMessage:
			'This instance created the shared folder, so it keeps a “master copy.” If it leaves, disables an item, or the shared folder is deleted, it automatically gets its own private copy back.',
	},
})
</script>

<template>
	<ConfirmDeleteInstanceModal
		ref="deleteConfirmModal"
		:instances="[instance]"
		@delete="removeInstance"
	/>
	<IconEditorModal
		ref="iconEditorModal"
		:instance-id="instance.id"
		:config="iconConfig"
		@saved="onGeneratedIconSaved"
	/>
	<PlaytimeCorrectionModal ref="playtimeCorrectionModal" :instance-id="instance.id" />
	<div class="block">
		<div class="float-end ml-10 relative group w-fit">
			<div class="flex flex-col gap-1">
				<span class="text-lg font-semibold text-contrast">
					{{ formatMessage(messages.icon) }}
				</span>
				<div class="group relative w-fit">
					<TeleportOverflowMenu
						:label="formatMessage(messages.editIcon)"
						:tooltip="formatMessage(messages.editIcon)"
						:icon-only="false"
						type="quiet"
						interaction="none"
						class="m-0 !h-auto cursor-pointer appearance-none border-none bg-transparent !p-0 transition-transform group-active:scale-95"
						:options="[
							{
								id: 'select',
								label: icon
									? formatMessage(messages.replaceIcon)
									: formatMessage(messages.selectIcon),
								action: () => setIcon(),
							},
							{
								id: 'create',
								action: () => openIconEditor(),
							},
							{
								id: 'remove',
								label: formatMessage(messages.removeIcon),
								action: () => resetIcon(),
								shown: !!icon,
							},
						]"
					>
						<Avatar
							:src="getInstanceIconUrl(icon)"
							size="108px"
							class="transition-[filter] group-hover:brightness-75"
							:tint-by="instance.id"
							no-shadow
						/>
						<div
							class="absolute top-0 h-full w-full flex items-center justify-center opacity-0 transition-all group-hover:opacity-100"
						>
							<EditIcon aria-hidden="true" class="h-10 w-10 text-white opacity-70" />
						</div>
						<template #select>
							<UploadIcon />
							{{ icon ? formatMessage(messages.replaceIcon) : formatMessage(messages.selectIcon) }}
						</template>
						<template #create>
							<PaletteIcon />
							{{ formatMessage(iconConfig ? messages.editCreatedIcon : messages.createIcon) }}
						</template>
						<template #remove> <TrashIcon /> {{ formatMessage(messages.removeIcon) }} </template>
					</TeleportOverflowMenu>
				</div>
			</div>
		</div>
		<label for="instance-name" class="m-0 text-lg font-semibold text-contrast block">
			{{ formatMessage(messages.name) }}
		</label>
		<div class="flex">
			<Input
				id="instance-name"
				v-model="title"
				autocomplete="off"
				:maxlength="80"
				wrapper-class="flex-grow"
			/>
		</div>
		<template v-if="instance.install_stage == 'installed'">
			<div class="flex flex-col gap-2.5 mt-6">
				<h2 id="duplicate-instance-label" class="m-0 text-lg font-semibold text-contrast block">
					{{ formatMessage(messages.duplicateInstance) }}
				</h2>
				<Button
					v-tooltip="installing ? formatMessage(messages.duplicateButtonTooltipInstalling) : null"
					aria-labelledby="duplicate-instance-label"
					:disabled="installing"
					class="w-max"
					@click="duplicateInstance"
				>
					<CopyIcon /> {{ formatMessage(messages.duplicateButton) }}
				</Button>
				<p class="m-0">
					{{ formatMessage(messages.duplicateInstanceDescription) }}
				</p>
			</div>
		</template>
		<div class="flex flex-col gap-2.5 mt-6">
			<h2 class="m-0 text-lg font-semibold text-contrast block">
				{{ formatMessage(messages.updateChannel) }}
			</h2>
			<Chips
				v-model="selectedReleaseChannel"
				:items="releaseChannelOptions"
				:format-label="formatReleaseChannelLabel"
				:capitalize="false"
				:disabled-items="releaseChannelDisabledItems"
				:aria-label="formatMessage(messages.selectUpdateChannelAriaLabel)"
			/>
			<p class="m-0">
				{{ formatReleaseChannelDescription(selectedReleaseChannel) }}
			</p>
		</div>

		<div class="flex flex-col gap-2.5 mt-6">
			<h2 class="m-0 text-lg font-semibold text-contrast block">
				{{ formatMessage(messages.playtimeCorrectionHeading) }}
			</h2>
			<div class="flex flex-col gap-0.5 text-sm text-secondary">
				<span>
					{{
						formatMessage(messages.modrinthPlaytime, {
							hours: formatTotalPlaytime(modrinthPlaytimeSeconds),
						})
					}}
				</span>
				<span>
					{{
						formatMessage(messages.correctedPlaytime, {
							hours: formatCorrection(correctionSeconds),
						})
					}}
				</span>
			</div>
			<Button type="outlined" class="w-fit" @click="playtimeCorrectionModal?.show()">
				<ClockIcon /> {{ formatMessage(messages.playtimeCorrectionButton) }}
			</Button>
			<p class="m-0">
				{{ formatMessage(messages.playtimeCorrectionSectionDescription) }}
			</p>
		</div>

		<div class="flex flex-col gap-2.5 mt-6">
			<div class="flex items-center justify-between gap-4">
				<h2 id="shared-folder-label" class="m-0 text-lg font-semibold text-contrast block">
					{{ formatMessage(messages.sharedFolderHeading) }}
				</h2>
				<Toggle
					id="use-shared-folder"
					v-tooltip="instanceRunning ? formatMessage(messages.sharedFolderRunningTooltip) : null"
					aria-labelledby="shared-folder-label"
					:model-value="toggleOn"
					:disabled="applyingSharedFolder || instanceRunning"
					@update:model-value="toggleSharedFolder"
				/>
			</div>
			<p
				class="m-0 flex items-center gap-1.5 rounded-xl border border-solid border-orange bg-highlight-orange px-3 py-2 text-sm text-orange"
			>
				<TriangleAlertIcon class="size-4 shrink-0" aria-hidden="true" />
				{{ formatMessage(messages.sharedFolderExperimentalNotice) }}
			</p>
			<p class="m-0">
				{{ formatMessage(messages.sharedFolderDescription) }}
			</p>
			<p v-if="instanceRunning" class="m-0 text-sm text-orange">
				{{ formatMessage(messages.sharedFolderRunningNotice) }}
			</p>

			<div v-if="toggleOn" class="flex flex-col gap-2 mt-1">
				<div
					v-if="sharedProfiles.length > 0 && !creatingSharedFolder"
					class="flex flex-wrap items-center gap-2"
				>
					<Chips
						v-model="selectedSharedProfileId"
						:items="sharedProfiles.map((profile) => profile.id)"
						:format-label="(id) => sharedProfiles.find((profile) => profile.id === id)?.name ?? id"
						:capitalize="false"
						:never-empty="false"
						:disabled-items="
							applyingSharedFolder || instanceRunning
								? sharedProfiles.map((profile) => profile.id)
								: []
						"
						:disabled-tooltip="
							instanceRunning
								? formatMessage(messages.sharedFolderRunningTooltip)
								: formatMessage(messages.sharedFolderApplyingTooltip)
						"
					/>
					<SpinnerIcon
						v-if="applyingSharedFolder"
						class="animate-spin text-secondary"
						aria-hidden="true"
					/>
					<Button
						type="outlined"
						:disabled="applyingSharedFolder || instanceRunning"
						@click="creatingSharedFolder = true"
					>
						<PlusIcon /> {{ formatMessage(messages.sharedFolderNewButton) }}
					</Button>
					<Button
						v-if="currentSharedProfile"
						type="outlined"
						:disabled="applyingSharedFolder"
						@click="openManageSharedFolder"
					>
						<SettingsIcon /> {{ formatMessage(messages.sharedFolderManageButton) }}
					</Button>
				</div>
				<div v-if="creatingSharedFolder" class="flex items-center gap-2">
					<Input
						v-model="newSharedFolderName"
						autocomplete="off"
						:maxlength="256"
						:placeholder="formatMessage(messages.sharedFolderNamePlaceholder)"
						wrapper-class="flex-grow"
						:disabled="applyingSharedFolder"
						@keyup.enter="confirmCreateSharedFolder"
					/>
					<Button
						type="colored"
						color="brand"
						:disabled="!newSharedFolderName.trim() || applyingSharedFolder"
						@click="confirmCreateSharedFolder"
					>
						<FolderOpenIcon /> {{ formatMessage(messages.sharedFolderCreateButton) }}
					</Button>
					<Button
						v-if="sharedProfiles.length > 0"
						type="outlined"
						:disabled="applyingSharedFolder"
						@click="cancelCreateSharedFolder"
					>
						{{ formatMessage(commonMessages.cancelButton) }}
					</Button>
				</div>
			</div>
		</div>

		<NewModal
			ref="manageModal"
			:header="
				formatMessage(messages.sharedFolderManageHeading, {
					name: currentSharedProfile?.name ?? '',
				})
			"
			:on-hide="() => (confirmingDeleteSharedFolder = false)"
		>
			<div v-if="currentSharedProfile" class="flex flex-col gap-4">
				<p class="m-0 text-sm text-secondary">
					{{ formatMessage(messages.sharedFolderManageDescription) }}
				</p>
				<p v-if="isOwnerOfCurrentSharedFolder" class="m-0 text-sm text-brand">
					{{ formatMessage(messages.sharedFolderOwnerNotice) }}
				</p>
				<div class="flex flex-col gap-3">
					<Checkbox
						v-model="managedItems.share_saves"
						:label="formatMessage(messages.sharedFolderItemSaves)"
						:disabled="savingManagedItems || deletingSharedFolder"
					/>
					<Checkbox
						v-model="managedItems.share_config"
						:label="formatMessage(messages.sharedFolderItemConfig)"
						:disabled="savingManagedItems || deletingSharedFolder"
					/>
					<Checkbox
						v-model="managedItems.share_resourcepacks"
						:label="formatMessage(messages.sharedFolderItemResourcepacks)"
						:disabled="savingManagedItems || deletingSharedFolder"
					/>
					<Checkbox
						v-model="managedItems.share_options"
						:label="formatMessage(messages.sharedFolderItemOptions)"
						:disabled="savingManagedItems || deletingSharedFolder"
					/>
					<p class="m-0 ml-8 -mt-2 text-xs text-secondary">
						{{ formatMessage(messages.sharedFolderItemOptionsDescription) }}
					</p>
					<Checkbox
						v-model="managedItems.share_servers"
						:label="formatMessage(messages.sharedFolderItemServers)"
						:disabled="savingManagedItems || deletingSharedFolder"
					/>
				</div>
				<Button
					type="colored"
					color="brand"
					class="w-fit"
					:disabled="savingManagedItems || deletingSharedFolder"
					@click="saveManagedItems"
				>
					<SpinnerIcon v-if="savingManagedItems" class="animate-spin" />
					{{
						savingManagedItems
							? formatMessage(messages.sharedFolderManageSavingButton)
							: formatMessage(messages.sharedFolderManageSaveButton)
					}}
				</Button>

				<hr class="w-full border-surface-5" />

				<div class="flex flex-col gap-2">
					<h3 class="m-0 text-base font-semibold text-contrast">
						{{ formatMessage(messages.sharedFolderDeleteButton) }}
					</h3>
					<p class="m-0 text-sm text-secondary">
						{{ formatMessage(messages.sharedFolderDeleteDescription) }}
					</p>
					<Button
						type="colored"
						color="red"
						class="w-fit"
						:disabled="savingManagedItems || deletingSharedFolder"
						@click="deleteCurrentSharedFolder"
					>
						<SpinnerIcon v-if="deletingSharedFolder" class="animate-spin" />
						<TrashIcon v-else />
						{{
							deletingSharedFolder
								? formatMessage(messages.sharedFolderDeletingButton)
								: confirmingDeleteSharedFolder
									? formatMessage(messages.sharedFolderDeleteConfirmButton)
									: formatMessage(messages.sharedFolderDeleteButton)
						}}
					</Button>
				</div>
			</div>
		</NewModal>

		<div class="flex flex-col gap-2.5 mt-6">
			<h2 id="delete-instance-label" class="m-0 text-lg font-semibold text-contrast block">
				{{ formatMessage(messages.deleteInstance) }}
			</h2>
			<Button
				type="colored"
				color="red"
				aria-labelledby="delete-instance-label"
				:disabled="removing"
				class="w-fit"
				@click="deleteConfirmModal.show()"
			>
				<SpinnerIcon v-if="removing" class="animate-spin" />
				<TrashIcon v-else />
				{{
					removing
						? formatMessage(messages.deletingInstanceButton)
						: formatMessage(messages.deleteInstanceButton)
				}}
			</Button>
			<p class="m-0">
				{{ formatMessage(messages.deleteInstanceDescription) }}
			</p>
		</div>
	</div>
</template>
<style scoped lang="scss">
.hovering-icon-shadow {
	box-shadow: var(--shadow-inset-sm), var(--shadow-raised);
}
</style>
