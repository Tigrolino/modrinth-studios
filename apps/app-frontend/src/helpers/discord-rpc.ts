// Modrinth Studios addition: bindings for the `discord-rpc` Tauri plugin
// (packages/app-lib/src/api/discord_rpc.rs + apps/app/src/api/discord_rpc.rs).
// New file — nothing here touches upstream Modrinth code.
import { invoke } from '@tauri-apps/api/core'

export type DiscordActivityMode = 'default' | 'detailed' | 'minimal' | 'custom'
export type DiscordActivityKind = 'playing' | 'listening' | 'watching' | 'competing'

// Field names are plain snake_case on purpose — the Rust struct has no
// `#[serde(rename_all = "camelCase")]`, so this passes straight through.
export interface DiscordRpcSettings {
	mode: DiscordActivityMode
	activity_type: DiscordActivityKind
	show_elapsed_time: boolean
	custom_state_template: string
	custom_details_template: string
	idle_text: string
	button_1_label: string | null
	button_1_url: string | null
	button_2_label: string | null
	button_2_url: string | null
}

export function defaultDiscordRpcSettings(): DiscordRpcSettings {
	return {
		mode: 'default',
		activity_type: 'playing',
		show_elapsed_time: true,
		custom_state_template: '',
		custom_details_template: '',
		idle_text: 'Idling...',
		button_1_label: null,
		button_1_url: null,
		button_2_label: null,
		button_2_url: null,
	}
}

export async function getDiscordRpcSettings(): Promise<DiscordRpcSettings> {
	return await invoke('plugin:discord-rpc|discord_rpc_get_settings')
}

export async function setDiscordRpcSettings(settings: DiscordRpcSettings): Promise<void> {
	return await invoke('plugin:discord-rpc|discord_rpc_set_settings', { settings })
}
