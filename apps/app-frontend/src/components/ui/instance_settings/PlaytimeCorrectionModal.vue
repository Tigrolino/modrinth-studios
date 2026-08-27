<!--
	Modrinth Studios addition: new file, not touching any upstream modal.
	Lets someone set a manual playtime correction for an instance — meant for
	crediting hours tracked by another launcher (Prism, MultiMC, etc.) before
	switching to Modrinth, since Modrinth itself has no way to know about
	those. Kept fully separate from the real Modrinth-tracked total; see
	packages/app-lib/src/api/playtime_correction.rs.
-->
<script setup lang="ts">
import { SaveIcon, XIcon } from '@modrinth/assets'
import {
	Button,
	commonMessages,
	defineMessage,
	defineMessages,
	injectNotificationManager,
	Input,
	NewModal,
	useVIntl,
} from '@modrinth/ui'
import { useQueryClient } from '@tanstack/vue-query'
import { ref } from 'vue'

import { setPlaytimeCorrection } from '@/helpers/playtime-correction'
import { instanceKeys, instancePlaytimeCorrectionQueryOptions } from '@/pages/instance/query-options'

const props = defineProps<{
	instanceId: string
}>()

const emit = defineEmits<{
	saved: [correctionSeconds: number]
}>()

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()
const queryClient = useQueryClient()

const modal = ref<InstanceType<typeof NewModal>>()
const hours = ref(0)
const loading = ref(false)
const saving = ref(false)

async function show() {
	modal.value?.show()
	loading.value = true
	try {
		// Goes through the shared query cache (same key the settings page and
		// instance header read from) rather than calling the Tauri command
		// directly, so this always shows whatever was most recently saved —
		// even if that save happened moments ago from this same modal.
		const seconds = await queryClient.fetchQuery(
			instancePlaytimeCorrectionQueryOptions(props.instanceId),
		)
		hours.value = Math.round((seconds / 3600) * 10) / 10
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
		const seconds = Math.round(hours.value * 3600)
		await setPlaytimeCorrection(props.instanceId, seconds)
		// Modrinth Studios addition: pushes the new value straight into every
		// component reading this query key (the settings page's stat line,
		// the instance header's playtime badge) so they update immediately
		// instead of only refreshing the next time they happen to remount.
		queryClient.setQueryData(instanceKeys.playtimeCorrection(props.instanceId), seconds)
		emit('saved', seconds)
		hide()
	} catch (err) {
		handleError(err as Error)
	} finally {
		saving.value = false
	}
}

defineExpose({ show })

const titleMessage = defineMessage({
	id: 'instance.settings.tabs.general.playtime-correction.title',
	defaultMessage: 'Playtime correction',
})

const messages = defineMessages({
	description: {
		id: 'instance.settings.tabs.general.playtime-correction.description',
		defaultMessage:
			"Add hours from another launcher (like Prism) that Modrinth never tracked. This replaces the current correction — it isn't added on top each time you save.",
	},
	label: {
		id: 'instance.settings.tabs.general.playtime-correction.label',
		defaultMessage: 'Extra hours',
	},
})
</script>
<template>
	<NewModal ref="modal" :header="formatMessage(titleMessage)" width="450px" max-width="450px">
		<div class="flex flex-col gap-3">
			<p class="m-0 text-sm text-secondary">
				{{ formatMessage(messages.description) }}
			</p>
			<label class="flex flex-col gap-1">
				<span class="text-sm font-semibold text-contrast">{{ formatMessage(messages.label) }}</span>
				<Input
					v-model="hours"
					type="number"
					step="0.1"
					autocomplete="off"
					:disabled="loading"
					@keyup.enter="save"
				/>
			</label>
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
