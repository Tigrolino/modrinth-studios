/** Modrinth Studios: opens a news article in the in-app article popup. */
import { ref } from 'vue'

import type { AppArticle } from './articles'

export const openedArticle = ref<AppArticle | null>(null)

export function openArticle(article: AppArticle) {
	openedArticle.value = { ...article }
}

const SEEN_KEY = 'studios-news-seen'

export function seenArticles(): string[] {
	try {
		const parsed: unknown = JSON.parse(localStorage.getItem(SEEN_KEY) ?? '[]')
		return Array.isArray(parsed)
			? parsed.filter((value): value is string => typeof value === 'string')
			: []
	} catch {
		return []
	}
}

export function markArticleSeen(slug: string) {
	try {
		const seen = new Set(seenArticles())
		seen.add(slug)
		localStorage.setItem(SEEN_KEY, JSON.stringify([...seen]))
	} catch {
		// Storage unavailable: the popup may show again next time, that's all.
	}
}
