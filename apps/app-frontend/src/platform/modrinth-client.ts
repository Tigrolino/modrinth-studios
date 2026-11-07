import {
	AuthFeature,
	NodeAuthFeature,
	nodeAuthState,
	PanelVersionFeature,
	setWebSocketFactory,
	TauriModrinthClient,
	VerboseLoggingFeature,
} from '@modrinth/api-client'
import { getVersion } from '@tauri-apps/api/app'

import { config } from '@/config'
import { get as getCreds } from '@/helpers/mr_auth'
import { createLocalFileUploader } from '@/platform/adapters/local-file-upload'
// Modrinth Studios addition: self-hosted Pterodactyl servers in the hosting tab
import { PterodactylFeature } from '@/platform/pterodactyl/feature'
import { isPteroServerId } from '@/platform/pterodactyl/ids'
import { PteroConsoleSocket } from '@/platform/pterodactyl/socket'

export function createAppClient(): TauriModrinthClient {
	const appVersion = getVersion()

	setWebSocketFactory((serverId) =>
		isPteroServerId(serverId) ? (new PteroConsoleSocket(serverId) as unknown as WebSocket) : null,
	)

	return new TauriModrinthClient({
		userAgent: async () => `modrinth/theseus/${await appVersion} (support@modrinth.com)`,
		labrinthBaseUrl: config.labrinthBaseUrl,
		archonBaseUrl: config.archonBaseUrl,
		sharedInstancesBaseUrl: config.sharedInstancesBaseUrl,
		localFileUploader: createLocalFileUploader(),
		features: [
			// Must stay first (innermost): see platform/pterodactyl/feature.ts
			new PterodactylFeature(),
			new NodeAuthFeature({
				getAuth: () => nodeAuthState.getAuth?.() ?? null,
				refreshAuth: async () => {
					if (nodeAuthState.refreshAuth) {
						await nodeAuthState.refreshAuth()
					}
				},
			}),
			new AuthFeature({
				token: async () => (await getCreds())?.session,
			}),
			new PanelVersionFeature(),
			new VerboseLoggingFeature(),
		],
	})
}
