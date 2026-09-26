# Studio

This is a personal fork of the [Modrinth App](https://modrinth.com/app) with a handful of extra features bolted on. It's not affiliated with Modrinth or Rinth, Inc.

This file covers what's actually different from stock Modrinth App and how the repo is put together. It's meant to stay short. For the blow by blow of every bug fixed and dead end tried along the way, that history is kept locally and isn't part of this repo.

## How this repo is organized

There are two branches that matter:

- `main` tracks upstream Modrinth exactly, no changes.
- `studio` has everything custom on it. This is the branch that gets built and released.

To pull in upstream changes:

```sh
git checkout main
git pull origin main
git checkout studio
git merge main
```

Conflicts happen most often in the sidebar, settings pages, and instance/pack logic, since those are the areas Studio touches the most. Resolve them file by file and rebuild before pushing.

**One gotcha worth knowing up front:** Tauri commands need to be registered in three separate places, the Rust `#[tauri::command]` function, the `generate_handler!` macro list, and the frontend's TypeScript bindings. Missing one of the three usually shows up as a silent "command not found" at runtime rather than a build error, so if a new command isn't working, check all three spots before assuming the logic itself is wrong.

## What's different from stock Modrinth App

### Replays tab

A new sidebar tab for browsing ReplayMod and Flashback recordings across all instances. It scans instance folders for replay files, shows metadata (map, date, players, duration), and supports sorting, grouping, and bulk delete. Metadata is cached so the tab loads fast on repeat visits instead of rescanning the filesystem every time.

### Storage page

Rebuilt from the ground up. Shows real disk usage per instance (worlds, mods, resource packs, etc.) instead of just a folder size, and instance renames no longer break anything downstream that was keyed off the old folder name.

### Multiple instances at once

Stock Modrinth App expects one instance running at a time. Studio allows launching several instances simultaneously without them stepping on each other's state.

### Playtime correction

Fixes cases where playtime tracking could overcount or undercount, mostly around instances that crash or get force closed mid session.

### Appearance

A full theming layer on top of the stock light/dark toggle:

- Custom accent colors
- Custom background images
- Adjustable transparency and blur on UI surfaces
- Custom window/taskbar icons
- Splash screen customization
- A theme lock option, so a chosen look doesn't get reset by app updates

### Discord Rich Presence

Customizable presence text and images instead of the fixed upstream format, so what shows up on Discord while playing can actually reflect what you're doing.

### Upstream version shown in Settings

Settings shows which upstream Modrinth App version this build is based on, so it's easy to tell how far behind (or caught up) Studio is at a glance.

### Smaller fixes

A number of quality of life fixes have gone in over time too: dismissible in app banners, resilience around instance renames so nothing silently breaks, a fix for a database migration conflict that could occur if both Studio and the official Modrinth app were installed on the same machine, and a fix for a misleading error dialog that showed up in situations that weren't actually errors.

Not everything tried made the cut. A shared Minecraft folders feature (letting instances share one set of mod/resource files instead of duplicating them) was built and then removed after turning out to be too easy to misuse in ways that could corrupt a world or save file. A few UI experiments (drag to reorder instances, a window restore animation) were also reverted after they caused more problems than they solved. None of that is currently in the app.

## Releasing an update

1. Bump the version in the relevant `Cargo.toml` / `package.json` files.
2. Commit and tag the release (`vX.Y.Z`).
3. Push the tag, which kicks off the GitHub Actions build for Windows, macOS, and Linux.
4. Once the build finishes, publish the draft release it creates.

Windows installs use NSIS in passive mode (a visible but non interactive progress bar) rather than fully silent, after silent installs turned out to cause confusing "is this frozen?" moments for people who couldn't tell an update was happening at all.

**Important:** if this repository ever goes private, both the in app auto updater and manual downloads for anyone without a GitHub login will stop working until it's public again.

Releases are signed. That needs two secrets set in the repo: `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

## Licensing note

Everything inherited from Modrinth keeps its original license (see the README for the breakdown by package). Anything added or changed in this fork is licensed the same way as the code it touches. Modrinth's branding and assets aren't covered by any of that and aren't reused here.
