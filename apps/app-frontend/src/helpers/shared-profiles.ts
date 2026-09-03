// Modrinth Studios addition: shared Minecraft folders — lets several
// instances use the same worlds/config/resourcepacks/options.txt/servers.dat
// instead of each keeping its own private copy. See
// packages/app-lib/src/api/shared_profile.rs for what actually happens on
// disk; this file is just the thin invoke() wrapper for it.
import { invoke } from '@tauri-apps/api/core'

export interface SharedProfile {
	id: string
	name: string
	share_saves: boolean
	share_config: boolean
	share_resourcepacks: boolean
	share_options: boolean
	share_servers: boolean
	/** The instance that originally created this shared folder, if known.
	 * Purely "who created this" bookkeeping — every member instance gets a
	 * real private copy restored (never emptied out) whenever it's detached
	 * from an item, not just this one. `null` for folders created before
	 * this existed, or if that instance has since been deleted. */
	owner_instance_id: string | null
}

/** Which of a shared profile's items to share — used by
 * `updateSharedProfileItems`. Keys line up 1:1 with `SharedProfile`'s
 * `share_*` fields. */
export interface SharedProfileItemFlags {
	share_saves: boolean
	share_config: boolean
	share_resourcepacks: boolean
	share_options: boolean
	share_servers: boolean
}

/** Every shared folder profile that exists, alphabetical by name. */
export async function listSharedProfiles(): Promise<SharedProfile[]> {
	return await invoke<SharedProfile[]>('plugin:shared-profile|shared_profile_list')
}

/** Creates a new, empty shared folder profile. Shares everything by default —
 * turn individual items off afterward with `updateSharedProfileItems`.
 * `ownerInstanceId`, if given, is recorded as "who created this" — pass the
 * instance you're creating it from, since that's always the one about to
 * join it immediately after. Doesn't change how joining/leaving behaves for
 * that instance; every member gets the same never-lose-data guarantee. */
export async function createSharedProfile(
	name: string,
	ownerInstanceId?: string,
): Promise<SharedProfile> {
	return await invoke<SharedProfile>('plugin:shared-profile|shared_profile_create', {
		name,
		ownerInstanceId: ownerInstanceId ?? null,
	})
}

export async function renameSharedProfile(id: string, newName: string): Promise<SharedProfile> {
	return await invoke<SharedProfile>('plugin:shared-profile|shared_profile_rename', {
		id,
		newName,
	})
}

/** Changes which items a shared folder actually shares (worlds, config,
 * resource packs, options.txt, and the server list), then re-syncs every
 * instance currently using it — instances that are running right now are
 * skipped and will pick up the change next time they're stopped and
 * restarted. Note: vanilla Minecraft stores keybinds inside options.txt
 * alongside video/sound/language settings, so there's no way to share worlds
 * without also sharing keybinds unless "Options" is turned off entirely. */
export async function updateSharedProfileItems(
	id: string,
	flags: SharedProfileItemFlags,
): Promise<SharedProfile> {
	return await invoke<SharedProfile>('plugin:shared-profile|shared_profile_update_items', {
		id,
		shareSaves: flags.share_saves,
		shareConfig: flags.share_config,
		shareResourcepacks: flags.share_resourcepacks,
		shareOptions: flags.share_options,
		shareServers: flags.share_servers,
	})
}

/** Deletes a shared folder profile — every instance still using it is moved
 * back to its own private copy of the data first, so nothing is lost. */
export async function deleteSharedProfile(id: string): Promise<void> {
	await invoke('plugin:shared-profile|shared_profile_delete', { id })
}

/** The shared profile this instance currently belongs to, if any. */
export async function getInstanceSharedProfile(instanceId: string): Promise<SharedProfile | null> {
	return await invoke<SharedProfile | null>('plugin:shared-profile|shared_profile_get_for_instance', {
		instanceId,
	})
}

/** Joins `instanceId` to `sharedProfileId`, or leaves whatever shared folder
 * it's currently in if `sharedProfileId` is `null`. Rejects while the
 * instance is running — stop it first. */
export async function setInstanceSharedProfile(
	instanceId: string,
	sharedProfileId: string | null,
): Promise<void> {
	await invoke('plugin:shared-profile|shared_profile_set_for_instance', {
		instanceId,
		sharedProfileId,
	})
}
