# Modrinth Studios

Modrinth Studios is a fork of the [Modrinth App](https://modrinth.com/app), the Minecraft launcher for mods and modpacks. It keeps everything the official app does and adds a few bigger things on top:

- Manage your own servers from a Pterodactyl panel in the Hosting tab, with the same console, file manager, backups and mod browser that Modrinth Hosting servers get. You don't need a Modrinth account for this.
- See analytics for your servers, both Modrinth Hosting and Pterodactyl: live usage, who played and when, and stats from the world itself.
- Play mode, if you mostly play one instance. The Play button takes you straight to it, and your library gets its own button.
- A Replays tab for ReplayMod and Flashback recordings, a better Storage page, running more than one instance at a time, playtime correction, custom Discord status, and lots of options for how the app looks.

[STUDIO.md](STUDIO.md) goes through all of it in more detail.

This project isn't affiliated with Modrinth or Rinth, Inc. If you want the official app, get it from [modrinth.com/app](https://modrinth.com/app).

## Download

Grab the latest installer from the [releases page](https://github.com/Tigrolino/modrinth-studios/releases). Only Windows is built at the moment. The app updates itself when a new version is out.

It can be installed next to the official Modrinth App without problems.

## AI notice

This fork was coded with AI. I've reviewed every function and made sure I understand and
approve of the changes, but there may still be issues I haven't discovered - if you come across
anything that seems incorrect or broken, please report it to me.

I also want to be transparent about my views on AI. I don't fully support its use in every context, and I believe there are areas where it shouldn't be used, such as the creation of images or videos. However, I found that adding these features made the Modrinth launcher much more useful to me, which is why I decided to make this fork. I wanted to be upfront about how AI was used in the project and provide some context for why I chose to use it.

## Building it yourself

```sh
pnpm install
cp packages/app-lib/.env.prod packages/app-lib/.env
pnpm app:dev
```

This starts a dev build with hot reload. Modrinth's [contributor guide](https://docs.modrinth.com/contributing/theseus/) for the app still applies to everything inherited from upstream.

The repo only contains the desktop app and the packages it needs:

- `apps/app` is the Tauri/Rust side
- `apps/app-frontend` is the Vue UI
- `packages/app-lib` is the launcher logic (instances, downloads, accounts)
- `packages/ui`, `packages/assets`, `packages/utils` and `packages/api-client` are shared frontend packages
- `packages/daedalus`, `packages/ariadne` and a few small Rust crates are supporting libraries

## License

Modrinth's code is split across packages, each under its own license: GPL-3.0 for `apps/app`, `apps/app-frontend`, `packages/ui`, `packages/assets` and `packages/utils`, and LGPL-3.0 for `packages/api-client`. Check the `LICENSE` file in each package for details. Changes made in this fork use the same license as the code they're in.

Modrinth's branding (the logo, cover images and so on) isn't covered by those licenses and can't be reused without permission from Rinth, Inc. [COPYING.md](COPYING.md) lists what that includes. This fork doesn't ship any of it.

## Security

Security issues in code that comes from Modrinth should go through their [disclosure process](https://modrinth.com/legal/security). This fork has no servers of its own. The only extra sensitive data it stores is your Pterodactyl API key, which stays on your computer and is only sent to your own panel.

## Support

For problems with this fork, open an issue here. For anything about Modrinth itself (the website, the official app, your account), use their [support page](https://support.modrinth.com) or [Discord](https://discord.modrinth.com).
