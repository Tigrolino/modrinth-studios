// Stand-in for every `@tauri-apps/*` module in unit tests (see vitest.config.ts).
// Anything that would really talk to the app's backend fails loudly.
function unavailable(name: string) {
	return () => {
		throw new Error(`${name} isn't available in unit tests`)
	}
}

export const invoke = async (command: string) => {
	throw new Error(`invoke('${command}') isn't available in unit tests`)
}
export class Channel<T = unknown> {
	onmessage: (message: T) => void = () => {}
}
export const listen = async () => () => {}
export const convertFileSrc = (path: string) => path
export const fetch = unavailable('tauri fetch')
export const getCurrentWindow = unavailable('getCurrentWindow')
export const readFile = unavailable('readFile')
export const open = unavailable('open')
export const save = unavailable('save')
