<script setup lang="ts">
/** Modrinth Studios: the small round "?" that explains something on hover (Modrinth's own icon). */
import { UnknownIcon } from '@modrinth/assets'

defineProps<{ text: string }>()
</script>

<template>
	<span
		v-tooltip="text"
		tabindex="0"
		role="img"
		:aria-label="text"
		class="inline-flex shrink-0 cursor-help text-secondary transition-colors hover:text-contrast"
		@click.stop
	>
		<UnknownIcon class="size-[18px]" aria-hidden="true" />
	</span>
</template>
