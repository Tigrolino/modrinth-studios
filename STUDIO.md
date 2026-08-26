# Modrinth Studios

This is a private fork of the [Modrinth App](https://github.com/modrinth/code) with a few
custom features layered on top. This file documents what was changed, how updates work, and
how to pull in Modrinth's upstream changes later without losing any of it.

## How this is structured

- `main` tracks upstream `modrinth/code` (remote `origin`) untouched.
- `studio` is where all of our changes live, and is what actually gets built/released.
- Everything custom is either a brand-new file (safest — never conflicts with upstream) or a
  small, clearly-commented block inside an existing file, marked with `Modrinth Studios addition`
  or `Modrinth Studios` in a comment. Search for that string to find every touch point.

### Pulling in a Modrinth update later

```sh
git checkout main
git pull origin main
git checkout studio
git merge main   # resolve any conflicts — should mostly be none, given the above
git push myfork studio
```

Then tag + push a release (see below) so everyone's app picks it up.

## What's different from stock Modrinth App

- **Sidebar**: closed by default; hover the button in the top-right corner (or the thin strip
  along the right edge of the window when it's closed) to reveal the show/hide toggle. This
  reuses Modrinth's own "hide right sidebar" behavior setting, just defaulted on
  (`packages/app-lib/migrations/20260825190000_studio-sidebar-closed-default.sql`).
- **Sidebar ad + "Upgrade to Modrinth+"**: hidden, not deleted. Flip `STUDIO_HIDE_SIDEBAR_PROMO`
  in `apps/app-frontend/src/App.vue` back to `false` to restore it.
- **Replays tab**: new tab on each instance, shown only if it has a `replay_recordings/`
  (ReplayMod) or `flashback/replays/` (Flashback) folder. Lists recordings with whatever
  metadata we could read out of them, and lets you launch the instance, rename/delete a
  recording, open its folder, or add an existing replay file.
  - Neither mod publishes a versioned file format, so metadata parsing
    (`packages/app-lib/src/api/replays.rs`) is deliberately lenient and falls back to filename
    and file timestamps for anything it can't read.
  - Neither mod exposes a way to jump straight into a specific replay — "Launch" starts the
    instance the normal way; open the replay from the mod's own in-game UI from there.
- **Appearance settings** (Settings → Appearance, below Modrinth's own section): custom accent
  color, an image or gradient background, a popup/modal background opacity slider, and a
  custom window/taskbar icon. All implemented as runtime CSS variable overrides
  (`apps/app-frontend/src/composables/use-studio-appearance.ts` +
  `apps/app-frontend/src/assets/styles/studio-overrides.css`) — nothing in the shared
  `packages/ui` / `packages/assets` theme files was touched.
  - The window icon change only affects the *running* app (title bar/taskbar while it's open).
    Windows' own icon for the installed .exe/shortcut (what Explorer shows before you open it)
    can only be changed by rebuilding with new files under `apps/app/icons/` — see
    [Tauri's icon docs](https://v2.tauri.app/reference/cli/#icon) (`pnpm tauri icon <image>`).
  - When a custom background is active, its "background transparency" slider makes a broad set
    of panels/buttons/cards see-through (`SURFACE_PROPERTIES` in use-studio-appearance.ts) and
    pairs most of them with a `backdrop-filter` blur for a frosted-glass look
    (`studio-overrides.css`). The blur is deliberately **not** applied to `ContentCardTable`'s
    rows (the mod/resource-pack/shader/datapack list) — that list can run into the hundreds of
    rows, and blurring every one of them at once overwhelmed the webview's compositor (corrupted
    color blocks, heavy flashing while scrolling). Those rows stay plain see-through instead. If
    a similar long, densely-repeated list gets this treatment in the future, give it the same
    exception rather than adding it to the blur selector list in `studio-overrides.css`.
  - "Keep update buttons green" (only shown once an accent color is set) opts a color back out
    of the accent recolor — `--color-green` is shared by a lot of unrelated "positive" UI
    (update buttons, the Beta tag, ping indicators), so this is a blanket "leave green stuff
    green" switch, not something scoped to literally just update buttons.

## Releasing an update

1. Bump `version` in `apps/app-frontend/package.json` (that's what
   `apps/app/tauri.conf.json`'s own `"version": "../app-frontend/package.json"` points at — it's
   a path reference, not a literal version, so this is the one place to actually change) and
   commit. **Use plain semver** (`0.1.3`, not `1.0.0-local` or anything with a suffix) — the
   in-app updater compares this against the latest release with normal semver rules, so a
   non-numeric or otherwise out-of-order version can make it think there's nothing newer to
   update to, forever.
2. `git tag studio-v0.1.2 && git push myfork studio-v0.1.2` (match the number you just set, bump
   each time).
3. `.github/workflows/studio-release.yml` builds the Windows app, signs it with our updater key,
   and publishes a GitHub release with the files the in-app updater expects.
4. Everyone running the app gets the update prompt automatically (Modrinth's built-in updater,
   just pointed at `speedzing/modrinth-studios` releases instead of Modrinth's own servers).

### One-time repo setup for releases

Add these two **repository secrets** on GitHub (Settings → Secrets and variables → Actions):

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (blank/empty value — the key has no password)

The keypair itself was generated during setup and given to you separately (not committed to
git, on purpose — anyone with the private key could sign fake "updates" for everyone running
the app). If it's ever lost, generate a new one with `pnpm tauri signer generate`, update
`plugins.updater.pubkey` in `apps/app/tauri-release.conf.json` with the new public key, and
re-save the two secrets above.

There's no real code-signing certificate here (Modrinth's own builds use a paid DigiCert cert
we don't have access to), so Windows SmartScreen will show an "unknown publisher" warning on
install. That's expected for a private, unlisted build — just click through it.

## Licensing note

Modrinth App is source-available but not permissively licensed for redistribution (see
`COPYING.md`/`LICENSE` in this repo). Since this fork stays private and is only ever shared
directly with people you know, that's a non-issue in practice — just don't make the repo public
or distribute builds beyond your friend group.
