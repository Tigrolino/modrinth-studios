/**
 * Modrinth Studios addition: a WebSocket look-alike that speaks Archon's
 * console protocol to the hosting UI while actually talking to Wings through
 * the Rust proxy (which takes care of Wings auth and token refreshes).
 *
 * Also hosts a tiny in-memory bus feeding the fake Archon `/sync` stream, so
 * events like "backup finished" refresh the UI the same way they do on
 * Modrinth Hosting.
 */
import type { Archon } from '@modrinth/api-client'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import { markRestoreFinished } from './backups'
import { pteroRequest, wsClose, wsConnect, wsSend } from './bridge'
import { parsePteroServerId } from './ids'
import { fetchServerInfo, invalidatePteroServerInfo } from './server'

type WingsMessage = { event: string; args?: (string | null)[] }

// --- sync bus -------------------------------------------------------------

const syncControllers = new Map<string, Set<ReadableStreamDefaultController<Uint8Array>>>()
const encoder = new TextEncoder()
let syncEventCounter = 0

export function emitPteroSyncEvent(serverId: string, event: Archon.Sync.v1.SyncEvent) {
	const controllers = syncControllers.get(serverId)
	if (!controllers?.size) return
	const chunk = encoder.encode(`id: ${++syncEventCounter}\ndata: ${JSON.stringify(event)}\n\n`)
	for (const controller of controllers) {
		try {
			controller.enqueue(chunk)
		} catch {
			controllers.delete(controller)
		}
	}
}

export function createPteroSyncStream(serverId: string): ReadableStream<Uint8Array> {
	let registered: ReadableStreamDefaultController<Uint8Array> | null = null
	return new ReadableStream<Uint8Array>({
		start(controller) {
			registered = controller
			const set = syncControllers.get(serverId) ?? new Set()
			set.add(controller)
			syncControllers.set(serverId, set)
			controller.enqueue(encoder.encode(': pterodactyl\n\n'))
		},
		cancel() {
			if (registered) syncControllers.get(serverId)?.delete(registered)
		},
	})
}

// --- live socket registry (for events produced by Studio itself) ------------

const liveSockets = new Map<string, Set<PteroConsoleSocket>>()
const installProgress = new Map<string, Archon.Websocket.v0.InstallProgressItem[]>()

/** Sends an Archon-style event to every open console socket of a server. */
export function emitPteroSocketEvent(serverId: string, event: Archon.Websocket.v0.WSEvent) {
	for (const socket of liveSockets.get(serverId) ?? []) socket.deliver(event)
}

export function getPteroInstallProgress(serverId: string): Archon.Websocket.v0.InstallProgressItem[] {
	return installProgress.get(serverId) ?? []
}

/** Publishes the current install-progress snapshot (kept for sockets that connect later). */
export function setPteroInstallProgress(
	serverId: string,
	items: Archon.Websocket.v0.InstallProgressItem[],
) {
	if (items.length) installProgress.set(serverId, items)
	else installProgress.delete(serverId)
	emitPteroSocketEvent(serverId, { event: 'install-progress', items })
}

// --- console socket -------------------------------------------------------

const POWER_STATES: Record<string, Archon.Websocket.v0.PowerState> = {
	running: 'running',
	starting: 'starting',
	stopping: 'stopping',
	offline: 'stopped',
}

let connectionCounter = 0

export class PteroConsoleSocket {
	static readonly CONNECTING = 0
	static readonly OPEN = 1
	static readonly CLOSING = 2
	static readonly CLOSED = 3

	readyState = 0
	readonly url: string
	onopen: ((event: Event) => void) | null = null
	onmessage: ((event: MessageEvent) => void) | null = null
	onclose: ((event: CloseEvent) => void) | null = null
	onerror: ((event: Event) => void) | null = null

	private readonly connId = `studio${Date.now().toString(36)}${(++connectionCounter).toString(36)}`
	private unlisteners: UnlistenFn[] = []
	private wingsAuthed = false
	private clientAuthed = false
	private sentAuthOk = false
	private lastPowerState: Archon.Websocket.v0.PowerState | null = null
	private memoryLimitBytes = 0
	private diskLimitBytes = 0
	/** Set when the Wings socket couldn't be opened and we fall back to polling the panel. */
	private pollTimer: ReturnType<typeof setInterval> | null = null

	constructor(private readonly serverId: string) {
		this.url = `ptero://${serverId}`
		const set = liveSockets.get(serverId) ?? new Set()
		set.add(this)
		liveSockets.set(serverId, set)
		void this.open()
	}

