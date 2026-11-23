<script setup lang="ts">
/**
 * Modrinth Studios: reads a news article inside the app, laid out like an
 * article on modrinth.com/news (banner, title, summary, date, body). Works
 * for Studios' own articles and Modrinth's. Also opens a big update's
 * article once, the first time that version starts.
 */
import { ExternalIcon, SpinnerIcon } from '@modrinth/assets'
import { ArticleBody, Button, NewModal, useFormatDateTime } from '@modrinth/ui'
import { getVersion } from '@tauri-apps/api/app'
import { useQuery } from '@tanstack/vue-query'
import { computed, onMounted, onUnmounted, ref, useTemplateRef, watch } from 'vue'

import { openUrl } from '@/platform/studio-client-desktop'

import { markArticleSeen, openArticle, openedArticle, seenArticles } from './article-viewer'
import { articleHtml, studioArticles } from './articles'

const modal = useTemplateRef<InstanceType<typeof NewModal>>('modal')
const formatDate = useFormatDateTime({ dateStyle: 'long' })
const bannerFailed = ref(false)

const article = computed(() => openedArticle.value)
function clearArticle() {
	openedArticle.value = null
}

watch(openedArticle, (a) => {
	bannerFailed.value = false
	if (a) {
		modal.value?.show()
		if (a.source === 'studios') markArticleSeen(a.slug)
	}
})

const bodyQuery = useQuery({
	queryKey: computed(() => ['news', 'article', article.value?.source, article.value?.slug]),
	queryFn: ({ signal }) => articleHtml(article.value!, signal),
	enabled: computed(() => !!article.value),
	staleTime: 30 * 60_000,
	retry: false,
})

const dateLabel = computed(() =>
	article.value?.date && Number.isFinite(Date.parse(article.value.date))
		? formatDate(article.value.date)
		: '',
)

// A big update's article pops up once, the first time that version starts.
let popupTimer: ReturnType<typeof setTimeout> | undefined
let disposed = false
onUnmounted(() => {
	disposed = true
	clearTimeout(popupTimer)
})
onMounted(() => {
	popupTimer = setTimeout(async () => {
		try {
			const version = await getVersion()
			const seen = seenArticles()
			const latest = (await studioArticles()).find(
				(a) => a.popup && a.version === version && !seen.includes(a.slug),
			)
			if (!disposed && latest && !openedArticle.value) openArticle(latest)
		} catch {
			// Offline or no articles: nothing to show.
		}
	}, 2500)
})
</script>

<template>
	<NewModal
		ref="modal"
		:header="article?.source === 'studios' ? 'Modrinth Studios news' : 'Modrinth news'"
		max-width="56rem"
		width="52rem"
		scrollable
		max-content-height="78vh"
		:animate-resize="false"
		:on-after-hide="clearArticle"
	>
		<article v-if="article" class="flex w-full flex-col">
			<img
				v-if="article.banner && !bannerFailed"
				:src="article.banner"
				alt=""
				class="mb-6 aspect-[2/1] w-full rounded-2xl border border-solid border-surface-5 object-cover"
				@error="bannerFailed = true"
			/>
			<h1 class="m-0 text-3xl font-extrabold leading-tight text-contrast sm:text-4xl">
				{{ article.title }}
			</h1>
			<p class="m-0 mt-3 text-lg leading-relaxed text-secondary">{{ article.summary }}</p>
			<div class="mt-4 flex flex-wrap items-center gap-2 text-sm text-secondary">
				<span
					class="rounded-full px-2.5 py-0.5 text-xs font-semibold"
					:class="
						article.source === 'studios'
							? 'bg-brand-highlight text-brand'
							: 'bg-surface-4 text-contrast'
					"
				>
					{{ article.source === 'studios' ? 'Modrinth Studios' : 'Modrinth' }}
				</span>
				<span v-if="dateLabel">{{ dateLabel }}</span>
				<span v-if="article.version">· Version {{ article.version }}</span>
			</div>
			<hr class="my-6 border-0 border-t border-solid border-surface-5" />

			<div v-if="bodyQuery.isPending.value" class="flex items-center gap-2 py-8 text-secondary">
				<SpinnerIcon class="size-5 animate-spin" /> Loading article…
			</div>
			<div v-else-if="bodyQuery.isError.value" class="flex flex-col items-start gap-3 py-4">
				<span class="text-secondary">
					{{ (bodyQuery.error.value as Error)?.message || "This article couldn't be loaded." }}
				</span>
				<Button @click="openUrl(article.link)"> <ExternalIcon /> Read it in your browser </Button>
			</div>
			<ArticleBody v-else class="studio-article" :html="bodyQuery.data.value ?? ''" />
		</article>
		<template #actions>
			<div class="flex w-full justify-end">
				<Button v-if="article" type="outlined" @click="openUrl(article.link)">
					<ExternalIcon />
					{{ article.source === 'studios' ? 'View on GitHub' : 'Open on modrinth.com' }}
				</Button>
			</div>
		</template>
	</NewModal>
</template>

<style scoped>
/* Same reading width and rhythm as modrinth.com/news articles. */
.studio-article :deep(.markdown-body) {
	font-size: 1rem;
	line-height: 1.7;
}
.studio-article :deep(img),
.studio-article :deep(video) {
	max-width: 100%;
	border-radius: 1rem;
	border: 1px solid var(--surface-5);
}
.studio-article :deep(.video-wrapper video) {
	width: 100%;
}
.studio-article :deep(h2) {
	margin-top: 2rem;
}
</style>
