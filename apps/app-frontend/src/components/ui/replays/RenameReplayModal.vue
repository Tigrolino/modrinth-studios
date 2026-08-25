<!-- Modrinth Studios addition: new file, not touching any upstream modal. -->
<script setup lang="ts">
import { SaveIcon, XIcon } from '@modrinth/assets'
import {
	Button,
	commonMessages,
	defineMessage,
	injectNotificationManager,
	Input,
	NewModal,
	useVIntl,
} from '@modrinth/ui'
import { ref } from 'vue'

import { renameReplay, type Replay } from '@/helpers/replays'

const props = defineProps<{
	instanceId: string
}>()

const emit = defineEmits<{
	submit: []
}>()

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()

const modal = ref<InstanceType<typeof NewModal>>()
const current = ref<Replay | null>(null)
const newName = ref('')
const saving = ref(false)

function extensionOf(fileName: string): string {
	const idx = fileName.lastIndexOf('.')
	return idx === -1 ? '' : fileName.slice(idx)
}

function show(replay: Replay) {
	current.value = replay
	newName.value = replay.fileName.slice(
		0,
		replay.fileName.length - extensionOf(replay.fileName).length,
	)
	modal.value?.show()
}

function hide() {
	modal.value?.hide()
}

async function save() {
	if (!current.value || !newName.value.trim() || saving.value) return
	saving.value = true
	try {
		const ext = extensionOf(current.value.fileName)
		await renameReplay(
			props.instanceId,
			current.value.kind,
			current.value.fileName,
			`${newName.value.trim()}${ext}`,
		)
		emit('submit')
		hide()
	} catch (err) {
		handleError(err as Error)
	} finally {
		saving.value = false
	}
}

defineExpose({ show })

const titleMessage = defineMessage({
	id: 'app.instance.replays.rename-title',
	defaultMessage: 'Rename replay',
})
</script>
<template>
	<NewModal ref="modal" :header="formatMessage(titleMessage)" width="450px" max-width="450px">
		<Input
			v-model="newName"
			type="text"
			autocomplete="off"
			:spellcheck="false"
			@keyup.enter="save"
		/>
		<template #actions>
			<div class="flex gap-2 justify-end">
				<Button type="outlined" @click="hide()">
					<XIcon />
					{{ formatMessage(commonMessages.cancelButton) }}
				</Button>
				<Button type="colored" color="brand" :disabled="!newName.trim() || saving" @click="save">
					<SaveIcon />
					{{ formatMessage(commonMessages.saveChangesButton) }}
				</Button>
			</div>
		</template>
	</NewModal>
</template>
