<!--
	Modrinth Studios addition: new file. A Modrinth-styled trigger for color
	selection, replacing the raw browser `<input type="color">` square (which
	looks out of place next to the rest of the settings UI). Clicking it still
	opens the real OS color picker via a visually-hidden native input — this
	keeps full, reliable color selection (hue/sat/hex/eyedropper) without
	needing to build and blind-test a custom HSV picker from scratch.
-->
<script setup lang="ts">
import { ref } from 'vue'

const props = defineProps<{
	modelValue: string
	label?: string
}>()

const emit = defineEmits<{
	'update:modelValue': [string]
}>()

const input = ref<HTMLInputElement>()

function triggerPicker() {
	input.value?.click()
}

function onInput(event: Event) {
	emit('update:modelValue', (event.target as HTMLInputElement).value)
}
</script>

<template>
	<button
		type="button"
		class="studio-color-swatch"
		:style="{ '--studio-color-swatch-value': props.modelValue }"
		@click="triggerPicker"
	>
		<span class="studio-color-swatch__ring">
			<span class="studio-color-swatch__fill" />
		</span>
		<span v-if="label" class="studio-color-swatch__label">{{ label }}</span>
		<input
			ref="input"
			type="color"
			class="studio-color-swatch__input"
			:value="props.modelValue"
			tabindex="-1"
			aria-hidden="true"
			@input="onInput"
		/>
	</button>
</template>

<style scoped>
.studio-color-swatch {
	position: relative;
	display: inline-flex;
	align-items: center;
	gap: 0.5rem;
	padding: 0.3125rem 0.75rem 0.3125rem 0.3125rem;
	border-radius: 9999px;
	border: 1px solid var(--color-surface-5, var(--color-divider));
	background-color: var(--color-button-bg);
	color: var(--color-primary);
	cursor: pointer;
	font: inherit;
	transition:
		border-color 100ms ease,
		background-color 100ms ease;
}

.studio-color-swatch:hover {
	background-color: var(--color-button-bg-hover, var(--color-button-bg));
	border-color: var(--color-surface-6, var(--color-brand));
}

.studio-color-swatch:focus-visible {
	outline: 2px solid var(--color-brand);
	outline-offset: 2px;
}

.studio-color-swatch__ring {
	display: inline-flex;
	align-items: center;
	justify-content: center;
	width: 1.75rem;
	height: 1.75rem;
	border-radius: 9999px;
	padding: 3px;
	background-color: color-mix(in srgb, var(--studio-color-swatch-value) 35%, transparent);
	flex-shrink: 0;
}

.studio-color-swatch__fill {
	display: block;
	width: 100%;
	height: 100%;
	border-radius: 9999px;
	background-color: var(--studio-color-swatch-value);
	box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.15);
}

.studio-color-swatch__label {
	font-size: 0.8125rem;
	font-weight: 600;
	white-space: nowrap;
}

.studio-color-swatch__input {
	position: absolute;
	inset: 0;
	opacity: 0;
	pointer-events: none;
	width: 100%;
	height: 100%;
	border: none;
	padding: 0;
}
</style>
