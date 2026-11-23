/**
 * Modrinth Studios: news articles read inside the app.
 *
 * - Studios articles live in `studio-news/` in this repo (see its README):
 *   fetched from the `studio` branch on GitHub, so they go live when pushed.
 * - Modrinth's own articles are read from Modrinth's open-source blog
 *   (`packages/blog/articles/<slug>.md` in modrinth/code), so they open in
 *   the app too.
 */
import { renderString } from '@modrinth/utils/parse'
import { invoke } from '@tauri-apps/api/core'
import { queryOptions } from '@tanstack/vue-query'

const REMOTE_BASE =
	'https://raw.githubusercontent.com/Tigrolino/modrinth-studios/studio/studio-news/'
/** The repo's own `studio-news/`, served by Vite in dev and copied into the build. */
const LOCAL_BASE = '/studio-news/'
/**
 * Where to look first: the local folder while developing (so articles can be
 * tried before they're pushed), GitHub in releases (so new articles show up
 * without an app update), each falling back to the other.
 */
const STUDIO_BASES = import.meta.env.DEV ? [LOCAL_BASE, REMOTE_BASE] : [REMOTE_BASE, LOCAL_BASE]
const STUDIO_SOURCE = 'https://github.com/Tigrolino/modrinth-studios/tree/studio/studio-news/'
const MODRINTH_BLOG_RAW =
	'https://raw.githubusercontent.com/modrinth/code/main/packages/blog/articles/'

export interface StudioArticleMeta {
	slug: string
	title: string
	summary: string
	date: string
	version?: string
	banner?: string
	popup?: boolean
}

/** One article, from either source, as the viewer shows it. */
export interface AppArticle {
	source: 'studios' | 'modrinth'
	slug: string
	title: string
	summary: string
	date: string
	banner: string | null
	/** Where it can be read in a browser. */
	link: string
	/** Studios: the folder the article was found in (local or GitHub). */
	base?: string
	version?: string
	popup?: boolean
}

/** A file next to a Studios article (banner, images, videos). */
function studioFile(base: string, slug: string, file: string) {
	return `${base}${slug}/${file.replace(/^\.\//, '')}`
}

function toAppArticle(meta: StudioArticleMeta, base: string): AppArticle {
	return {
		source: 'studios',
		slug: meta.slug,
		title: meta.title,
		summary: meta.summary,
		date: meta.date,
		banner: meta.banner ? studioFile(base, meta.slug, meta.banner) : null,
		link: `${STUDIO_SOURCE}${meta.slug}/article.md`,
		version: meta.version,
		popup: meta.popup,
		base,
	}
}

async function fetchText(url: string, signal?: AbortSignal) {
	// GitHub goes through the app (no CSP/CORS in the way); the local folder
	// is served by the app itself.
	if (url.startsWith('https://')) {
		return await invoke<string>('plugin:studio-clients|studio_news_text', { url })
	}
	const res = await fetch(url, { signal, cache: 'no-cache' })
	if (!res.ok) throw new Error(`HTTP ${res.status}`)
	return await res.text()
}

export async function studioArticles(signal?: AbortSignal): Promise<AppArticle[]> {
	let index: { articles?: StudioArticleMeta[] } | undefined
	let base = STUDIO_BASES[0]
	for (const candidate of STUDIO_BASES) {
		try {
			index = JSON.parse(await fetchText(`${candidate}index.json`, signal))
			base = candidate
			break
		} catch {
			// Try the next place.
		}
	}
	return (index?.articles ?? [])
		.filter((a) => a.slug && a.title)
		.map((a) => toAppArticle(a, base))
		.sort((a, b) => (Date.parse(b.date) || 0) - (Date.parse(a.date) || 0))
}

export function studioArticlesQueryOptions() {
	return queryOptions({
		queryKey: ['news', 'studios'] as const,
		queryFn: ({ signal }) => studioArticles(signal),
		staleTime: 30 * 60_000,
		retry: false,
		refetchOnWindowFocus: false,
	})
}

/** A Modrinth news-feed article, readable in the app. */
export function modrinthArticle(a: {
	path: string
	title: string
	summary: string
	date: string
	thumbnail: string
}): AppArticle {
	const slug = a.path.replace(/\/+$/, '').split('/').pop() ?? ''
	return {
		source: 'modrinth',
		slug,
		title: a.title,
		summary: a.summary,
		date: a.date,
		banner: a.thumbnail || null,
		link: a.path,
	}
}

function stripFrontMatter(md: string) {
	return md.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, '')
}

/** Points relative images, videos and links at where the article's files live. */
function resolveUrls(md: string, resolve: (path: string) => string) {
	return md
		.replace(/(\]\()(\.\/[^)\s]+)/g, (_, p, path) => `${p}${resolve(path)}`)
		.replace(/(src=["'])(\.\/[^"']+)/g, (_, p, path) => `${p}${resolve(path)}`)
		.replace(/(src=["'])(\/news\/[^"']+)/g, (_, p, path) => `${p}https://modrinth.com${path}`)
}

/** The article's body as HTML, ready for ArticleBody. */
export async function articleHtml(article: AppArticle, signal?: AbortSignal): Promise<string> {
	let md: string
	if (article.source === 'studios') {
		const bases = [
			article.base ?? STUDIO_BASES[0],
			...STUDIO_BASES.filter((b) => b !== article.base),
		]
		let found: { text: string; base: string } | undefined
		for (const base of bases) {
			try {
				found = { text: await fetchText(`${base}${article.slug}/article.md`, signal), base }
				break
			} catch {
				// Try the next place.
			}
		}
		if (!found) throw new Error('This article is not available right now.')
		md = resolveUrls(stripFrontMatter(found.text), (p) => studioFile(found.base, article.slug, p))
	} else {
		md = await fetchText(`${MODRINTH_BLOG_RAW}${article.slug}.md`, signal).catch(() => {
			throw new Error('This article is not available in the app.')
		})
		md = resolveUrls(
			stripFrontMatter(md),
			(p) => `https://modrinth.com/news/article/${article.slug}/${p.replace(/^\.\//, '')}`,
		)
	}
	return await withLocalVideos(renderString(md))
}

/**
 * Modrinth's article videos (webm on modrinth.com) are downloaded by the app
 * and put in the article as in-memory blob URLs, with the attributes the
 * article asked for, so they play inline like on the website. If one can't
 * be downloaded, a link to it is shown instead of an empty box.
 */
async function withLocalVideos(html: string): Promise<string> {
	const videos = [...html.matchAll(/<video\b[^>]*>[\s\S]*?<\/video>/g)]
	let out = html
	for (const [tag] of videos) {
		const src = tag.match(/src="(https:\/\/modrinth\.com\/news\/[^"]+)"/)?.[1]
		if (!src) continue
		const type = tag.match(/type="([^"]+)"/)?.[1] ?? 'video/webm'
		try {
			const bytes = await invoke<ArrayBuffer>('plugin:studio-clients|studio_news_media', {
				url: src,
			})
			const url = URL.createObjectURL(new Blob([bytes], { type }))
			out = out.replace(
				tag,
				`<video src="${url}" autoplay loop muted playsinline controls preload="auto"></video>`,
			)
		} catch (e) {
			console.warn('Article video could not be loaded', src, e)
			out = out.replace(
				tag,
				`<p><a href="${src}" target="_blank" rel="noopener">Watch this video on modrinth.com</a> (it couldn't be loaded here: ${String(
					(e as Error)?.message ?? e,
				).replace(/[<>"]/g, '')})</p>`,
			)
		}
	}
	return out
}