	/** Used by emitPteroSocketEvent. */
	deliver(event: Archon.Websocket.v0.WSEvent) {
		if (this.readyState === PteroConsoleSocket.OPEN) this.emitLocal(event)
	}

	private async open() {
		const ref = parsePteroServerId(this.serverId)
		try {
			fetchServerInfo(ref)
				.then((info) => {
					this.memoryLimitBytes = info.attributes.limits.memory * 1024 * 1024
					this.diskLimitBytes = info.attributes.limits.disk * 1024 * 1024
				})
				.catch(() => undefined)

			this.unlisteners.push(
				await listen<string>(`ptero-ws:${this.connId}`, (event) => this.handleWings(event.payload)),
				await listen<number>(`ptero-ws-close:${this.connId}`, (event) =>
					this.finishClose(event.payload ?? 1006),
				),
			)
			await wsConnect(this.connId, ref.panelId, ref.identifier)
			if (this.readyState !== PteroConsoleSocket.CONNECTING) return
			this.readyState = PteroConsoleSocket.OPEN
			this.onopen?.(new Event('open'))
		} catch (error) {
			console.error('[pterodactyl] Console connection failed, falling back to polling', error)
			const reason =
				error instanceof Error
					? error.message
					: ((error as { message?: string })?.message ?? String(error))
			this.startPolling(reason)
		}
	}

	/**
	 * Fallback when Wings' console socket is unreachable (TLS, firewall,
	 * origin rules…): keep the panel usable by polling the Client API for
	 * state/stats and sending commands over HTTP. Live console output isn't
	 * available in this mode.
	 */
	private startPolling(reason: string) {
		if (this.readyState !== PteroConsoleSocket.CONNECTING) return
		const ref = parsePteroServerId(this.serverId)
		void wsClose(this.connId)
		this.wingsAuthed = true
		this.readyState = PteroConsoleSocket.OPEN
		this.onopen?.(new Event('open'))
		this.emitLocal({
			event: 'log',
			stream: 'stderr',
			message: `[Studio] Live console unavailable (${reason}). Showing status and stats from the panel instead; commands still work.`,
		})
		const poll = async () => {
			try {
				const response = await pteroRequest<{
					attributes: {
						current_state: string
						resources: {
							memory_bytes: number
							cpu_absolute: number
							disk_bytes: number
							network_rx_bytes: number
							network_tx_bytes: number
							uptime: number
						}
					}
				}>(ref.panelId, 'GET', `/servers/${ref.identifier}/resources`)
				const { current_state, resources } = response.attributes
				this.handleStats(
					JSON.stringify({
						memory_bytes: resources.memory_bytes,
						memory_limit_bytes: 0,
						cpu_absolute: resources.cpu_absolute,
						network: { rx_bytes: resources.network_rx_bytes, tx_bytes: resources.network_tx_bytes },
						state: current_state,
						uptime: resources.uptime,
						disk_bytes: resources.disk_bytes,
					}),
				)
			} catch {
				// keep polling
			}
		}
		void poll()
		this.pollTimer = setInterval(poll, 3000)
	}

	send(data: string) {
		if (this.readyState !== PteroConsoleSocket.OPEN) return
		let message: Archon.Websocket.v0.WSOutgoingMessage
		try {
			message = JSON.parse(data)
		} catch {
			return
		}
		if (message.event === 'auth') {
			this.clientAuthed = true
			this.maybeSendAuthOk()
		} else if (message.event === 'command') {
			if (this.pollTimer) {
				const ref = parsePteroServerId(this.serverId)
				void pteroRequest(ref.panelId, 'POST', `/servers/${ref.identifier}/command`, {
					body: { command: message.cmd },
				}).catch((error) =>
					this.emitLocal({
						event: 'log',
						stream: 'stderr',
						message: `[Studio] Command failed: ${error instanceof Error ? error.message : error}`,
					}),
				)
			} else {
				void wsSend(this.connId, JSON.stringify({ event: 'send command', args: [message.cmd] }))
			}
		}
	}

	close(code = 1000) {
		if (this.readyState >= PteroConsoleSocket.CLOSING) return
		this.readyState = PteroConsoleSocket.CLOSING
		void wsClose(this.connId)
		this.finishClose(code)
	}

	private finishClose(code: number) {
		if (this.readyState === PteroConsoleSocket.CLOSED) return
		this.readyState = PteroConsoleSocket.CLOSED
		liveSockets.get(this.serverId)?.delete(this)
		if (this.pollTimer) clearInterval(this.pollTimer)
		this.pollTimer = null
		for (const unlisten of this.unlisteners) unlisten()
		this.unlisteners = []
		this.onclose?.(
			new CloseEvent('close', { code, reason: '', wasClean: code === 1000 }),
		)
	}

