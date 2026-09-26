# Modrinth Studios

Modrinth Studios is a fork of the [Modrinth App](https://modrinth.com/app), the official Minecraft mod/modpack launcher, with a set of extra features layered on top: a Replays tab for ReplayMod and Flashback recordings, shared Minecraft folders between instances, a rewritten Storage page, custom Discord Rich Presence, playtime correction, and a full appearance system covering accent colors, custom backgrounds, transparency, and window icons. It started as a personal project and grew from there.

This isn't affiliated with Modrinth or Rinth, Inc. in any way. If you're looking for the real thing, it's at [modrinth.com](https://modrinth.com), and the official app download is [here](https://modrinth.com/app).

## What's different from upstream

The features above are the headline changes, but there's more detail than fits in a README: why each one was built the way it was, and the bugs that came up along the way. All of that lives in [STUDIO.md](STUDIO.md), which gets updated whenever something changes and is the closest thing this fork has to a real changelog.

## Repository layout

This started as a full clone of Modrinth's monorepo, which also contains the website, the backend API, and several internal tools. None of that is needed to build the desktop app, so it's been trimmed out — what's left is just the app itself and the packages it actually depends on:

- `apps/app` — the Tauri/Rust shell
- `apps/app-frontend` — the Vue UI
- `packages/app-lib` — the core launcher logic (instance management, mod downloads, auth, etc.)
- `packages/ui`, `packages/assets`, `packages/utils`, `packages/api-client` — shared frontend packages
- `packages/daedalus`, `packages/ariadne`, and a few smaller Rust crates — supporting libraries the app depends on

## Development

```sh
pnpm install
cp packages/app-lib/.env.prod packages/app-lib/.env
pnpm app:dev
```

That gets you a dev build with hot reload. Modrinth's own contributor guide for the desktop app is still accurate for the parts of the codebase inherited from upstream, and worth a read before making changes: [docs.modrinth.com/contributing/theseus](https://docs.modrinth.com/contributing/theseus/).

Before touching anything, read the "How this is structured" section at the top of [STUDIO.md](STUDIO.md) — it explains how this fork stays mergeable with upstream, and a Tauri command registration gotcha that's caused real bugs more than once.

## Pulling in upstream updates

`main` tracks real Modrinth (`origin`) untouched. `studio` is the branch with everything custom on it, and it's what actually gets built and released.

```sh
git checkout main
git pull origin main
git checkout studio
git merge main
```

STUDIO.md's "Releasing an update" section covers bumping the version and tagging a release after that.

## License

Modrinth's code is split across packages, each under its own license — GPL-3.0 for `apps/app`, `apps/app-frontend`, `packages/ui`, `packages/assets`, and `packages/utils`; LGPL-3.0 for `packages/api-client`. Check the `LICENSE` file in a given package for specifics. Everything changed or added in this fork stays under the same terms as the code it touches.

Modrinth's branding — the wrench-in-labyrinth logo, cover images, and so on — isn't covered by that and can't be reused without permission from Rinth, Inc. See [COPYING.md](COPYING.md) for the full list of what that covers; this fork doesn't ship any of it.

## Security

If you find a security issue in code inherited from upstream Modrinth, report it through Modrinth's own [disclosure process](https://modrinth.com/legal/security) rather than here. This fork doesn't run any of its own backend services or handle user data beyond what the official app already does.

## Support

There's no formal support channel for this fork. For anything about the actual Modrinth platform — the website, the official app, accounts — use their [support page](https://support.modrinth.com) or [Discord](https://discord.modrinth.com).
