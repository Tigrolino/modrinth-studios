# Modrinth Studios

Modrinth Studios is a fork of the [Modrinth App](https://modrinth.com/app), the official Minecraft mod and modpack launcher, with some extra features on top: a Replays tab for ReplayMod and Flashback recordings, a rebuilt Storage page, custom Discord Rich Presence, playtime correction, and an appearance system with accent colors, custom backgrounds, transparency, and window icons. It started as a personal project and grew from there.

This isn't affiliated with Modrinth or Rinth, Inc. in any way. If you're looking for the real thing, it's at [modrinth.com](https://modrinth.com), and the official app download is [here](https://modrinth.com/app).

## AI notice

This fork was coded with AI. I've reviewed every function and made sure I understand and
approve of the changes, but there may still be issues I haven't discovered - if you come across
anything that seems incorrect or broken, please report it to me.

I also want to be transparent about my views on AI. I don't fully support its use in every context, and I believe there are areas where it shouldn't be used, such as the creation of images or videos. However, I found that adding these QoL features made the Modrinth launcher much more useful to me, which is why I decided to make this fork. I wanted to be upfront about how AI was used in the project and provide some context for why I chose to use it.

## What's different from upstream

The list above covers the highlights, but [STUDIO.md](STUDIO.md) goes into more detail on each one and is kept up to date as things change.

## Repository layout

This started as a full clone of Modrinth's monorepo, which also has the website, the backend API, and various internal tools in it. None of that is needed to build the desktop app, so it's been trimmed down to just the app and the packages it depends on:

- `apps/app`: the Tauri/Rust shell
- `apps/app-frontend`: the Vue UI
- `packages/app-lib`: the core launcher logic (instance management, mod downloads, auth, etc.)
- `packages/ui`, `packages/assets`, `packages/utils`, `packages/api-client`: shared frontend packages
- `packages/daedalus`, `packages/ariadne`, and a few smaller Rust crates: supporting libraries the app depends on

## Development

```sh
pnpm install
cp packages/app-lib/.env.prod packages/app-lib/.env
pnpm app:dev
```

That gets you a dev build with hot reload. Modrinth's contributor guide for the desktop app is still accurate for the parts of the codebase inherited from upstream, and worth reading before making changes: [docs.modrinth.com/contributing/theseus](https://docs.modrinth.com/contributing/theseus/).

Before changing anything, take a look at the "How this repo is organized" section in [STUDIO.md](STUDIO.md). It covers how this fork stays mergeable with upstream, plus a Tauri command registration gotcha that's caused real bugs more than once.

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

Modrinth's code is split across packages, each under its own license: GPL-3.0 for `apps/app`, `apps/app-frontend`, `packages/ui`, `packages/assets`, and `packages/utils`; LGPL-3.0 for `packages/api-client`. Check the `LICENSE` file in a given package for specifics. Everything changed or added in this fork stays under the same terms as the code it touches.

Modrinth's branding (the wrench-in-labyrinth logo, cover images, and so on) isn't covered by that and can't be reused without permission from Rinth, Inc. See [COPYING.md](COPYING.md) for the full list of what that covers; this fork doesn't ship any of it.

## Security

If you find a security issue in code inherited from upstream Modrinth, report it through Modrinth's own [disclosure process](https://modrinth.com/legal/security) rather than here. This fork doesn't run any of its own backend services or handle user data beyond what the official app already does.

## Support

There's no formal support channel for this fork. For anything about the actual Modrinth platform (the website, the official app, accounts), use their [support page](https://support.modrinth.com) or [Discord](https://discord.modrinth.com).