	private maybeSendAuthOk() {
		if (this.wingsAuthed && this.clientAuthed && !this.sentAuthOk) {
			this.sentAuthOk = true
			this.emitLocal({ event: 'auth-ok' })
			// Archon follows auth with the current install progress; the hosting UI
			// waits for it before treating the server as ready.
			this.emitLocal({
				event: 'install-progress',
				items: installProgress.get(this.serverId) ?? [],
			})
		}
	}

	private emitLocal(event: Archon.Websocket.v0.WSEvent) {
		this.onmessage?.(new MessageEvent('message', { data: JSON.stringify(event) }))
	}

	private setPowerState(state: Archon.Websocket.v0.PowerState) {
		if (state === this.lastPowerState) return
		this.lastPowerState = state
		this.emitLocal({ event: 'power-state', state })
	}

	private handleWings(raw: string) {
		let message: WingsMessage
		try {
			message = JSON.parse(raw)
		} catch {
			return
		}
		const arg = message.args?.[0] ?? null
		switch (message.event) {
			case 'auth success':
				this.wingsAuthed = true
				this.maybeSendAuthOk()
				break
			case 'status':
				if (arg && POWER_STATES[arg]) this.setPowerState(POWER_STATES[arg])
				break
			case 'console output':
			case 'install output':
				if (arg !== null) this.emitLocal({ event: 'log', stream: 'stdout', message: arg })
				break
			case 'daemon message':
				if (arg !== null)
					this.emitLocal({ event: 'log', stream: 'stdout', message: `[Pterodactyl] ${arg}` })
				break
			case 'daemon error':
				if (arg !== null)
					this.emitLocal({ event: 'log', stream: 'stderr', message: `[Pterodactyl] ${arg}` })
				break
			case 'stats':
				if (arg) this.handleStats(arg)
				break
			case 'install completed':
				invalidatePteroServerInfo(this.serverId)
				this.emitLocal({ event: 'installation-result', result: 'ok' })
				break
			case 'backup completed':
				this.handleBackup(arg, 'create')
				break
			case 'backup restore completed':
				markRestoreFinished(this.serverId)
				this.handleBackup(arg, 'restore')
				break
			case 'jwt error':
				this.emitLocal({ event: 'auth-incorrect' })
				break
		}
	}

	private handleStats(raw: string) {
		try {
			const stats = JSON.parse(raw) as {
				memory_bytes: number
				memory_limit_bytes: number
				cpu_absolute: number
				network?: { rx_bytes: number; tx_bytes: number }
				state?: string
				uptime?: number
				disk_bytes: number
			}
			if (stats.state && POWER_STATES[stats.state]) this.setPowerState(POWER_STATES[stats.state])
			this.emitLocal({
				event: 'stats',
				cpu_percent: stats.cpu_absolute ?? 0,
				ram_usage_bytes: stats.memory_bytes ?? 0,
				ram_total_bytes: this.memoryLimitBytes || stats.memory_limit_bytes || 1,
				storage_usage_bytes: stats.disk_bytes ?? 0,
				storage_total_bytes: this.diskLimitBytes || stats.disk_bytes || 1,
				net_tx_bytes: stats.network?.tx_bytes ?? 0,
				net_rx_bytes: stats.network?.rx_bytes ?? 0,
			})
			if (stats.uptime && stats.state === 'running') {
				this.emitLocal({ event: 'uptime', uptime: Math.floor(stats.uptime / 1000) })
			}
		} catch {
			// ignore malformed stats
		}
	}

	private handleBackup(raw: string | null, task: 'create' | 'restore') {
		let backupId = ''
		let successful = true
		if (raw) {
			try {
				const parsed = JSON.parse(raw) as { uuid?: string; is_successful?: boolean }
				backupId = parsed.uuid ?? ''
				successful = parsed.is_successful ?? true
			} catch {
				// ignore
			}
		}
		this.emitLocal({
			event: 'backup-progress',
			id: backupId,
			task,
			state: successful ? 'done' : 'failed',
			progress: 1,
		})
		emitPteroSyncEvent(this.serverId, {
			type: task === 'create' ? 'backup.operation.create.done' : 'backup.operation.restore.done',
			world_id: 'default',
			backup_id: backupId,
			operation_id: 0,
			status: successful ? 'completed' : 'failed',
		})
	}
}
