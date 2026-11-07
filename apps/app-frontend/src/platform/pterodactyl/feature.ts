/**
 * Modrinth Studios addition: request feature that intercepts every Archon /
 * Kyros request aimed at a Pterodactyl server (`ptero-…` id or the fake
 * `….ptero.studio.invalid` node host) and answers it from the panel instead.
 *
 * It must be the *first* feature in the client's list: features run from the
 * last to the first, so this one sees the request after NodeAuthFeature has
 * resolved the node URL, and before anything hits the network.
 */
import { AbstractFeature, type RequestContext } from '@modrinth/api-client'

import { findPteroServerId, isPteroNodeUrl, isPteroServerId, resolvePteroDownloadUrl } from './ids'
import { handlePteroRequest } from './router'

export class PterodactylFeature extends AbstractFeature {
	constructor() {
		super({ enabled: true })
	}

	shouldApply(): boolean {
		// The node URL is only known after NodeAuthFeature ran, so decide in execute().
		return true
	}

	async execute<T>(next: () => Promise<T>, context: RequestContext): Promise<T> {
		const url = context.url ?? ''
		const api = context.options.api

		// Downloads to a fake node URL: swap in the real signed Wings URL.
		if (isPteroNodeUrl(url) && context.options.params?.token && url.includes('download')) {
			context.url = resolvePteroDownloadUrl(
				`${url}?token=${encodeURIComponent(String(context.options.params.token))}`,
			)
			context.options.params = undefined
			context.options.headers = {}
			return next()
		}

		if (isPteroNodeUrl(url)) {
			const serverId = findPteroServerId(url)
			if (serverId) return (await handlePteroRequest(context, serverId, 'node')) as T
		}

		if (api === 'archon') {
			const scope = context.options.params?.scope
			if (
				context.path.replace(/^\//, '') === 'sync' &&
				typeof scope === 'string' &&
				isPteroServerId(scope.replace(/^server:/, ''))
			) {
				return (await handlePteroRequest(context, scope.replace(/^server:/, ''), 'sync')) as T
			}
			const serverId = findPteroServerId(context.path)
			if (serverId) return (await handlePteroRequest(context, serverId, 'archon')) as T
		}

		return next()
	}
}
