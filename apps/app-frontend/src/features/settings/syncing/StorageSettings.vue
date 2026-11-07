<!--
	Modrinth Studios addition: overview of how much disk space Studio's data
	uses, styled after Steam's own per-drive storage breakdown — a segmented
	bar across the whole drive at the top (Modrinth instances / worlds /
	resource packs / shaders / mods / replays / everything else on the drive /
	free space), then every instance listed below with its own composition
	bar (which of *that* instance's own categories its space is going to)
	instead of a bar that's just relative to the biggest instance. Clicking a
	row now opens that instance instead of its folder — folder access,
	instance settings, and delete all moved into a "..." menu, matching how
	the Library's own instance cards work. See use-studio-instance-storage.ts
	for the fetch/format helpers and packages/app-lib/src/api/instance/storage.rs
	for the actual disk walk.
-->
<script setup>
import {
	DatabaseIcon,
	FolderOpenIcon,
	MoreVerticalIcon,
	SettingsIcon,
	SpinnerIcon,
	TrashIcon,
} from '@modrinth/assets'
import {
	Avatar,
	commonMessages,
	defineMessages,
	injectNotificationManager,
	SmartClickable,
	TeleportOverflowMenu,
	Toggle,
	useVIntl,
} from '@modrinth/ui'
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import ConfirmDeleteInstanceModal from '@/components/ui/modal/ConfirmDeleteInstanceModal.vue'
import { useStudioAppearance } from '@/composables/use-studio-appearance.ts'
import {
	fetchInstanceStorageUsage,
	fetchSystemStorageOverview,
	formatStorageSize,
} from '@/composables/use-studio-instance-storage.ts'
import { getInstanceIconUrl, list as listInstances, remove as removeInstance } from '@/helpers/instance'
import { showInstanceInFolder } from '@/helpers/utils.js'

const { handleError } = injectNotificationManager()
const { formatMessage } = useVIntl()
const appearance = useStudioAppearance()
const router = useRouter()

const loading = ref(true)
const usage = ref([])
const instancesById = ref(new Map())
const overview = ref(null)
const overviewLoading = ref(true)

const messages = defineMessages({
	title: {
		id: 'app.settings.storage.title',
		defaultMessage: 'Instance storage',
	},
	description: {
		id: 'app.settings.storage.description',
		defaultMessage: "Sorted largest first. Each bar shows what that instance's own space is made up of.",
	},
	total: {
		id: 'app.settings.storage.total',
		defaultMessage: '{size} total across {count} instances',
	},
	empty: {
		id: 'app.settings.storage.empty',
		defaultMessage: 'No instances found.',
	},
	showInFolderList: {
		id: 'app.settings.storage.show-toggle.title',
		defaultMessage: 'Show storage size next to playtime',
	},
	showInFolderListDescription: {
		id: 'app.settings.storage.show-toggle.description',
		defaultMessage:
			"Show each instance's storage size alongside its total playtime on its page.",
	},
	moreOptions: {
		id: 'app.settings.storage.more-options',
		defaultMessage: 'More options',
	},
	overviewTitle: {
		id: 'app.settings.storage.overview.title',
		defaultMessage: 'Overview',
	},
	overviewDescription: {
		id: 'app.settings.storage.overview.description',
		defaultMessage: 'How the drive Studio\'s data lives on is being used, system-wide.',
	},
	categoryInstances: {
		id: 'app.settings.storage.overview.category.instances',
		defaultMessage: 'Modrinth instances',
	},
	categoryWorlds: {
		id: 'app.settings.storage.overview.category.worlds',
		defaultMessage: 'Worlds',
	},
	categoryContent: {
		id: 'app.settings.storage.overview.category.content',
		defaultMessage: 'Mods & content',
	},
	categoryReplays: {
		id: 'app.settings.storage.overview.category.replays',
		defaultMessage: 'Replays',
	},
	categoryNonModrinth: {
		id: 'app.settings.storage.overview.category.non-modrinth',
		defaultMessage: 'Non-Modrinth',
	},
	categoryFree: {
		id: 'app.settings.storage.overview.category.free',
		defaultMessage: 'Free',
	},
	diskUnknown: {
		id: 'app.settings.storage.overview.disk-unknown',
		defaultMessage: "Couldn't read this drive's total capacity.",
	},
})

// Modrinth Studios addition: one shared flat color per category, reused
// between the system-wide overview bar and every instance's own composition
// bar below it, so e.g. "purple" always means "worlds" no matter which bar
// you're looking at. All straight from this fork's existing theme tokens
// (--color-green-500/blue/purple/orange/red/gray — see packages/assets's
// variables.scss) — no blended/mixed hues, on the person's feedback that the
// original palette (which used two color-mix blends for mods/replays) read
// as muddy.
//
// "Modrinth instances" deliberately uses the raw `--color-green-500` scale
// token instead of the semantic `--color-brand` (or `--color-green`) it
// normally aliases to — this fork's accent-color picker
// (use-studio-appearance.ts's applyAccentVars) overwrites `--color-brand`
// and `--color-green` with whatever the person picked as their UI accent,
// so a brown/orange accent choice was turning this category brown too. The
// numbered scale token isn't touched by that override, so this bar always
// shows Modrinth's actual green here regardless of accent color.
const CATEGORY_COLOR = {
	other: 'var(--color-green-500)',
	worlds: 'var(--color-purple)',
	content: 'var(--color-orange)',
	replays: 'var(--color-red)',
	nonModrinth: 'var(--color-gray)',
	free: 'var(--surface-5)',
}

const overviewSegments = computed(() => {
	if (!overview.value) return []
	const o = overview.value
	return [
		{ key: 'other', label: formatMessage(messages.categoryInstances), bytes: o.instances_other_bytes, color: CATEGORY_COLOR.other },
		{ key: 'worlds', label: formatMessage(messages.categoryWorlds), bytes: o.worlds_bytes, color: CATEGORY_COLOR.worlds },
		{
			key: 'content',
			label: formatMessage(messages.categoryContent),
			// Modrinth Studios addition: mods, resource packs, and shaders used
			// to each get their own segment — on feedback that nine categories
			// was too many to read at a glance, they're folded into one
			// "Mods & content" bucket, the same umbrella term the instance
			// page's own Content tab already uses for these three project
			// types (see state::instance_types::ProjectType).
			bytes: o.resourcepacks_bytes + o.shaderpacks_bytes + o.mods_bytes,
			color: CATEGORY_COLOR.content,
		},
		{ key: 'replays', label: formatMessage(messages.categoryReplays), bytes: o.replays_bytes, color: CATEGORY_COLOR.replays },
		{ key: 'nonModrinth', label: formatMessage(messages.categoryNonModrinth), bytes: o.non_modrinth_bytes, color: CATEGORY_COLOR.nonModrinth },
		{ key: 'free', label: formatMessage(messages.categoryFree), bytes: o.free_disk_bytes, color: CATEGORY_COLOR.free },
	]
})

function instanceSegments(entry) {
	const b = entry.breakdown
	return [
		{ key: 'worlds', label: formatMessage(messages.categoryWorlds), bytes: b.worlds_bytes, color: CATEGORY_COLOR.worlds },
		{
			key: 'content',
			label: formatMessage(messages.categoryContent),
			bytes: b.resourcepacks_bytes + b.shaderpacks_bytes + b.mods_bytes,
			color: CATEGORY_COLOR.content,
		},
		{ key: 'replays', label: formatMessage(messages.categoryReplays), bytes: b.replays_bytes, color: CATEGORY_COLOR.replays },
		{ key: 'other', label: formatMessage(messages.categoryInstances), bytes: b.other_bytes, color: CATEGORY_COLOR.other },
	]
}

function segmentTooltip(segment) {
	return `${segment.label}: ${formatStorageSize(segment.bytes)}`
}

const totalBytes = ref(0)

async function load() {
	loading.value = true
	try {
		const [usageResult, instances] = await Promise.all([
			fetchInstanceStorageUsage(),
			listInstances().catch(() => []),
		])
		usage.value = usageResult
		instancesById.value = new Map(instances.map((instance) => [instance.id, instance]))
		totalBytes.value = usage.value.reduce((sum, entry) => sum + entry.size_bytes, 0)
	} catch (err) {
		handleError(err)
	} finally {
		loading.value = false
	}
}

async function loadOverview() {
	overviewLoading.value = true
	try {
		overview.value = await fetchSystemStorageOverview()
	} catch (err) {
		handleError(err)
	} finally {
		overviewLoading.value = false
	}
}

// Modrinth Studios addition: a stub GameInstance-shaped object for
// ConfirmDeleteInstanceModal when, for whatever reason, this row's instance
// isn't in the freshly-fetched instance list (a race with it being deleted
// elsewhere) — its Avatar/formatLoader calls all tolerate missing
// icon/loader fields already, they just show the defaults.
function instanceFor(instanceId, name) {
	return instancesById.value.get(instanceId) ?? { id: instanceId, name }
}

function openInstanceSettings(instanceId) {
	router.push({ path: `/instance/${encodeURIComponent(instanceId)}`, query: { studioOpenSettings: '1' } })
}

const deleteConfirmModal = ref()
const instanceToDelete = ref(null)

function confirmDeleteInstance(entry) {
	instanceToDelete.value = instanceFor(entry.instance_id, entry.name)
	deleteConfirmModal.value?.show()
}

async function performDeleteInstance() {
	if (!instanceToDelete.value) return
	const id = instanceToDelete.value.id
	try {
		await removeInstance(id)
		usage.value = usage.value.filter((entry) => entry.instance_id !== id)
		instancesById.value.delete(id)
		totalBytes.value = usage.value.reduce((sum, entry) => sum + entry.size_bytes, 0)
		// Best-effort: the overview bar's totals are now stale, but a failure
		// refreshing it shouldn't be treated as the delete itself failing.
		loadOverview()
	} catch (err) {
		handleError(err)
	} finally {
		instanceToDelete.value = null
	}
}

onMounted(load)
onMounted(loadOverview)
</script>

<template>
	<div class="flex flex-col gap-6">
		<ConfirmDeleteInstanceModal
			ref="deleteConfirmModal"
			:instances="instanceToDelete ? [instanceToDelete] : []"
			@delete="performDeleteInstance"
		/>

		<div class="flex flex-col gap-2.5">
			<h2 class="m-0 text-lg font-semibold text-contrast">
				{{ formatMessage(messages.overviewTitle) }}
			</h2>
			<p class="m-0 leading-tight text-secondary">
				{{ formatMessage(messages.overviewDescription) }}
			</p>

			<div v-if="overviewLoading" class="flex items-center gap-2 text-secondary py-4">
				<SpinnerIcon class="animate-spin" />
				<span>{{ formatMessage(commonMessages.loadingLabel) }}</span>
			</div>
			<template v-else-if="overview">
				<p v-if="!overview.total_disk_bytes" class="m-0 text-sm text-secondary">
					{{ formatMessage(messages.diskUnknown) }}
				</p>
				<div class="flex h-4 w-full items-stretch gap-px overflow-hidden rounded-full bg-surface-3">
					<div
						v-for="segment in overviewSegments"
						v-show="segment.bytes > 0"
						:key="segment.key"
						v-tooltip="segmentTooltip(segment)"
						class="h-full"
						:style="{ flexGrow: segment.bytes, flexBasis: '0%', minWidth: '2px', background: segment.color }"
					/>
				</div>
				<div class="mt-1 flex flex-wrap gap-x-4 gap-y-1.5">
					<div
						v-for="segment in overviewSegments"
						:key="segment.key"
						class="flex items-center gap-1.5 text-sm"
					>
						<span class="size-2.5 shrink-0 rounded-full" :style="{ background: segment.color }" />
						<span class="text-secondary">{{ segment.label }}</span>
						<span class="font-semibold text-contrast">{{ formatStorageSize(segment.bytes) }}</span>
					</div>
				</div>
			</template>
		</div>

		<div class="flex items-center justify-between gap-4">
			<div>
				<h2 class="m-0 text-lg font-semibold text-contrast">
					{{ formatMessage(messages.showInFolderList) }}
				</h2>
				<p class="m-0 mt-1">
					{{ formatMessage(messages.showInFolderListDescription) }}
				</p>
			</div>
			<Toggle
				id="show-instance-storage-usage"
				v-model="appearance.showInstanceStorageUsage"
			/>
		</div>

		<div class="flex flex-col gap-2.5">
			<h2 class="m-0 text-lg font-semibold text-contrast">
				{{ formatMessage(messages.title) }}
			</h2>
			<p class="m-0 leading-tight text-secondary">
				{{ formatMessage(messages.description) }}
			</p>

			<div v-if="loading" class="flex items-center gap-2 text-secondary py-4">
				<SpinnerIcon class="animate-spin" />
				<span>{{ formatMessage(commonMessages.loadingLabel) }}</span>
			</div>

			<template v-else>
				<p v-if="usage.length === 0" class="m-0 text-secondary">
					{{ formatMessage(messages.empty) }}
				</p>
				<template v-else>
					<p class="m-0 mb-1 text-sm font-semibold text-secondary">
						{{
							formatMessage(messages.total, {
								size: formatStorageSize(totalBytes),
								count: usage.length,
							})
						}}
					</p>
					<div class="flex flex-col gap-1.5">
						<SmartClickable v-for="entry in usage" :key="entry.instance_id" class="[--active-scale:0.99]">
							<template #clickable>
								<router-link
									class="no-click-animation"
									:to="`/instance/${encodeURIComponent(entry.instance_id)}`"
								/>
							</template>
							<div
								class="clickable-card flex items-center gap-3 rounded-lg bg-bg-raised p-2.5 smart-clickable:highlight-on-hover transition-[filter] ease-out [--hover-brightness:0.9]"
							>
								<Avatar
									:src="getInstanceIconUrl(instanceFor(entry.instance_id, entry.name).icon_path)"
									:tint-by="entry.instance_id"
									size="40px"
									class="!rounded-xl shrink-0"
									no-shadow
								/>
								<div class="flex min-w-0 flex-1 flex-col gap-1.5">
									<div class="flex items-center justify-between gap-3">
										<span class="truncate font-semibold text-contrast">{{ entry.name }}</span>
										<span class="flex shrink-0 items-center gap-1 text-sm text-secondary">
											<DatabaseIcon class="shrink-0" aria-hidden="true" />
											{{ formatStorageSize(entry.size_bytes) }}
										</span>
									</div>
									<div
										data-no-card-click
										class="flex h-2 gap-px overflow-hidden rounded-full bg-surface-3 smart-clickable:allow-pointer-events"
									>
										<div
											v-for="segment in instanceSegments(entry)"
											v-show="segment.bytes > 0"
											:key="segment.key"
											v-tooltip="segmentTooltip(segment)"
											class="h-full"
											:style="{ flexGrow: segment.bytes, flexBasis: '0%', minWidth: '2px', background: segment.color }"
										/>
									</div>
								</div>
								<div data-no-card-click class="shrink-0 smart-clickable:allow-pointer-events">
									<TeleportOverflowMenu
										type="quiet"
										:label="formatMessage(messages.moreOptions)"
										:options="[
											{
												id: 'open-folder',
												label: formatMessage(commonMessages.openFolderButton),
												action: () => showInstanceInFolder(entry.instance_id),
											},
											{
												id: 'settings',
												label: formatMessage(commonMessages.settingsLabel),
												action: () => openInstanceSettings(entry.instance_id),
											},
											{
												id: 'delete',
												label: formatMessage(commonMessages.deleteLabel),
												tone: 'red',
												action: () => confirmDeleteInstance(entry),
											},
										]"
									>
										<MoreVerticalIcon aria-hidden="true" />
										<template #open-folder>
											<FolderOpenIcon aria-hidden="true" />
											{{ formatMessage(commonMessages.openFolderButton) }}
										</template>
										<template #settings>
											<SettingsIcon aria-hidden="true" />
											{{ formatMessage(commonMessages.settingsLabel) }}
										</template>
										<template #delete>
											<TrashIcon aria-hidden="true" />
											{{ formatMessage(commonMessages.deleteLabel) }}
										</template>
									</TeleportOverflowMenu>
								</div>
							</div>
						</SmartClickable>
					</div>
				</template>
			</template>
		</div>
	</div>
</template>
<style scoped>
.clickable-card:has([data-no-card-click]:hover) {
	--hover-brightness: 1;
}
</style>
