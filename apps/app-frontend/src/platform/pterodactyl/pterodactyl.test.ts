import { describe, expect, it } from 'vitest'

import { compatibleLoaders, defaultKind, kindDirectory, manifestKey } from './content'
import { normalizePath, splitPath } from './files'
import {
	findPteroServerId,
	isPteroNodeUrl,
	isPteroServerId,
	makePteroServerId,
	parsePteroServerId,
	PTERO_FAKE_JWT,
	PTERO_WORLD_ID,
	pteroNodeHost,
	resolvePteroDownloadUrl,
} from './ids'
import {
	archonLoader,
	contentDirectory,
	detectFlavor,
	javaVersionFromImage,
	modrinthLoader,
	primaryAllocation,
	type PteroFlavor,
	type PteroServerAttributes,
	type PteroServerInfo,
	toArchonServerV0,
	toArchonServerV1,
} from './server'

const ALL_FLAVORS: PteroFlavor[] = [
	'paper',
	'purpur',
	'spigot',
	'proxy',
	'fabric',
	'quilt',
	'forge',
	'neoforge',
	'vanilla',
	'pumpkin',
	'unknown',
]

function attributes(overrides: Partial<PteroServerAttributes> = {}): PteroServerAttributes {
	return {
		server_owner: true,
		identifier: 'a1b2c3d4',
		uuid: 'a1b2c3d4-0000-0000-0000-000000000000',
		name: 'Survival',
		node: 'node-1',
		sftp_details: { ip: 'sftp.example.com', port: 2022 },
		description: '',
		limits: { memory: 4096, swap: 0, disk: 10240, io: 500, cpu: 200 },
		invocation: 'java -Xms128M -jar server.jar',
		docker_image: 'ghcr.io/pterodactyl/yolks:java_21',
		egg_features: null,
		feature_limits: { databases: 0, allocations: 2, backups: 3 },
		status: null,
		is_suspended: false,
		is_installing: false,
		is_transferring: false,
		relationships: {
			allocations: {
				data: [
					{
						attributes: {
							id: 1,
							ip: '10.0.0.1',
							ip_alias: null,
							port: 25570,
							notes: null,
							is_default: false,
						},
					},
					{
						attributes: {
							id: 2,
							ip: '10.0.0.2',
							ip_alias: 'play.example.com',
							port: 25565,
							notes: null,
							is_default: true,
						},
					},
				],
			},
			variables: { data: [] },
			egg: { attributes: { uuid: 'egg', name: 'Paper' } },
		},
		...overrides,
	}
}

function info(overrides: Partial<PteroServerInfo> = {}): PteroServerInfo {
	const ref = parsePteroServerId(makePteroServerId('panel1', 'a1b2c3d4'))
	return {
		ref,
		panel: { id: 'panel1', name: 'My Panel', url: 'https://panel.example.com' },
		attributes: attributes(),
		username: 'steve',
		flavor: 'paper',
		eggFlavor: 'paper',
		settings: {} as PteroServerInfo['settings'],
		mcVersion: '1.21.11',
		loaderVersion: '120',
		variables: {},
		...overrides,
	}
}

describe('Pterodactyl server ids', () => {
	it('round-trips panel and identifier through the synthetic id', () => {
		const id = makePteroServerId('panel1', 'a1b2c3d4')
		expect(id).toBe('ptero-panel1-a1b2c3d4')
		expect(isPteroServerId(id)).toBe(true)
		expect(parsePteroServerId(id)).toEqual({
			serverId: id,
			panelId: 'panel1',
			identifier: 'a1b2c3d4',
		})
	})

	it('rejects ids that are not Pterodactyl ones', () => {
		for (const id of [null, undefined, '', 'ptero-', 'ptero-a', 'ptero-a-b-c', 'abc-123', 'x ptero-a-b']) {
			expect(isPteroServerId(id), String(id)).toBe(false)
		}
	})

	it('finds the server id inside request URLs', () => {
		expect(findPteroServerId('https://api.modrinth.com/v0/servers/ptero-p1-abc123/fs')).toBe(
			'ptero-p1-abc123',
		)
		expect(findPteroServerId('https://api.modrinth.com/v0/servers/123/fs')).toBeNull()
	})

	it('builds fake node hosts the request router recognises', () => {
		const host = pteroNodeHost('ptero-p1-abc')
		expect(isPteroNodeUrl(`https://${host}/download`)).toBe(true)
		expect(isPteroNodeUrl('https://node.modrinth.com/download')).toBe(false)
	})

	it('pulls the signed Wings URL back out of a fake node download URL', () => {
		const wings = 'https://wings.example.com/download/file?token=abc'
		const fake = `https://${pteroNodeHost('ptero-p1-abc')}/dl?token=${encodeURIComponent(wings)}`
		expect(resolvePteroDownloadUrl(fake)).toBe(wings)
		expect(resolvePteroDownloadUrl('https://cdn.modrinth.com/file.jar')).toBe(
			'https://cdn.modrinth.com/file.jar',
		)
	})

	it('uses a JWT-shaped token that never expires', () => {
		const parts = PTERO_FAKE_JWT.split('.')
		expect(parts).toHaveLength(3)
		const payload = JSON.parse(atob(parts[1].replace(/-/g, '+').replace(/_/g, '/')))
		expect(payload.exp * 1000).toBeGreaterThan(Date.now())
		expect(PTERO_FAKE_JWT).not.toMatch(/[+/=]/)
	})
})

