<!--
	Modrinth Studios addition: new file, not touching any upstream modal.
	Lets the person customize what Studio's Discord "Playing" activity shows,
	instead of the fixed "Playing <instance name>" text upstream always sends.
	See packages/app-lib/src/api/discord_rpc.rs for the full rationale
	(especially why there's no custom image option).
-->
<script setup lang="ts">
import { SaveIcon, XIcon } from '@modrinth/assets'
import {
	Button,
	Chips,
	commonMessages,
	defineMessage,
	defineMessages,
	injectNotificationManager,
	Input,
	NewModal,
	Toggle,
	useVIntl,
} from '@modrinth/ui'
import { ref } from 'vue'

import {
	defaultDiscordRpcSettings,
	type DiscordActivityKind,
	type DiscordActivityMode,
	type DiscordRpcSettings,
	getDiscordRpcSettings,
	setDiscordRpcSettings,
} from '@/helpers/discord-rpc'

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()

const modal = ref<InstanceType<typeof NewModal>>()
const loading = ref(false)
const saving = ref(false)
const form = ref<DiscordRpcSettings>(defaultDiscordRpcSettings())

const modeOptions: DiscordActivityMode[] = ['default', 'detailed', 'minimal', 'custom']
const activityTypeOptions: DiscordActivityKind[] = ['playing', 'listening', 'watching', 'competing']

async function show() {
	modal.value?.show()
	loading.value = true
	try {
		form.value = await getDiscordRpcSettings()
	} catch (err) {
		handleError(err as Error)
	} finally {
		loading.value = false
	}
}

function hide() {
	modal.value?.hide()
}

async function save() {
	if (saving.value) return
	saving.value = true
	try {
		// Blank labels/URLs read as "no button" on the backend, but keep the
		// value out entirely rather than sending a half-filled pair.
		const cleaned: DiscordRpcSettings = {
			...form.value,
			button_1_label: form.value.button_1_label?.trim() || null,
			button_1_url: form.value.button_1_url?.trim() || null,
			button_2_label: form.value.button_2_label?.trim() || null,
			button_2_url: form.value.button_2_url?.trim() || null,
		}
		await setDiscordRpcSettings(cleaned)
		hide()
	} catch (err) {
		handleError(err as Error)
	} finally {
		saving.value = false
	}
}

defineExpose({ show })

const titleMessage = defineMessage({
	id: 'app.settings.privacy.discord-rich-presence.customize.title',
	defaultMessage: 'Customize Discord Rich Presence',
})

const messages = defineMessages({
	modeLabel: {
		id: 'app.settings.privacy.discord-rich-presence.customize.mode-label',
		defaultMessage: 'What to show',
	},
	modeDefault: {
		id: 'app.settings.privacy.discord-rich-presence.customize.mode-default',
		defaultMessage: 'Default',
	},
	modeDetailed: {
		id: 'app.settings.privacy.discord-rich-presence.customize.mode-detailed',
		defaultMessage: 'Detailed',
	},
	modeMinimal: {
		id: 'app.settings.privacy.discord-rich-presence.customize.mode-minimal',
		defaultMessage: 'Minimal',
	},
	modeCustom: {
		id: 'app.settings.privacy.discord-rich-presence.customize.mode-custom',
		defaultMessage: 'Custom',
	},
	modeDescriptionDefault: {
		id: 'app.settings.privacy.discord-rich-presence.customize.mode-description-default',
		defaultMessage: '"Playing <instance name>", same as before.',
	},
	modeDescriptionDetailed: {
		id: 'app.settings.privacy.discord-rich-presence.customize.mode-description-detailed',
		defaultMessage: 'Also shows the loader and Minecraft version, e.g. "fabric 1.21.11".',
	},
	modeDescriptionMinimal: {
		id: 'app.settings.privacy.discord-rich-presence.customize.mode-description-minimal',
		defaultMessage: 'Just "Playing Minecraft", hides which modpack you\'re running.',
	},
	modeDescriptionCustom: {
		id: 'app.settings.privacy.discord-rich-presence.customize.mode-description-custom',
		defaultMessage: 'Write your own text below.',
	},
	activityTypeLabel: {
		id: 'app.settings.privacy.discord-rich-presence.customize.activity-type-label',
		defaultMessage: 'Activity verb',
	},
	activityPlaying: {
		id: 'app.settings.privacy.discord-rich-presence.customize.activity-playing',
		defaultMessage: 'Playing',
	},
	activityListening: {
		id: 'app.settings.privacy.discord-rich-presence.customize.activity-listening',
		defaultMessage: 'Listening to',
	},
	activityWatching: {
		id: 'app.settings.privacy.discord-rich-presence.customize.activity-watching',
		defaultMessage: 'Watching',
	},
	activityCompeting: {
		id: 'app.settings.privacy.discord-rich-presence.customize.activity-competing',
		defaultMessage: 'Competing in',
	},
	elapsedTimeLabel: {
		id: 'app.settings.privacy.discord-rich-presence.customize.elapsed-time-label',
		defaultMessage: 'Show elapsed time',
	},
	idleTextLabel: {
		id: 'app.settings.privacy.discord-rich-presence.customize.idle-text-label',
		defaultMessage: 'Idle text',
	},
	idleTextDescription: {
		id: 'app.settings.privacy.discord-rich-presence.customize.idle-text-description',
		defaultMessage: 'Shown on Discord whenever nothing is running.',
	},
	customStateLabel: {
		id: 'app.settings.privacy.discord-rich-presence.customize.custom-state-label',
		defaultMessage: 'First line',
	},
	customDetailsLabel: {
		id: 'app.settings.privacy.discord-rich-presence.customize.custom-details-label',
		defaultMessage: 'Second line (optional)',
	},
	placeholderHint: {
		id: 'app.settings.privacy.discord-rich-presence.customize.placeholder-hint',
		defaultMessage:
			'Use {instancePlaceholder}, {loaderPlaceholder}, and {versionPlaceholder} — they\'ll be swapped in automatically.',
	},
	buttonsLabel: {
		id: 'app.settings.privacy.discord-rich-presence.customize.buttons-label',
		defaultMessage: 'Buttons (optional, up to 2)',
	},
	buttonsDescription: {
		id: 'app.settings.privacy.discord-rich-presence.customize.buttons-description',
		defaultMessage: 'Shown on your Discord profile card. Both a label and a URL are needed for a button to appear.',
	},
	buttonLabelPlaceholder: {
		id: 'app.settings.privacy.discord-rich-presence.customize.button-label-placeholder',
		defaultMessage: 'Label',
	},
	buttonUrlPlaceholder: {
		id: 'app.settings.privacy.discord-rich-presence.customize.button-url-placeholder',
		defaultMessage: 'https://...',
	},
})

function modeLabel(mode: DiscordActivityMode) {
	switch (mode) {
		case 'detailed':
			return formatMessage(messages.modeDetailed)
		case 'minimal':
			return formatMessage(messages.modeMinimal)
		case 'custom':
			return formatMessage(messages.modeCustom)
		default:
			return formatMessage(messages.modeDefault)
	}
}

function modeDescription(mode: DiscordActivityMode) {
	switch (mode) {
		case 'detailed':
			return formatMessage(messages.modeDescriptionDetailed)
		case 'minimal':
			return formatMessage(messages.modeDescriptionMinimal)
		case 'custom':
			return formatMessage(messages.modeDescriptionCustom)
		default:
			return formatMessage(messages.modeDescriptionDefault)
	}
}

function activityTypeLabel(kind: DiscordActivityKind) {
	switch (kind) {
		case 'listening':
			return formatMessage(messages.activityListening)
		case 'watching':
			return formatMessage(messages.activityWatching)
		case 'competing':
			return formatMessage(messages.activityCompeting)
		default:
			return formatMessage(messages.activityPlaying)
	}
}
</script>
<template>
	<NewModal ref="modal" :header="formatMessage(titleMessage)" width="520px" max-width="520px">
		<div class="flex flex-col gap-4">
			<div class="flex flex-col gap-1.5">
				<span class="text-sm font-semibold text-contrast">{{ formatMessage(messages.modeLabel) }}</span>
				<Chips
					v-model="form.mode"
					:items="modeOptions"
					:format-label="modeLabel"
					:capitalize="false"
				/>
				<p class="m-0 text-sm text-secondary">{{ modeDescription(form.mode) }}</p>
			</div>

			<div v-if="form.mode === 'custom'" class="flex flex-col gap-3 rounded-xl border border-solid border-surface-5 p-3">
				<p class="m-0 text-sm text-secondary">
					{{
						formatMessage(messages.placeholderHint, {
							instancePlaceholder: '{instance}',
							loaderPlaceholder: '{loader}',
							versionPlaceholder: '{version}',
						})
					}}
				</p>
				<label class="flex flex-col gap-1">
					<span class="text-sm font-semibold text-contrast">{{ formatMessage(messages.customStateLabel) }}</span>
					<Input
						v-model="form.custom_state_template"
						:placeholder="`Playing {instance}`"
						autocomplete="off"
						:disabled="loading"
					/>
				</label>
				<label class="flex flex-col gap-1">
					<span class="text-sm font-semibold text-contrast">{{ formatMessage(messages.customDetailsLabel) }}</span>
					<Input
						v-model="form.custom_details_template"
						:placeholder="`{loader} {version}`"
						autocomplete="off"
						:disabled="loading"
					/>
				</label>
			</div>

			<div class="flex flex-col gap-1.5">
				<span class="text-sm font-semibold text-contrast">{{ formatMessage(messages.activityTypeLabel) }}</span>
				<Chips
					v-model="form.activity_type"
					:items="activityTypeOptions"
					:format-label="activityTypeLabel"
					:capitalize="false"
				/>
			</div>

			<div class="flex items-center justify-between gap-4">
				<span class="text-sm font-semibold text-contrast">{{ formatMessage(messages.elapsedTimeLabel) }}</span>
				<Toggle id="discord-rpc-elapsed-time" v-model="form.show_elapsed_time" :disabled="loading" />
			</div>

			<label class="flex flex-col gap-1">
				<span class="text-sm font-semibold text-contrast">{{ formatMessage(messages.idleTextLabel) }}</span>
				<p class="m-0 text-sm text-secondary">{{ formatMessage(messages.idleTextDescription) }}</p>
				<Input v-model="form.idle_text" autocomplete="off" :disabled="loading" />
			</label>

			<div class="flex flex-col gap-2">
				<span class="text-sm font-semibold text-contrast">{{ formatMessage(messages.buttonsLabel) }}</span>
				<p class="m-0 text-sm text-secondary">{{ formatMessage(messages.buttonsDescription) }}</p>
				<div class="flex gap-2">
					<Input
						v-model="form.button_1_label"
						:placeholder="formatMessage(messages.buttonLabelPlaceholder)"
						autocomplete="off"
						:disabled="loading"
						class="flex-1"
					/>
					<Input
						v-model="form.button_1_url"
						:placeholder="formatMessage(messages.buttonUrlPlaceholder)"
						autocomplete="off"
						:disabled="loading"
						class="flex-[2]"
					/>
				</div>
				<div class="flex gap-2">
					<Input
						v-model="form.button_2_label"
						:placeholder="formatMessage(messages.buttonLabelPlaceholder)"
						autocomplete="off"
						:disabled="loading"
						class="flex-1"
					/>
					<Input
						v-model="form.button_2_url"
						:placeholder="formatMessage(messages.buttonUrlPlaceholder)"
						autocomplete="off"
						:disabled="loading"
						class="flex-[2]"
					/>
				</div>
			</div>
		</div>
		<template #actions>
			<div class="flex gap-2 justify-end">
				<Button type="outlined" @click="hide()">
					<XIcon />
					{{ formatMessage(commonMessages.cancelButton) }}
				</Button>
				<Button type="colored" color="brand" :disabled="loading || saving" @click="save">
					<SaveIcon />
					{{ formatMessage(commonMessages.saveChangesButton) }}
				</Button>
			</div>
		</template>
	</NewModal>
</template>
