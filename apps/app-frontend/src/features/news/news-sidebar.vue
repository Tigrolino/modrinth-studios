<script setup lang="ts">
import { NewspaperIcon } from '@modrinth/assets'
import { ButtonLink, defineMessages, NewsArticleCard, useVIntl } from '@modrinth/ui'
import { useQuery } from '@tanstack/vue-query'
import { computed, watch } from 'vue'

import { openArticle } from './article-viewer'
import { type AppArticle, modrinthArticle, studioArticlesQueryOptions } from './articles'
import { newsArticlesQueryOptions } from './queries'

const { formatMessage } = useVIntl()
const messages = defineMessages({
	news: {
		id: 'app.news.title',
		defaultMessage: 'News',
	},
	viewAllNews: {
		id: 'app.news.view-all',
		defaultMessage: 'View all news',
	},
})

const { data: modrinthNews, error } = useQuery(newsArticlesQueryOptions())
// Modrinth Studios: our own update articles, in the same list.
const { data: studioNews } = useQuery(studioArticlesQueryOptions())

watch(error, (error) => {
	if (error) console.error('Failed to fetch news articles', error)
})

/** Studios articles are shown newest first, followed by Modrinth news. */
const news = computed<AppArticle[]>(() => [
	...(studioNews.value ?? []).slice(0, 3),
	...(modrinthNews.value ?? []).slice(0, 2).map(modrinthArticle),
])
</script>

<template>
	<div v-if="news.length > 0" class="p-4 flex flex-col items-center">
		<h3 class="text-base mb-4 text-primary font-medium m-0 text-left w-full">
			{{ formatMessage(messages.news) }}
		</h3>
		<div class="space-y-4 flex flex-col items-center w-full">
			<!-- Modrinth Studios: articles open in the app instead of the browser -->
			<div
				v-for="item in news"
				:key="`${item.source}:${item.slug}`"
				class="relative w-full"
				@click.capture.prevent.stop="openArticle(item)"
			>
				<NewsArticleCard
					:article="{
						path: item.link,
						thumbnail: item.banner ?? '',
						title: item.title,
						summary: item.summary,
						date: item.date,
					}"
				/>
				<span
					v-if="item.source === 'studios'"
					class="pointer-events-none absolute left-2 top-2 rounded-full bg-brand px-2 py-0.5 text-xs font-bold text-brand-inverted shadow"
				>
					Studios
				</span>
			</div>
			<ButtonLink
				type="colored"
				color="brand"
				size="xl"
				href="https://modrinth.com/news"
				target="_blank"
				class="my-4"
			>
				<NewspaperIcon />
				{{ formatMessage(messages.viewAllNews) }}
			</ButtonLink>
		</div>
	</div>
</template>
