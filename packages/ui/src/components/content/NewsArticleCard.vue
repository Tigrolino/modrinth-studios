<script setup lang="ts">
import { ref, watch } from 'vue'

import { useFormatDateTime } from '../../composables'
import AutoLink from '../base/AutoLink.vue'

const formatDate = useFormatDateTime({ dateStyle: 'long' })

export interface Article {
	path: string
	thumbnail: string
	title: string
	summary: string
	date: string
}

const props = defineProps<{
	article: Article
}>()
const imageFailed = ref(false)
watch(
	() => props.article.thumbnail,
	() => {
		imageFailed.value = false
	},
)
</script>

<template>
	<AutoLink
		:to="article.path"
		class="active:scale-[0.99]! group flex flex-col transition-all ease-in-out hover:brightness-125 cursor-pointer"
	>
		<article class="flex h-full grow flex-col gap-4">
			<img
				v-if="article.thumbnail && !imageFailed"
				:src="article.thumbnail"
				alt=""
				@error="imageFailed = true"
				class="aspect-video w-full rounded-xl border-[1px] border-solid border-button-border object-cover"
			/>
			<div class="flex grow flex-col gap-2">
				<h3 class="m-0 text-base leading-tight group-hover:underline">
					{{ article.title }}
				</h3>
				<p v-if="article.summary" class="m-0 text-sm leading-tight text-primary">
					{{ article.summary }}
				</p>
				<div
					v-if="article.date && Number.isFinite(Date.parse(article.date))"
					class="mt-auto text-sm text-secondary"
				>
					{{ formatDate(article.date) }}
				</div>
			</div>
		</article>
	</AutoLink>
</template>
