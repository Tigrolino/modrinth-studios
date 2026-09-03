# ![Modrinth Studios](/.github/assets/monorepo_cover.png)

## Modrinth Studios

This is a private fork of the [Modrinth](https://github.com/modrinth/code) monorepo. It's the same codebase as the real Modrinth app and website, plus a set of custom features layered on top for personal use and for a small group of friends. It isn't affiliated with Modrinth or Rinth, Inc., and it isn't public.

If you somehow ended up here and you're just looking for Modrinth, the real thing lives at [modrinth.com](https://modrinth.com) and you can download the official app [here](https://modrinth.com/app).

### What's actually different

The short version: same launcher, with a Replays tab, shared Minecraft folders between instances, a redesigned Storage page, custom Discord Rich Presence, playtime correction, and a full appearance system (custom accent colors, backgrounds, transparency, window icon). A couple of upstream bugs around scroll performance and modpack install speed got fixed along the way too.

The long version, including why each thing was built the way it was and every bug that came up fixing it, lives in [STUDIO.md](STUDIO.md). That file is the real changelog for this fork and is kept up to date every time something changes.

## Development

This repo is structured the same way upstream Modrinth's is. The two packages that matter for this fork:

- Desktop app: `apps/app` (Tauri/Rust) and `apps/app-frontend` (Vue). Run it with `pnpm app:dev` after copying `packages/app-lib/.env.prod` to `packages/app-lib/.env`.
- Website and API: `apps/frontend` and `apps/labrinth`, carried along from upstream and mostly untouched here.

Modrinth's own contributor docs for these still apply and are worth reading before touching anything: the [desktop app guide](https://docs.modrinth.com/contributing/theseus/) and the [website guide](https://docs.modrinth.com/contributing/knossos/).

Before adding or changing anything, read the "How this is structured" section at the top of [STUDIO.md](STUDIO.md) first. It explains how this fork stays mergeable with upstream and a gotcha with Tauri commands that has caused real bugs twice already.

## Pulling in upstream updates

`main` tracks real Modrinth (`origin`) untouched. `studio` is the branch with everything custom on it, and it's what actually gets built and released. To pull in a new Modrinth release:

```sh
git checkout main
git pull origin main
git checkout studio
git merge main
git push myfork studio
```

Then bump the version and tag a release, both covered in STUDIO.md's "Releasing an update" section.

## Security

This is a private fork run for personal use. If something looks like a real security issue in the parts of the code inherited from upstream Modrinth, it should go through Modrinth's own [responsible disclosure process](https://modrinth.com/legal/security), not this repo.

## Support

There's no support channel for this fork beyond whoever's running it. For anything related to actual Modrinth (the website, the official app, accounts, mods), use their [support page](https://support.modrinth.com) or [Discord](https://discord.modrinth.com).

## License and branding

Modrinth's code is source-available but not licensed for redistribution, and its branding (logo, cover images, and so on) can't be reused without permission. See [COPYING.md](COPYING.md) for the details. This fork stays private and is only ever shared directly with people who already know what it is, which is what keeps that a non-issue in practice. It's not published anywhere public and builds aren't distributed outside that.
