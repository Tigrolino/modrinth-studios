import vue from '@vitejs/plugin-vue'
import { cpSync, existsSync, readFileSync, statSync } from 'fs'
import { resolve } from 'path'
import { defineConfig, type Plugin } from 'vite'
import svgLoader from 'vite-svg-loader'

import tauriConf from '../app/tauri.conf.json'

const projectRootDir = resolve(__dirname)
const appLibEnvDir = resolve(projectRootDir, '../../packages/app-lib')
const apiClientSource = resolve(projectRootDir, '../../packages/api-client/src/index.ts')

// Load .env from app-lib manually instead of using Vite's envDir, which would auto-load .env.local and override values
const envFilePath = resolve(appLibEnvDir, '.env')
if (existsSync(envFilePath)) {
	for (const line of readFileSync(envFilePath, 'utf-8').split('\n')) {
		const trimmed = line.trim()
		if (!trimmed || trimmed.startsWith('#')) continue
		const eqIndex = trimmed.indexOf('=')
		if (eqIndex === -1) continue
		const key = trimmed.slice(0, eqIndex)
		const value = trimmed.slice(eqIndex + 1)
		if (!(key in process.env)) {
			process.env[key] = value
		}
	}
}

// Modrinth Studios: the repo's `studio-news/` folder (update articles) is
// served at `/studio-news/` while developing, and copied into the build, so
// the app can read articles locally before (or without) them being on GitHub.
const studioNewsDir = resolve(projectRootDir, '../../studio-news')
const STUDIO_NEWS_TYPES: Record<string, string> = {
	json: 'application/json',
	md: 'text/markdown; charset=utf-8',
	png: 'image/png',
	webp: 'image/webp',
	jpg: 'image/jpeg',
	jpeg: 'image/jpeg',
	gif: 'image/gif',
	webm: 'video/webm',
	mp4: 'video/mp4',
}
function studioNews(): Plugin {
	let outDir = 'dist'
	return {
		name: 'studio-news',
		configResolved(config) {
			outDir = resolve(config.root, config.build.outDir)
		},
		configureServer(server) {
			server.middlewares.use('/studio-news', (req, res, next) => {
				const rel = decodeURIComponent((req.url ?? '/').split('?')[0])
				const file = resolve(studioNewsDir, '.' + rel)
				if (!file.startsWith(studioNewsDir) || !existsSync(file) || !statSync(file).isFile()) {
					return next()
				}
				const ext = file.split('.').pop()?.toLowerCase() ?? ''
				res.setHeader('Content-Type', STUDIO_NEWS_TYPES[ext] ?? 'application/octet-stream')
				res.setHeader('Cache-Control', 'no-cache')
				res.end(readFileSync(file))
			})
		},
		closeBundle() {
			if (existsSync(studioNewsDir)) {
				cpSync(studioNewsDir, resolve(outDir, 'studio-news'), {
					recursive: true,
					// Only what the app shows: no image scripts or readmes.
					filter: (src) => !/\.(py|md)$/i.test(src) || src.endsWith('article.md'),
				})
			}
		},
	}
}

// https://vitejs.dev/config/
export default defineConfig({
	css: {
		preprocessorOptions: {
			scss: {
				// TODO: dont forget about this
				silenceDeprecations: ['import'],
			},
		},
	},
	resolve: {
		alias: [
			{
				find: '@modrinth/api-client',
				replacement: apiClientSource,
			},
			{
				find: '@',
				replacement: resolve(projectRootDir, 'src'),
			},
		],
	},
	plugins: [
		studioNews(),
		vue(),
		svgLoader({
			svgoConfig: {
				plugins: [
					{
						name: 'preset-default',
						params: {
							overrides: {
								removeViewBox: false,
								cleanupIds: {
									minify: false,
								},
							},
						},
					},
				],
			},
		}),
	],

	// Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
	// prevent vite from obscuring rust errors
	clearScreen: false,
	// tauri expects a fixed port, fail if that port is not available
	server: {
		port: 1420,
		strictPort: true,
		headers: {
			'content-security-policy': Object.entries(tauriConf.app.security.csp)
				.map(([directive, sources]) => {
					// An additional websocket connect-src is required for Vite dev tools to work
					if (directive === 'connect-src') {
						sources = Array.isArray(sources) ? sources : [sources]
						sources.push('ws://localhost:1420')
					}

					return Array.isArray(sources)
						? `${directive} ${sources.join(' ')}`
						: `${directive} ${sources}`
				})
				.join('; '),
		},
	},
	// to make use of `TAURI_ENV_DEBUG` and other env variables
	// https://v2.tauri.app/reference/environment-variables/#tauri-cli-hook-commands
	envPrefix: ['VITE_', 'TAURI_', 'MODRINTH_', 'SHARED_INSTANCES_'],
	build: {
		rolldownOptions: {
			onwarn(warning, defaultHandler) {
				if (warning.code === 'INEFFECTIVE_DYNAMIC_IMPORT') return
				defaultHandler(warning)
			},
		},
		// Tauri supports es2021
		target: process.env.TAURI_ENV_PLATFORM == 'windows' ? 'chrome105' : 'safari13', // eslint-disable-line turbo/no-undeclared-env-vars
		// don't minify for debug builds
		minify: !process.env.TAURI_ENV_DEBUG, // eslint-disable-line turbo/no-undeclared-env-vars
		// produce sourcemaps for debug builds
		sourcemap: !!process.env.TAURI_ENV_DEBUG, // eslint-disable-line turbo/no-undeclared-env-vars
	},
})