describe('Pterodactyl file paths', () => {
	it('normalizes slashes, empty parts and missing paths', () => {
		expect(normalizePath(null)).toBe('/')
		expect(normalizePath('')).toBe('/')
		expect(normalizePath('mods//a.jar')).toBe('/mods/a.jar')
		expect(normalizePath('\\world\\datapacks\\')).toBe('/world/datapacks')
	})

	it('splits a path into folder and name', () => {
		expect(splitPath('/mods/a.jar')).toEqual({ dir: '/mods', name: 'a.jar' })
		expect(splitPath('server.properties')).toEqual({ dir: '/', name: 'server.properties' })
		expect(splitPath('/a/b/c.txt')).toEqual({ dir: '/a/b', name: 'c.txt' })
	})
})

describe('Pterodactyl server flavors', () => {
	const flavorOf = (egg: string, extra: Partial<PteroServerAttributes> = {}) =>
		detectFlavor(
			attributes({
				docker_image: 'ghcr.io/pterodactyl/yolks:java_21',
				invocation: 'java -jar server.jar',
				relationships: { egg: { attributes: { uuid: 'e', name: egg } } },
				...extra,
			}),
		)

	it.each([
		['Paper', 'paper'],
		['Folia', 'paper'],
		['Purpur', 'purpur'],
		['Spigot', 'spigot'],
		['Bukkit', 'spigot'],
		['Velocity', 'proxy'],
		['BungeeCord', 'proxy'],
		['Waterfall', 'proxy'],
		['Fabric', 'fabric'],
		['Quilt', 'quilt'],
		['Forge Enhanced', 'forge'],
		['NeoForge', 'neoforge'],
		['Vanilla Minecraft', 'vanilla'],
		['Pumpkin', 'pumpkin'],
		['Something else', 'unknown'],
	] as const)('egg "%s" is %s', (egg, flavor) => {
		expect(flavorOf(egg)).toBe(flavor)
	})

	it('checks NeoForge before Forge (its name contains "forge")', () => {
		expect(flavorOf('Generic', { invocation: 'java @libraries/net/neoforged/neoforge/args.txt' })).toBe(
			'neoforge',
		)
	})

	it('maps every flavor onto a loader, Modrinth loader and content folder', () => {
		for (const flavor of ALL_FLAVORS) {
			expect(archonLoader(flavor)).toBeTruthy()
			expect(modrinthLoader(flavor)).toBeTruthy()
		}
		expect(contentDirectory('fabric')).toBe('/mods')
		expect(contentDirectory('neoforge')).toBe('/mods')
		expect(contentDirectory('paper')).toBe('/plugins')
		expect(contentDirectory('proxy')).toBe('/plugins')
		expect(contentDirectory('vanilla')).toBeNull()
		expect(modrinthLoader('proxy')).toBe('velocity')
		expect(modrinthLoader('unknown')).toBe('vanilla')
		expect(archonLoader('neoforge')).toBe('NeoForge')
	})

	it('reads the Java version from the Docker image', () => {
		expect(javaVersionFromImage('ghcr.io/pterodactyl/yolks:java_21')).toBe(21)
		expect(javaVersionFromImage('ghcr.io/parkervcp/yolks:java_8')).toBe(8)
		expect(javaVersionFromImage('eclipse-temurin:jre17')).toBe(17)
		expect(javaVersionFromImage('ghcr.io/pterodactyl/yolks:debian')).toBeNull()
	})
})

