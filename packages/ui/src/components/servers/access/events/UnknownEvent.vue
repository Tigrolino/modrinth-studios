<template>
	<BaseEvent>
		<template v-if="label">
			<span class="mr-1 text-primary">{{ label }}</span>
			<EventInlineText v-if="detail" :text="detail" class="font-mono text-secondary" />
		</template>
		<template v-else>
			<span class="mr-1 text-secondary">{{ formatMessage(messages.unknownEvent) }}</span>
			<EventInlineText :text="rawAction" class="font-mono text-secondary" />
		</template>
	</BaseEvent>
</template>

<script setup lang="ts">
import { defineMessages, useVIntl } from '../../../../composables/i18n'
import BaseEvent from './BaseEvent.vue'
import EventInlineText from './EventInlineText.vue'

defineProps<{
	rawAction: string
	/** Modrinth Studios: readable name for panel events Archon doesn't know. */
	label?: string | null
	detail?: string | null
}>()

const { formatMessage } = useVIntl()

const messages = defineMessages({
	unknownEvent: {
		id: 'servers.audit-log.event.unknown',
		defaultMessage: 'Unknown event',
	},
})
</script>
