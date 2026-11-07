/**
 * Modrinth Studios addition: thin wrappers around the Rust Pterodactyl bridge
 * (apps/app/src/api/pterodactyl.rs). All network traffic to the panel and to
 * Wings goes through Rust, so the API key stays out of the webview and the
 * panel can be any URL (including plain http on the LAN).
 */
import { invoke } from '@tauri-apps/api/core'

export type PteroPanel = { id: string; name: string; url: string }

export function listPanels(): Promise<PteroPanel[]> {
	return invoke<PteroPanel[]>('plugin:pterodactyl|ptero_list_panels')
}

export function addPanel(name: string, url: string, apiKey: string): Promise<PteroPanel> {
	return invoke<PteroPanel>('plugin:pterodactyl|ptero_add_panel', { name, url, apiKey })
}

export function removePanel(panelId: string): Promise<void> {
	return invoke('plugin:pterodactyl|ptero_remove_panel', { panelId })
}

export class PteroHttpError extends Error {
	constructor(
		message: string,
		readonly status: number,
	) {
		super(message)
		this.name = 'PteroHttpError'
	}
}

function describeError(status: number, body: string): string {
	try {
		const parsed = JSON.parse(body) as { errors?: { detail?: string; code?: string }[] }
		const detail = parsed.errors?.map((e) => e.detail ?? e.code).filter(Boolean).join(' ')
		if (detail) return `${detail} (HTTP ${status})`
	} catch {
		// not JSON
	}
	return `Pterodactyl request failed (HTTP ${status})`
}

/** Calls the panel's Client API. `path` is relative to `/api/client`. */
export async function pteroRequest<T = unknown>(
	panelId: string,
	method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE',
	path: string,
	options: { body?: unknown; text?: string } = {},
): Promise<T> {
	const response = await invoke<{ status: number; body: string }>(
		'plugin:pterodactyl|ptero_request',
		{
			panelId,
			method,
			path,
			body: options.body === undefined ? null : options.body,
			textBody: options.text ?? null,
		},
	)
	if (response.status >= 400) {
		throw new PteroHttpError(describeError(response.status, response.body), response.status)
	}
	if (!response.body) return undefined as T
	try {
		return JSON.parse(response.body) as T
	} catch {
		return response.body as T
	}
}

/** Raw bytes of a Client API GET. */
export function pteroBytes(panelId: string, path: string): Promise<ArrayBuffer> {
	return invoke<ArrayBuffer>('plugin:pterodactyl|ptero_request_bytes', { panelId, path })
}

/**
 * Writes bytes to `path` on the server. `write` uses the Client API write
 * endpoint (text files / small edits); `upload` goes through a signed Wings
 * upload and works for any size.
 */
export async function pteroWrite(
	panelId: string,
	server: string,
	path: string,
	data: ArrayBuffer | Uint8Array,
	mode: 'write' | 'upload' = 'upload',
): Promise<void> {
	const bytes = data instanceof Uint8Array ? data : new Uint8Array(data)
	await invoke('plugin:pterodactyl|ptero_write_bytes', bytes, {
		headers: {
			'x-ptero-panel': encodeURIComponent(panelId),
			'x-ptero-server': encodeURIComponent(server),
			'x-ptero-path': encodeURIComponent(path),
			'x-ptero-mode': mode,
		},
	})
}

export function pteroUploadLocal(
	panelId: string,
	server: string,
	path: string,
	localPath: string,
): Promise<void> {
	return invoke('plugin:pterodactyl|ptero_upload_local', {
		panelId,
		server,
		path,
		localPath,
	})
}

export type MrpackSummary = {
	name: string
	version_id: string
	dependencies: Record<string, string>
	files: { path: string; sha1: string | null }[]
	overrides: number
}

export type MrpackProgress = { done: number; total: number; stage: string }

/** Installs a .mrpack onto the server (files + overrides). Progress is emitted on `event`. */
export async function pteroInstallMrpack(
	panelId: string,
	server: string,
	source: { url?: string; bytes?: ArrayBuffer; localPath?: string },
	event: string,
): Promise<MrpackSummary> {
	return invoke<MrpackSummary>(
		'plugin:pterodactyl|ptero_install_mrpack',
		source.bytes ? new Uint8Array(source.bytes) : new Uint8Array(),
		{
			headers: {
				'x-ptero-panel': encodeURIComponent(panelId),
				'x-ptero-server': encodeURIComponent(server),
				'x-ptero-event': encodeURIComponent(event),
				'x-ptero-url': encodeURIComponent(source.url ?? ''),
				'x-ptero-local': encodeURIComponent(source.localPath ?? ''),
			},
		},
	)
}

export function wsConnect(connId: string, panelId: string, server: string): Promise<void> {
	return invoke('plugin:pterodactyl|ptero_ws_connect', { connId, panelId, server })
}

export function wsSend(connId: string, text: string): Promise<void> {
	return invoke('plugin:pterodactyl|ptero_ws_send', { connId, text })
}

export function wsClose(connId: string): Promise<void> {
	return invoke('plugin:pterodactyl|ptero_ws_close', { connId })
}