describe('Pterodactyl content helpers', () => {
	it('puts each kind of content in the right folder', () => {
		expect(kindDirectory('fabric', 'mod')).toBe('/mods')
		expect(kindDirectory('paper', 'plugin')).toBe('/plugins')
		expect(kindDirectory('paper', 'datapack')).toBe('/world/datapacks')
		expect(kindDirectory('vanilla', 'mod')).toBeNull()
	})

	it('offers mods, plugins or datapacks depending on the server', () => {
		expect(defaultKind('forge')).toBe('mod')
		expect(defaultKind('purpur')).toBe('plugin')
		expect(defaultKind('vanilla')).toBe('datapack')
	})

	it('treats a disabled file as the same manifest entry', () => {
		expect(manifestKey('mod', 'sodium.jar.disabled')).toBe(manifestKey('mod', 'sodium.jar'))
		expect(manifestKey('mod', 'a.jar')).not.toBe(manifestKey('plugin', 'a.jar'))
	})

	it('asks Modrinth for compatible loaders in order of preference', () => {
		expect(compatibleLoaders('purpur', 'plugin')).toEqual(['purpur', 'paper', 'spigot', 'bukkit'])
		expect(compatibleLoaders('quilt', 'mod')).toEqual(['quilt', 'fabric'])
		expect(compatibleLoaders('fabric', 'datapack')).toEqual(['datapack'])
		expect(compatibleLoaders('vanilla', 'mod')).toEqual([])
	})
})

describe('Pterodactyl servers as Modrinth hosting servers', () => {
	it('uses the default allocation (and its alias) for the address', () => {
		const server = toArchonServerV0(info())
		expect(primaryAllocation(info())?.port).toBe(25565)
		expect(server.net.ip).toBe('play.example.com')
		expect(server.net.port).toBe(25565)
	})

	it('falls back to the first allocation, then to the default port', () => {
		const noDefault = info({
			attributes: attributes({
				relationships: {
					allocations: {
						data: [
							{
								attributes: {
									id: 1,
									ip: '10.0.0.9',
									ip_alias: null,
									port: 25599,
									notes: null,
									is_default: false,
								},
							},
						],
					},
				},
			}),
		})
		expect(toArchonServerV0(noDefault).net).toMatchObject({ ip: '10.0.0.9', port: 25599 })
		const none = info({ attributes: attributes({ relationships: {} }) })
		expect(toArchonServerV0(none).net).toMatchObject({ ip: null, port: 25565 })
	})

	it('maps status, SFTP login and limits', () => {
		const server = toArchonServerV0(info())
		expect(server.status).toBe('available')
		expect(server.sftp_username).toBe('steve.a1b2c3d4')
		expect(server.sftp_host).toBe('sftp.example.com:2022')
		expect(server.backup_quota).toBe(3)
		expect(server.studio_ptero).toMatchObject({ flavor: 'paper', allocation_limit: 2 })
		expect(toArchonServerV0(info({ attributes: attributes({ is_installing: true }) })).status).toBe(
			'installing',
		)
		const suspended = toArchonServerV0(info({ attributes: attributes({ is_suspended: true }) }))
		expect(suspended.status).toBe('suspended')
		expect(suspended.suspension_reason).toBe('other')
		expect(toArchonServerV0(info({ username: '' })).sftp_username).toBe('a1b2c3d4')
	})

	it('builds the v1 server with one world and the right specs', () => {
		const server = toArchonServerV1(info())
		expect(server.id).toBe('ptero-panel1-a1b2c3d4')
		expect(server.specs).toEqual({ cpu: 2, memory_mb: 4096, storage_mb: 10240, swap_mb: 0 })
		expect(server.worlds).toHaveLength(1)
		expect(server.worlds[0].id).toBe(PTERO_WORLD_ID)
		expect(server.worlds[0].content).toMatchObject({
			modloader: 'paper',
			game_version: '1.21.11',
			java_version: 21,
		})
		expect(isPteroNodeUrl(`https://${server.location.location_metadata.url_host}`)).toBe(true)
	})

	it('reports unlimited CPU as 0 and rounds partial cores up to one', () => {
		expect(toArchonServerV1(info({ attributes: attributes({ limits: { memory: 1, swap: 0, disk: 1, io: 1, cpu: 0 } }) })).specs.cpu).toBe(0)
		expect(toArchonServerV1(info({ attributes: attributes({ limits: { memory: 1, swap: 0, disk: 1, io: 1, cpu: 30 } }) })).specs.cpu).toBe(1)
	})
})
