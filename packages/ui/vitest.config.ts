import { defineConfig } from 'vitest/config'

// Unit tests for plain TypeScript logic in the UI package (no components).
export default defineConfig({
	test: {
		include: ['src/**/*.test.ts'],
		environment: 'node',
	},
})
