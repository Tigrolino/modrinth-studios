/**
 * Modrinth Studios: client installations (Horizon, NoRisk Client, Lunar
 * Client, LabyMod). See packages/app-lib/src/api/studio_clients.
 */
import { invoke } from '@tauri-apps/api/core'

export type ClientKind = 'horizon' | 'norisk' | 'lunar' | 'labymod'

export interface ClientVersion {
	id: string
	name: string
	game_version: string
	loader: 'vanilla' | 'fabric'
	detail: string | null
	recommended: boolean
}

export interface ClientInstall {
	client: ClientKind
	version_id: string
	version_name: string
	variant_name?: string | null
	game_version: string
	prepared: boolean
	checked_at: number
}

export interface HorizonProject {
	id: string
	kind: 'mod' | 'resourcepack' | 'shader'
	name: string
	summary: string
	icon: string | null
	authors: string[]
	downloads: number
	published: string | null
	categories: string[]
	version: string
	source: string | null
	signed: boolean
	installed: boolean
}

export async function clientVersions(client: ClientKind) {
	return await invoke<ClientVersion[]>('plugin:studio-clients|studio_clients_versions', { client })
}

export async function setupClient(instanceId: string, client: ClientKind, version: ClientVersion) {
	return await invoke<ClientInstall>('plugin:studio-clients|studio_clients_setup', {
		instanceId,
		client,
		version,
	})
}

export async function getClientInstall(instanceId: string) {
	return await invoke<ClientInstall | null>('plugin:studio-clients|studio_clients_get', {
		instanceId,
	})
}

export async function prepareClient(instanceId: string) {
	return await invoke<void>('plugin:studio-clients|studio_clients_prepare', { instanceId })
}

export interface NewsItem {
	title: string
	date: string | null
	url: string | null
	image: string | null
	summary: string | null
	body: string
}

export interface NoRiskMod {
	id: string
	name: string
	modrinth_id: string | null
}

/** Local path of the client's logo (used as the instance icon). */
export async function clientIcon(client: ClientKind) {
	return await invoke<string>('plugin:studio-clients|studio_clients_icon', { client })
}

export async function noriskPackMods(instanceId: string) {
	return await invoke<NoRiskMod[]>('plugin:studio-clients|norisk_pack_mods', { instanceId })
}

export async function clientNews(client: ClientKind) {
	return await invoke<NewsItem[]>('plugin:studio-clients|studio_clients_news', { client })
}

export async function horizonStoreList(instanceId?: string | null) {
	return await invoke<HorizonProject[]>('plugin:studio-clients|horizon_store_list', {
		instanceId: instanceId ?? null,
	})
}

export async function horizonStoreInstall(instanceId: string, projectId: string) {
	return await invoke<void>('plugin:studio-clients|horizon_store_install', {
		instanceId,
		projectId,
	})
}

export async function horizonStoreRemove(instanceId: string, projectId: string) {
	return await invoke<void>('plugin:studio-clients|horizon_store_remove', {
		instanceId,
		projectId,
	})
}

export async function noriskSignIn() {
	return await invoke<string>('plugin:studio-clients|norisk_sign_in')
}

export async function noriskSignedIn() {
	return await invoke<string | null>('plugin:studio-clients|norisk_signed_in')
}

export interface NoRiskCape {
	hash: string
	uses: number
	elytra: boolean
	accepted: boolean
	creation_date: number
}

export function noriskCapeUrl(hash: string, review = false) {
	return `https://cdn.norisk.gg/capes/${review ? 'review' : 'prod'}/${hash}.png`
}

export async function noriskCapesBrowse(
	sort: string,
	timeFrame: string | null,
	cursor: string | null,
) {
	return await invoke<{ capes: NoRiskCape[]; next_cursor: string | null }>(
		'plugin:studio-clients|norisk_capes_browse',
		{ sort, timeFrame, cursor },
	)
}

export async function noriskCapesMine() {
	return await invoke<NoRiskCape[]>('plugin:studio-clients|norisk_capes_mine')
}

export async function noriskCapeEquip(hash: string) {
	return await invoke<void>('plugin:studio-clients|norisk_cape_equip', { hash })
}

export async function noriskCapeUnequip() {
	return await invoke<void>('plugin:studio-clients|norisk_cape_unequip')
}

export async function noriskCapeUpload(path: string) {
	return await invoke<string>('plugin:studio-clients|norisk_cape_upload', { path })
}

export async function noriskCapeTexture(hash: string, review = false) {
	return await invoke<string>('plugin:studio-clients|norisk_cape_texture', { hash, review })
}

/** A game folder of the client's own launcher ("Copy from your Lunar install"). */
export interface ImportSource {
	path: string
	label: string
	matches_version: boolean
	/** Lunar: the module the data was made with ('lunar' or 'fabric'). */
	module: string | null
	items: { id: string; label: string; detail: string | null }[]
}

export async function clientImportSources(client: ClientKind, gameVersion: string) {
	return await invoke<ImportSource[]>('plugin:studio-clients|studio_clients_import_sources', {
		client,
		gameVersion,
	})
}

export async function clientImport(
	client: ClientKind,
	instanceId: string,
	source: string,
	items: string[],
) {
	return await invoke<void>('plugin:studio-clients|studio_clients_import', {
		client,
		instanceId,
		source,
		items,
	})
}
