import { fileURLToPath, URL } from 'node:url'

import { defineConfig } from 'vitest/config'

// Unit tests for Modrinth Studios' frontend logic (no app window, no Tauri).
// Tauri's modules are swapped for a stub so code that imports them can load.
const tauriStub = fileURLToPath(new URL('./src/test/tauri-stub.ts', import.meta.url))

export default defineConfig({
	resolve: {
		alias: [
			{ find: /^@tauri-apps\/.*$/, replacement: tauriStub },
			{ find: /^@\//, replacement: fileURLToPath(new URL('./src/', import.meta.url)) },
		],
	},
	test: {
		include: ['src/**/*.test.ts'],
		environment: 'node',
	},
})
