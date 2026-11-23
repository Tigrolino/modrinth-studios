import type { ClientKind } from '@/platform/app-lib/studio-clients'

export interface ClientMeta {
	kind: ClientKind
	name: string
	tagline: string
	/** Site whose icon is shown as the client's logo. */
	site: string
	/** Things to know before picking it. */
	notes: string[]
	/** Whether the instance page keeps the Modrinth content tab. */
	modrinthContent: boolean
	/** Short label for the risk level. */
	stability: 'Works well' | 'Experimental' | 'May break'
	/** Brand color for the client tab. */
	color: string
	/** Links shown on the client tab. */
	links: { label: string; url: string }[]
	/** Label of the version picker's second choice (module, pack, channel). */
	variantLabel: string | null
}

export const CLIENTS: ClientMeta[] = [
	{
		kind: 'lunar',
		name: 'Lunar Client',
		tagline: 'PvP client with built-in mods, from 1.8.9 to the newest versions.',
		site: 'lunarclient.com',
		notes: ["Unofficial launch via Lunar's own servers.", 'Pick Lunar + Fabric or Forge to add mods.'],
		modrinthContent: false,
		stability: 'Experimental',
		color: '#6a5cff',
		links: [
			{ label: 'Cosmetics store', url: 'https://store.lunarclient.com' },
			{ label: 'Lunar+', url: 'https://store.lunarclient.com/category/plus/' },
			{ label: 'Status', url: 'https://status.lunarclient.com' },
		],
		variantLabel: 'Module',
	},
	{
		kind: 'labymod',
		name: 'LabyMod',
		tagline: 'LabyMod 4 with its addons, cosmetics and menus.',
		site: 'labymod.net',
		notes: ['Unofficial, may break on some versions.', "Addons come from LabyMod's in-game store."],
		modrinthContent: false,
		stability: 'May break',
		color: '#2d8cf0',
		links: [
			{ label: 'Shop', url: 'https://www.labymod.net/en/shop' },
			{ label: 'Blog', url: 'https://www.labymod.net/en/blog' },
			{ label: 'Support', url: 'https://www.labymod.net/en/support' },
		],
		variantLabel: 'Channel',
	},
	{
		kind: 'norisk',
		name: 'NoRisk Client',
		tagline: 'Fabric-based client with cosmetics, replays and friends.',
		site: 'norisk.gg',
		notes: ['Signs in with your Minecraft account.', 'Fabric mods from Modrinth work too.'],
		modrinthContent: true,
		stability: 'Experimental',
		color: '#e8304a',
		links: [
			{ label: 'Blog', url: 'https://blog.norisk.gg' },
			{ label: 'Website', url: 'https://norisk.gg' },
		],
		variantLabel: 'Pack',
	},
	{
		kind: 'horizon',
		name: 'Horizon',
		tagline: 'Minecraft 26.3 rewritten in Rust, with its own mods and shaders.',
		site: 'playhorizon.dev',
		notes: ['Minecraft 26.3 only, no Java mods.', 'First launch takes a few minutes.'],
		modrinthContent: false,
		stability: 'Experimental',
		color: '#8b5cf6',
		links: [
			{ label: 'Community hub', url: 'https://playhorizon.dev/hub' },
			{ label: 'Docs', url: 'https://playhorizon.dev/docs' },
			{ label: 'Mods', url: 'https://playhorizon.dev/mods' },
			{ label: 'GitHub', url: 'https://github.com/horizon-rust/horizon' },
		],
		variantLabel: null,
	},
]

export function clientMeta(kind: ClientKind | null | undefined) {
	return CLIENTS.find((c) => c.kind === kind) ?? null
}

const GITHUB_ORG: Record<ClientKind, string> = {
	lunar: 'LunarClient',
	labymod: 'LabyMod',
	norisk: 'NoRiskClient',
	horizon: 'horizon-rust',
}

/** The client's logo (its GitHub organisation avatar, crisp at any size). */
export function clientLogo(meta: ClientMeta | ClientKind) {
	const kind = typeof meta === 'string' ? meta : meta.kind
	return kind === 'norisk'
		? 'https://norisk.gg/icon-192.png'
		: `https://github.com/${GITHUB_ORG[kind]}.png?size=256`
}
