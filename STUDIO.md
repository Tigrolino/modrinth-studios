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
- **Adding a new Tauri command/plugin? Three places, not one.** A `#[tauri::command]` plus
  `.plugin(api::x::init())` in `apps/app/src/main.rs` is not enough on its own — Tauri v2 blocks
  any command that isn't also explicitly allowlisted, and it fails at runtime ("not allowed,
  plugin not found"), not at compile time, so it's easy to ship without noticing:
  1. `apps/app/build.rs` — add a `.plugin("name", InlinedPlugin::new().commands(&[...]).default_permission(DefaultPermissionRule::AllowAllCommands))` block listing every command in that plugin (this is what generates the `name:default` permission).
  2. `apps/app/capabilities/plugins.json` — add `"name:default"` to the `permissions` array.
  3. The plugin name must be the exact same string in all three places: `tauri::plugin::Builder::new("name")` in the command file, the `build.rs` identifier, and the `capabilities/plugins.json` entry — and it's also the prefix the frontend calls via `invoke('plugin:name|command', ...)`.
  This has already bitten this fork twice (the replay thumbnail command, then the whole
  playtime-correction plugin) — both looked done after step 1 and only failed once actually
  clicked in the running app.

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

- **Sidebar**: closed by default; hover the small tab that peeks out from the right edge of the
  window (`.sidebar-edge-tab` in App.vue) to reveal it, then click to open/close. This reuses
  Modrinth's own "hide right sidebar" behavior setting, just defaulted on
  (`packages/app-lib/migrations/20260825190000_studio-sidebar-closed-default.sql`).
  - The sidebar still **pushes** content (a grid column growing 0 → 300px), it doesn't overlay it.
    The push itself was always smooth — the actual "jiggle" was pre-existing (it also happened on a
    plain window drag-resize, nothing to do with the sidebar specifically) and had nothing to do
    with Vue at all in the end (two earlier attempts at this — suppressing the instance grid's
    `<TransitionGroup>` move animation, then debouncing a `ResizeObserver`-driven height update —
    were both barking up the wrong tree). The real cause: each instance card
    (`instance-card-view.vue`) had `transition-all`, which — as the name says — transitions *every*
    animatable property that changes on it, including its own `width`. That width is set by the
    grid's responsive `auto-fill`/`minmax` column sizing, which is continuously changing throughout
    an active resize — so the card's rendered box was perpetually chasing a 150ms-lagged, constantly
    moving target instead of tracking the grid directly. Narrowed to
    `transition-[color,background-color,border-color,filter,transform]` — the properties actually
    meant to animate here (hover/selection color, border, brightness, the click/drag scale) — so the
    card's layout box itself is never transitioned. That fix was real, but it wasn't the whole
    story: even with no transition lagging behind, the grid columns were defined as
    `minmax(<min>,1fr)`, and that `1fr` is what makes every card stretch to fill whatever space is
    left over in the row, edge to edge — continuously, as the container's width changes. So cards
    were still visibly growing throughout a resize (correctly, natively, with zero lag), then
    snapping smaller the instant the grid fit one more column and had to redistribute the row among
    more of them. That's just what `1fr` does; it was never an animation bug. Dropped the `1fr` in
    `library/instance-group/index.vue`'s grid-cols classes so cards are a fixed size — only the
    *column count* changes on resize now, not each card's own width. Trade-off: rows can end with a
    bit of empty space on the right instead of always filling edge to edge.
  - The tab itself (`.sidebar-edge-tab`) always sits right at the current content/sidebar boundary
    — the window's right edge when closed, the sidebar's own left edge once open (`.sidebar-edge-zone`
    animates its `right` offset the same 0.32s as the push) — so it reads as "the handle for this
    edge" in both states, and stays visible while open so there's always an obvious way to close it.
- **Sidebar ad + "Upgrade to Modrinth+"**: hidden, not deleted. Flip `STUDIO_HIDE_SIDEBAR_PROMO`
  in `apps/app-frontend/src/App.vue` back to `false` to restore it.
- **Replays tab**: new tab on each instance, shown only if it has a `replay_recordings/`
  (ReplayMod) or `flashback/replays/` (Flashback) folder. Lists recordings with whatever
  metadata we could read out of them, a thumbnail if the replay has one, and lets you
  launch the instance, rename/delete a recording, open its folder, or add an existing
  replay file.
  - Neither mod publishes a versioned file format, so metadata parsing
    (`packages/app-lib/src/api/replays.rs`) falls back to filename and file timestamps for
    anything it can't read. That said, both mods' actual field names are now confirmed
    directly from their source (not guessed):
    - ReplayMod (`.mcpr`), via ReplayStudio's `ReplayMetaData.java`: `duration`, `date`,
      `mcversion`/`gameVersion`, `serverName`/`customServerName`, `singleplayer`. Thumbnail
      lives at `thumb.jpg` (or legacy `thumb`, optionally prefixed with a 7-byte Fibonacci
      magic-number header that must be stripped) — see `AbstractReplayFile.java`.
    - Flashback, via its own `FlashbackMeta.java`: **no** `duration`/`date`/`mcversion`/
      `serverName`/`singleplayer` fields exist at all — it never records that distinction,
      full stop. It has `world_name` and `total_ticks` instead. An earlier version of this
      tab treated Flashback's always-missing `singleplayer` as "assume true", which is why
      every Flashback replay used to show "Singleplayer" regardless of how it was actually
      recorded. Now: `world_name` is shown in place of the singleplayer/multiplayer guess,
      and duration is approximated from `total_ticks * 50ms` (assumes a steady 20 ticks/sec
      — an approximation, not a real stored duration like ReplayMod has). Thumbnail is
      Flashback's `icon.png`, written on a best-effort basis by `ReplayExporter.java` (not
      every replay will have one).
    - Thumbnails are fetched lazily per-row (`replays_thumbnail` command /
      `getReplayThumbnail()`), not bundled into the bulk `list_replays` call — with hundreds
      of replays, eagerly decoding + shipping every image up front would undo the concurrent
      metadata-loading perf work below.
  - Neither mod exposes a way to jump straight into a specific replay — "Launch" starts the
    instance the normal way; open the replay from the mod's own in-game UI from there.
- **Playtime correction** (Settings → General, per instance): a "Playtime correction" button
  opens a modal to set a manual +/- hours adjustment, for crediting time tracked by another
  launcher (Prism, MultiMC, etc.) before switching to Modrinth. Shown as two separate stat lines
  above the button — "Modrinth playtime" (the real tracked total) and "Corrected playtime" (the
  manual adjustment) — and only combined into one number for display elsewhere (e.g. the
  instance page header's playtime badge).
  - Stored in a brand-new `studio_playtime_corrections` table (own migration,
    `packages/app-lib/src/api/playtime_correction.rs`) rather than a new column on `instances`.
    `packages/app-lib/.cargo/config.toml` forces `SQLX_OFFLINE`, so upstream's `instances` queries
    (written with the compile-time-checked `sqlx::query!` macro) can only change alongside a
    regenerated `.sqlx` cache via `cargo sqlx prepare` — not something this fork's dev loop should
    depend on. A separate table + the plain runtime `sqlx::query()` API (already used elsewhere in
    this codebase, e.g. `instance_rows.rs`) needs no cache entry at all and can't touch upstream's
    own queries.
  - The value set via the modal *replaces* the stored correction, it isn't added to it each time —
    the modal always loads and pre-fills the current value so it reads as "edit this number," not
    "add more on top."
  - Deliberately not folded into Library's "Hours played" sort (`use-library.ts`) — that sorts a
    full list of instances synchronously, and querying every instance's correction to support it
    would mean either a bulk query added to a per-instance-designed table or an async waterfall
    over however many instances exist. Sorting there still reflects only Modrinth's own tracked
    time.
- **Appearance settings** (Settings → Appearance, below Modrinth's own section): custom accent
  color, an image or gradient background, a popup transparency slider, a surface darkness
  slider, and a custom window/taskbar icon. All implemented as runtime CSS variable overrides
  (`apps/app-frontend/src/composables/use-studio-appearance.ts` +
  `apps/app-frontend/src/assets/styles/studio-overrides.css`) — nothing in the shared
  `packages/ui` / `packages/assets` theme files was touched.
  - The window icon change only affects the *running* app (title bar/taskbar while it's open).
    Windows' own icon for the installed .exe/shortcut (what Explorer shows before you open it)
    can only be changed by rebuilding with new files under `apps/app/icons/` — see
    [Tauri's icon docs](https://v2.tauri.app/reference/cli/#icon) (`pnpm tauri icon <image>`).
  - **Multiple background images**: "Choose image(s)..." (multi-select file picker) or "Choose
    folder..." (every image file directly inside it, non-recursive, sorted alphabetically) both
    replace the whole background "pool" at once — there's no "add one more" on top of an existing
    set. With more than one image, two more controls appear: **Loop** (cycles through in that
    fixed order) vs **Random** (won't repeat the same one twice in a row) for which one comes
    next, and a **Change** dropdown for how often — five fixed timers (5m/15m/30m/1h/6h), once per
    day, or once per session (advances exactly once per app launch, not on a timer at all).
    - Backend: `studio_set_background_images`/`studio_set_background_folder`
      (`apps/app/src/api/studio.rs`) copy the picked file(s) into a fresh
      `$APPCONFIG/studio/backgrounds/<timestamp>/` batch directory each time (previous batches are
      deleted first) — same reasoning as the old single-background copy (asset-protocol scope +
      the original file could move/be deleted), extended to a whole set instead of one path.
    - Frontend: the pool lives in `backgroundImagePaths`; `backgroundImagePath` (the one field
      `applyBackgroundVars()` actually reads) is always whichever pool entry is currently showing,
      so rotation logic doesn't need to touch anything CSS-related directly — moving
      `backgroundImagePath` to the next entry is enough, the existing watcher picks it up.
    - Timed intervals are checked every 30s against `backgroundLastRotatedAt` (a real persisted
      timestamp, not a countdown) rather than scheduled as a precise `setTimeout` — that survives
      the interval/pool changing mid-wait and the app being closed/reopened (e.g. "once per day"
      set yesterday correctly fires shortly after launch today, instead of needing to have been
      running for a full 24h). "Once per session" is deliberately its own code path
      (`initializeStudioBackgroundRotation()`, called once from `main.js`) rather than folded into
      `applyStudioAppearance()` — that function also re-runs on a plain theme change, which must
      never count as a new "session".
    - The new `studio/backgrounds/<batch>/` folder needed its own entry in `tauri.conf.json`'s
      `security.assetProtocol.scope` (`$APPCONFIG/studio/backgrounds/*/*`) — the existing
      `$APPCONFIG/studio/*` only covers files directly inside `studio/`, one level deep, so images
      copied there were silently blocked by the asset protocol (no error, they just never loaded)
      until this was added. Worth remembering for anything else added under a new subfolder later.
    - The rotation interval picker uses `@modrinth/ui`'s `Combobox`, not `DropdownSelect`.
      `DropdownSelect`'s option list is an absolutely-positioned panel rendered inside its own DOM
      subtree, which got clipped by this Settings modal's scrollable body (the same category of bug
      `StudioColorSwatch`'s popover had before its own boundary-detection fix). A plain native
      `<select>` would dodge the clipping too, but it looks like an unstyled OS control next to
      everything else here, so it was rejected in favor of `Combobox`, which teleports its option
      panel to a root-level `#teleports` div (`<Teleport to="#teleports">`, see
      `packages/ui/src/components/base/Combobox.vue`) — outside any ancestor's `overflow`, while
      still rendering with Modrinth's own dropdown styling. This is the same component already used
      for the Browse page's "Sort by"/"View" dropdowns.
  - **Startup screen background** ("Settings → Appearance → Startup screen background") lets a
    picked image replace `SplashScreen.vue`'s default cube artwork, with a "Reset to default"
    button. Reuses the exact same `copy_into_studio_dir()` helper as the app icon feature
    (`studio_set_splash_background` in `apps/app/src/api/studio.rs`, dest stem
    `"splash-background"`) — same reasoning: the webview can only load images from inside the
    asset-protocol scope, not arbitrary filesystem paths, and the originally-picked file could
    later move or be deleted. Reset is pure frontend state (`splashBackgroundPath: null`) with no
    file to clean up — `SplashScreen.vue` just falls back to its own built-in `.cube-bg` CSS
    background whenever nothing is set, so there's nothing to delete or restore. Applied via an
    inline style (`background-image`/`background-size: cover`/`background-position: center`)
    rather than a CSS custom property like the main background image, since an arbitrary picked
    photo needs `cover` sizing, not the default artwork's `contain`.
    - The picker also accepts a **video** (mp4/webm/mov/m4v) or an animated **GIF**. A GIF needs
      no special handling at all — an animated GIF used as a plain CSS `background-image` already
      plays and loops on its own, so it flows through the exact same still-image code path.
      Video is genuinely different: `SplashScreen.vue` detects the extension
      (`isVideoBackground`) and swaps in a real `<video autoplay muted loop playsinline>` element
      instead of a `background-image` div for that case (a `key` on both branches forces Vue to
      fully replace the element rather than patch it in place when switching between them, so a
      stale `<video>` never lingers with the previous `src`). This needed one additional fix
      outside this component: `apps/app/tauri.conf.json`'s CSP `media-src` directive only allowed
      `https://*.githubusercontent.com` (for existing GitHub-hosted content elsewhere in the app) —
      video/audio elements are governed by `media-src`, not `img-src`, so a local file loaded via
      `convertFileSrc()` (an `asset:`/`http://asset.localhost` URL) was being silently blocked
      before this was added, the exact same category of "no error, it just never loads" bug as the
      asset-protocol-scope gotcha above, just enforced by CSP instead of Tauri's own scope config.
    - **"Replay startup animation"** (Settings → Appearance) re-shows the splash briefly to preview
      an accent color or startup background pick without restarting the whole app. This is
      deliberately its own tiny isolated composable (`use-splash-preview.ts`, a single
      `splashPreviewActive` ref + a `previewSplashScreen()` function that flips it off-then-on
      across a frame so a preview clicked again mid-preview restarts cleanly instead of just
      extending the running one) rather than reusing `@modrinth/ui`'s real `LoadingStateProvider` —
      firing fake `begin()`/`end()` tokens through the app's actual "is anything loading" tracker
      just to show an animation risked interfering with (or being interfered with by) genuine
      loading state. `SplashScreen.vue`'s `v-if` is simply `!doneLoading || splashPreviewActive`,
      and since `<SplashScreen>` stays mounted for the app's entire lifetime (`App.vue` only
      `v-if`s it away on a hard state-init failure), the same instance is still alive and watching
      that flag long after real startup finished. The progress bar has no real load to reflect
      during a preview, so it just runs a flat multi-second fill for looks, driven off the same
      `SPLASH_PREVIEW_DURATION_MS` the auto-hide timeout uses.
  - **Video** is a background mode between Image and Gradient — one or more videos
    (mp4/webm/mov/m4v) instead of a still image. See the dropped-feature note below for why
    there's no reverse/boomerang playback option.
    - Rotation with multiple videos is triggered by playback actually finishing, not a timer:
      `StudioVideoBackground.vue`'s `<video>` binds `loop` to `true` only when there's exactly one
      video (simplest — the native attribute handles everything, and a looping element never fires
      `ended`), and to `false` when there's more than one, specifically so `ended` *does* fire once
      each clip completes. Its `@ended` handler calls the new `advanceVideoOnEnded()`, which advances
      to the next clip per `backgroundRotationMode` ('loop' cycles in order, 'random' picks a
      different one at random) — the same pool-advance logic multi-image uses, just invoked from a
      media event instead of `maybeRotateOnSchedule()`'s timer. This was a deliberate change from an
      earlier version that reused the image pool's fixed-interval Combobox for video too: a timer is
      the wrong trigger for video specifically because clip lengths vary arbitrarily (a 20-minute
      video on a 5-minute interval would get cut off mid-playback), whereas "the clip actually ended"
      is always the right moment to advance. `maybeRotateOnSchedule()`/
      `initializeStudioBackgroundRotation()` stayed image-only as a result, and the video panel in
      `StudioAppearanceSettings.vue` has no rotation-interval `Combobox` — just the Loop/Random pills
      plus a line of text describing what each does, since there's no interval to configure.
    - This needed a real `<video>` element rather than fitting into the existing `--studio-bg-image`
      CSS-custom-property mechanism the Image/Gradient modes share — a CSS `background-image` has no
      way to play video at all. `StudioVideoBackground.vue` is a new component, mounted once near the
      top of `App.vue`, that renders a `position: fixed; inset: 0; z-index: -1` `<video>` whenever
      `backgroundMode === 'video'`. For that to actually be visible, `.app-contents`'s own
      `background-color` (which paints on top of it otherwise) had to become conditionally
      transparent — a new `--studio-app-bg-color` var, set to `transparent` only in video mode
      (`applyBackgroundVars()`), consumed in `studio-overrides.css`. The existing "Darken/tint
      strength" overlay gradient still paints on top of that same element regardless of what's
      behind it, so that slider needed no changes at all to keep working for video.
    - The video pool (`backgroundVideoPath(s)`) is entirely separate storage from the image pool,
      both in frontend state and on disk (`$APPCONFIG/studio/background-videos/` vs
      `.../backgrounds/`, via a new `fresh_video_batch_dir` alongside the existing
      `fresh_backgrounds_batch_dir`, both now thin wrappers over one shared `fresh_batch_dir(app,
      root_name)`). This matters because each pick command wipes and replaces *only its own root
      folder* — sharing one folder between image and video pools would mean picking a video while
      Image mode was previously configured (or vice versa) silently deleted the other pool's files
      out from under it, even though its `state.backgroundImagePaths` (say) still pointed at them.
      The two pools do still share `backgroundRotationMode`/`backgroundRotationInterval`/
      `backgroundLastRotatedAt` — only one pool is ever actually active at a time (whichever
      `backgroundMode` points at), so a second full set of rotation controls would've been pure
      duplication. The actual pool-advance step, `rotateBackgroundImage()`, was generalized to
      operate on "whichever pool is active" (`activePoolPaths()`/`activePoolCurrent()`/
      `setActivePoolCurrent()`) rather than hardcoding the image fields, so both `maybeRotateOnSchedule()`
      (image's timer trigger) and `advanceVideoOnEnded()` (video's `ended`-event trigger — see below)
      call into the same shared logic despite firing on entirely different signals.
    - **Dropped feature: boomerang playback.** An earlier version let a video play forward, then
      reverse back to the start, then forward again. No browser engine actually supports playing
      `<video>` backward — a negative `playbackRate` is accepted but silently does nothing in every
      engine it's been tried on — so this was faked by listening for `ended` and manually stepping
      `currentTime` backward every `requestAnimationFrame` tick (later reworked to wait for the
      browser's own `seeked` event between steps, the standard version of this trick). Even the
      corrected version stayed visibly choppy: every backward step is a fresh seek, and a seek can
      only jump straight to a keyframe — anything between two keyframes has to be decoded forward
      from the last one first. How janky that looks is dictated entirely by the source video's own
      keyframe spacing (most consumer encodes place them every 1-10 seconds), not by anything this
      component could control; a properly smooth version would need frequent-keyframe/all-intra
      source encoding, which isn't something to assume of whatever file a user picks. Removed
      rather than ship something that reads as broken more often than not — Video mode now just
      always loops via the plain native `loop` attribute, same as Image/Gradient's simplicity.
    - Needed one more asset-protocol-scope entry (`$APPCONFIG/studio/background-videos/*/*`,
      `tauri.conf.json`) for the new folder — same category of gotcha as the multi-image backgrounds
      folder above; scope entries are never recursive/inherited from a parent pattern.
  - There are two separate transparency/darkness controls, on purpose, because one combined
    slider used to conflate two different things:
    - **Popup transparency** (`popupOpacity`) controls only `.modal-body`'s own opacity
      (`--studio-modal-opacity`), and works in every background mode — it's just "how see-through
      are popups," independent of whether there's a custom background to show through them.
    - **Surface darkness** (`surfaceDarkness`) only applies while a custom background is active,
      and only darkens (tints toward black) the app's general panels/buttons/cards/sidebar
      (`SURFACE_PROPERTIES` in use-studio-appearance.ts) — it does **not** affect their
      transparency. Those surfaces' transparency is a fixed constant (`FIXED_SURFACE_ALPHA`,
      currently 0.75), deliberately not user-adjustable: dragging a "darkness" slider that also
      changed transparency could visibly *brighten* the screen (a very transparent panel lets much
      more of a bright image through), which read as broken rather than intentional.
    - Most of these surfaces are also paired with a `backdrop-filter` blur for a frosted-glass
      look (`studio-overrides.css`). The blur is deliberately **not** applied to
      `ContentCardTable`'s rows (the mod/resource-pack/shader/datapack list) — that list can run
      into the hundreds of rows, and blurring every one of them at once overwhelmed the webview's
      compositor (corrupted color blocks, heavy flashing while scrolling). Those rows stay plain
      see-through instead. If a similar long, densely-repeated list gets this treatment in the
      future, give it the same exception rather than adding it to the blur selector list in
      `studio-overrides.css`.
    - **Blur strength** (`glassBlurStrength`, 0-20px, Settings → Appearance) controls the radius on
      that same `backdrop-filter: blur()` (`applyGlassBlurVar()` in use-studio-appearance.ts) — 0
      turns it off entirely for a plain translucent look with no frosted-glass softening, without
      needing a separate toggle. This used to be a hardcoded flat `blur(10px)`, deliberately never
      wired to a slider — the comment there at the time pointed at the old FPS-drop-while-dragging
      bug (see `applyStudioAppearance()`'s watcher comment) as the reason. That bug was actually
      about one combined watcher re-running *every* apply function, including an expensive
      background-image re-decode, on every tick of an unrelated slider — not about blur specifically.
      `applyGlassBlurVar()` only ever sets one CSS custom property, exactly as cheap as
      `applyModalOpacityVar()` (which already was slider-driven), so it gets its own narrow watcher
      the same way, with no dragging performance issue.
    - **A translucent Logs tab terminal was attempted and then fully reverted.** xterm.js renders
      onto its own `<canvas>`, independent of the CSS above, and its theme-color parser
      (`css.toColor`, confirmed by reading the bundled `@xterm/xterm` source) explicitly **throws**
      unless a color's alpha reads back as exactly 255 — it hard-rejects any non-opaque color,
      including the literal string `'transparent'`. Multiple layers were tried to get around this
      (fading `.xterm-screen` with a CSS `opacity`, making `.xterm-viewport` and the outer
      `studio-terminal-frame` fully transparent so only one translucent layer remained instead of
      several compounding on each other, fixing a third compounding layer specific to the Expand
      button's fullscreen console view) — DevTools confirmed every one of these rules really was
      winning the cascade and targeting the right element, and it *still* never revealed anything: an
      extreme, live-edited test opacity of `0.2` on `.xterm-screen` showed no background at all.
      Whatever is actually blocking `backdrop-filter` from compositing here couldn't be pinned down
      without being able to run the app directly, so this was dropped rather than keep guessing.
      Separately, making the fullscreen overlay's own wrapper transparent (it's a plain `bg-surface-1`
      `fixed inset-0` div, intentionally opaque to hide the real page still mounted underneath it)
      actively broke fullscreen mode — the real page's header/tabs bled through and overlapped the
      console's own header. That part is reverted for good, not just paused: a `position: fixed`
      overlay needs an opaque backdrop regardless of what happens with the terminal itself. The Logs
      tab is back to plain and opaque, matching every build before this was tried.
    - **The attempt did surface one real, unrelated bug, which is fixed**: error/warn log lines were
      rendering with a background that visibly didn't match the rest of the terminal. `colorize()` in
      `packages/ui/src/layouts/shared/console/composables/console-filtering.ts` marks those lines with
      literal ANSI code `40` (`\x1b[31;40m`/`\x1b[33;40m`, red/yellow text on ANSI "black"), and
      `buildTerminalTheme()` mapped that ANSI "black" straight to `--surface-2`. While a custom
      background is active, `--surface-2` is a `color-mix(...)` expression rather than a plain hex —
      hitting the exact same xterm parser rejection described above, just less obviously, so every
      error/warn line's background silently fell back to whatever xterm treats an unparseable theme
      color as, instead of matching the terminal's own background right next to it. Fixed the same way
      `background` already handled this: `black` and `cursorAccent` in `buildTerminalTheme()` are now
      hardcoded to the same opaque hex, not read from the (possibly non-hex) `--surface-2` variable.
  - "Darken/tint strength" (`backgroundOverlay`, on the image/gradient itself, separate from
    surface darkness above) blends toward pure `black`, not `var(--color-bg)` — the app's own
    dark-theme background is dark but usually not literal black, which used to cap how dark the
    image could ever get regardless of how far the slider was pushed.
  - Switching Modrinth's own built-in theme (dark/light/oled/retro) while a custom background is
    active needs `use-studio-appearance.ts` to re-capture each surface's "original" color and
    recompute — it only ever computes that once and caches it (`surfaceOriginals`), so a plain
    theme-change watcher clears that cache and reapplies. Skipping this made theme switching
    silently do nothing while a custom background was on, since every surface kept being
    tinted/faded from the previous theme's stale cached colors forever.
  - **Color theme is locked to Dark whenever a custom background (image/gradient) is active** —
    the Settings → Appearance "Color theme" picker is hidden entirely in that case
    (`html.studio-theme-locked section:has(.theme-options)` in `studio-overrides.css`), and
    `applyThemeLock()` force-sets the underlying theme to `dark` and snaps it back any time
    something else changes it while locked. This isn't a preference — light/oled/retro were
    never going to render correctly here: the surface-darkening/tint math above assumes a dark
    theme (darkening panels toward black only helps when text is light), and light theme
    separately uses solid (not translucent) colors for its own selected-state UI, which doesn't
    compose with see-through surfaces at all. Switching back to the "Default" background restores
    whatever theme the user actually had chosen before locking (`themeBeforeLock`).
  - A dark enough accent color (e.g. near-black) used to leave several places with unreadable
    text: the Settings nav's selected item, the Beta badge, and the Combobox dropdown's selected
    option all put accent-*colored text* over a translucent *tint* of that same color — fine for
    any normal accent, since colored text still reads against the otherwise-dark panel underneath,
    but not once the accent itself is dark enough to double as invisible text. Rather than replace
    "colored text" outright (losing the accent-tinted look for every normal color, not just the
    broken ones), `applyAccentVars()` only swaps these to a plain readable color in the specific
    case that's actually broken — when the accent's own contrast color came out white — and leaves
    them referencing the accent directly otherwise. (This is separate from `--color-accent-contrast`
    itself, used for genuinely solid accent-colored surfaces like the Play button — those already
    always followed the accent's contrast correctly, regardless of how dark or light it is.)
    Same fix, same `--studio-accent-safe-text` variable, also applied to: Tabs.vue's active tab
    (the Browse/Discover filter pills, e.g. "All/Mods/Resource Packs" — same
    `bg-highlight-green`/`text-green` pattern as Combobox, scoped to `[role="tablist"]` since that
    component has no dedicated class of its own); Toggle.vue's "on" thumb, hardcoded to
    `bg-black/90` (fine against a normally-bright track color, invisible once the track itself —
    `bg-brand` — is dark); and our own background-mode pills (Default/Image/Gradient) in
    `StudioAppearanceSettings.vue`, which used `text-brand` directly.
  - The accent/gradient color picker (`StudioColorSwatch.vue`) opens its popover flipped to
    whichever side/direction actually has room, instead of always growing right-and-down from the
    swatch. That room is measured against the Settings modal's own scrollable panel
    (`.modal-body`), not the window — the panel is narrower than the window (it's centered with
    its own max-width), so a popover that fully fits within the *window* can still overflow the
    narrower panel; that doesn't visually "clip" so much as make the panel gain a horizontal
    scrollbar to accommodate it, which was the actual symptom reported.
  - "Keep update buttons green" (only shown once an accent color is set) opts a color back out
    of the accent recolor — `--color-green` is shared by a lot of unrelated "positive" UI
    (update buttons, the Beta tag, ping indicators), so this is a blanket "leave green stuff
    green" switch, not something scoped to literally just update buttons.
  - The home screen's "Jump in" Play button was one more thing that pattern didn't cover, but for
    a different reason than the Beta tag/Combobox/Tabs cases above: those were all one shared
    literal-green *class* recolored via a CSS override, but `InstanceItem.vue`'s Play button
    (`Button` with `color="green"`) sets its color via an inline `--button-color` CSS variable per
    instance, with no shared class to hook a CSS override onto. It also isn't a "positive status"
    color at all here, just upstream Modrinth's own inconsistency — the sibling `WorldItem.vue`'s
    otherwise-identical Play button already used `color="brand"`. Simplest fix was at the source:
    changed `InstanceItem.vue` to `color="brand"` too, so both Jump-in card types behave the same
    and always follow the accent color, with or without "keep update buttons green" on.
  - **"Everything bounces/unfolds" specifically when the window is restored after being
    minimized: attempted through six different fixes, all abandoned and fully reverted.** Not
    worth the trouble it caused for how minor the person considers the bug — the sixth attempt
    (disabling DWM's window-restore compositor animation via `DwmSetWindowAttribute` +
    `DWMWA_TRANSITIONS_FORCEDISABLED` in `main.rs`) made the actual visible behavior on restore
    noticeably worse, which is what ended the investigation. All six attempts below were removed
    entirely rather than left in place, including the "Reorder 'Jump in' live" toggle that one of
    them added (a `liveReorderJumpIn` setting in Settings → Appearance) — gone along with
    everything else, even though it addressed a real, separate stock-Modrinth rough edge on its
    own merits (see the git history around this entry if that specific toggle is ever wanted back
    on its own). None of these should be reattempted without new evidence:
    1. A guess that clicking Play re-sorts "Jump in" live (`set_instance_last_played` in
       `launch_context.rs` updates `last_played` before the game even opens, immediately
       re-sorting the row) and that suppressing this was the fix — the toggle for it made no
       difference when actually tested.
    2. A guess that WebView2 suspends rendering while minimized while JS keeps running regardless,
       so accumulated changes all animate at once on restore — freezing CSS transitions for one
       paint on `visibilitychange` made no observable difference.
    3. A guess that TanStack Query's default `refetchOnWindowFocus: true` was refetching every
       mounted query at once on restore — flipping the default off via an explicit `QueryClient`
       in `main.js` also made no observable difference to the bounce.
    4. A guess built from extracting the person's screen recording frame-by-frame (`ffmpeg -vf
       fps=60`), read at the time as evidence of a gradual OS resize animation — hiding the app on
       `visibilitychange` and revealing it once a burst of `resize` events settled. Also no
       observable difference.
    5. Only after adding `console.log` calls to every candidate event and reading back real
       DevTools output: `visibilitychange` turned out to **never fire at all** on this window on
       minimize/restore, explaining why both attempts gated behind it did nothing. What did fire
       was a plain `resize` event, jumping directly between a tiny minimized size (`144×19`) and
       the real size with no in-between steps — hiding `#app` on that jump and revealing it after
       several `requestAnimationFrame`s. Still no observable difference.
    6. Reasoning that a single instant `resize` jump with zero in-between DOM states meant this was
       never a web content issue at all, but Windows' own DWM compositor animation for restoring a
       window from the taskbar — disabled specifically for this window via `DwmSetWindowAttribute`
       + `DWMWA_TRANSITIONS_FORCEDISABLED` in `main.rs` (the `windows` crate and
       `Win32_Graphics_Dwm` feature were already a dependency, used elsewhere by
       `ads_occlusion_windows.rs`). This is the one that made things visibly worse, and is why the
       whole investigation stopped here rather than continuing to a seventh attempt.
  - Separately (and unrelated to the abandoned bounce investigation above): a white flash on
    restoring from minimize that the person confirmed didn't happen before any of this session's
    changes. The window config had no `backgroundColor` set, so WebView2 falls back to its own
    default white paint surface for the brief moment during a real window resize before the page's
    own (dark) CSS repaints on top of it — a well-known Tauri-on-Windows resize artifact, not
    something reachable from the page's own JS/CSS at all. Set `backgroundColor: "#16181c"` (the
    dark theme's `--surface-1`) on the main window in `apps/app/tauri.conf.json`, so that gap shows
    the app's own dark background color instead of WebView2's default white.
  - Any component that hardcodes a `.dark`/`.light` theme class on itself (like
    `SplashScreen.vue` does, so it looks the same regardless of the user's actual theme
    setting) will locally reset `--color-brand` back to plain green — `variables.scss`'s theme
    blocks redeclare it, and a declaration on the element itself always beats one inherited
    from `<html>`, no matter how the two compare in specificity. `--studio-brand-override` (set
    alongside the normal accent vars in `use-studio-appearance.ts`) exists so a component like
    that can restore the accent with `.my-component.dark { --color-brand: var(--studio-brand-override, var(--color-green)); }`.
    See the comment above `SplashScreen.vue`'s `.splash-screen.dark` rule for the full story.
  - **Fixed a whole-app freeze that only surfaced after long play sessions and needed a restart**
    (`packages/app-lib/src/api/server_address.rs`, `resolve_server_address`). Every server ping the
    app makes anywhere — recently-played cards on Home, the Worlds tab, adding/editing a server —
    funnels through a single global `Semaphore::const_new(24)` that caps how many DNS lookups can be
    in flight at once. The DNS lookup itself (`resolver.srv_lookup(...)`) had no timeout of its own,
    only relying on hickory-resolver's internal default — which is usually fine, but isn't a hard
    guarantee under every network condition (a captive portal or a VPN/firewall setup that silently
    swallows outbound UDP rather than rejecting it can make a single query hang far longer than
    that). A permit is only released once the `.await` holding it returns, so a lookup that hangs
    forever leaks one of the 24 permits forever. Ping the same flaky server enough times over a long
    session (e.g. it keeps showing up on Home and getting re-pinged) and the semaphore eventually
    hits zero free permits — at which point *every* server ping anywhere in the app, not just the
    original flaky one, blocks forever waiting for a permit that will never come back. Nothing else
    in the app was found to depend on that same stuck resource, so this wouldn't necessarily freeze
    unrelated UI on its own — but it's a real, unbounded resource leak that exactly matches "worked
    fine for a while, then needed a restart," so it's fixed here regardless: the lookup is now
    wrapped in the same `tokio::time::timeout` pattern `util/server_ping.rs`'s own
    `SERVER_STATUS_TIMEOUT` already uses for the ping step, so the permit is always released within 5
    seconds no matter how badly the network misbehaves, treating a timeout the same as "no SRV
    record found" (fall back to the address as typed) rather than erroring. This was investigated
    but not confirmed against an actual reproduction/crash dump — if a full freeze recurs after this
    ships, capturing a Task Manager dump of the process at the time (Details tab → right-click
    `Modrinth Studio.exe` → Create dump file) before restarting would make the next investigation
    far more conclusive than static code reading alone.
  - **Fixed a permanent "Linked modpack project  not found" error (note the blank spot) on
    instances imported from Prism Launcher** — a known issue in stock Modrinth App too, not
    something this fork introduced, but fixed here anyway since there's no reason to inherit it.
    Prism's `instance.cfg` can have `ManagedPackID`/`ManagedPackVersionID` keys present but blank
    (`ManagedPackID=` with nothing after the `=`), which deserializes to `Some("")`, not `None`.
    Every place that checked "is there a linked modpack?" only tested `Some(...)`, so a blank ID
    passed that check exactly like a real one and got turned into an
    `InstanceLink::ModrinthModpack` pointing at an empty project ID — permanently, since there was
    no in-app way to clear it, and every attempt to resolve it (for the Content tab, update checks,
    etc.) correctly failed to find a project with an empty ID and surfaced this error forever after.
    Fixed in two places: `api/pack/install_from.rs` now discards a blank project/version ID before
    ever constructing the link (so new imports can't get into this state), and — more importantly for
    anyone who already hit this — `state/instances/commands/list_content.rs`'s `linked_modpack_ids()`
    (the single choke point every `InstanceLink` variant funnels through before a lookup is even
    attempted) now treats a blank ID as "no link at all," which repairs existing already-imported
    instances automatically on next launch, with no reimport needed.
  - **The `MultiSelect` search box's focus ring was clipped** on any searchable dropdown (e.g. the
    "Game versions" filter on a project's Versions page) — not Studio-specific, a genuine upstream
    bug in `packages/ui/src/components/base/MultiSelect.vue`. `InputFrame.vue` shows focus with
    `ring-4` (a 4px box-shadow, rendered *outside* the element's own border), but the search row
    wrapping it had `px-0 py-1.5` — flush against the dropdown's own `overflow-hidden rounded-[14px]`
    edges on every side, leaving the ring nowhere to render without being cut off. Fixed by giving
    that row real padding (`px-2 py-2`) so the ring has room on all sides.
  - **Accent-tinted default app icon.** When a custom accent color is active and the person hasn't
    picked a fully custom window icon (Settings → Appearance → "App icon"), the running
    window/taskbar icon is recolored to match — the same ring-mark path data as
    `packages/assets/branding/logo.svg`, rendered onto a dark circular backdrop via Canvas at
    runtime (`renderAccentIconPng()` in `use-studio-appearance.ts`) and applied with
    `getCurrentWindow().setIcon()`, the same mechanism the custom-icon feature already used. A new
    `studio_set_generated_app_icon` Tauri command (mirrors `copy_into_studio_dir`'s pattern, just
    writing generated bytes instead of copying a picked file) persists it to
    `$APPCONFIG/studio/generated-icon.png` first, since `setIcon()` needs a path. Governed by a new
    `tintDefaultIconWithAccent` toggle (default **on**), placed directly under the accent color
    picker (not the App icon section further down — it's a property of the accent, not the icon
    picker) and shown only once it's actually relevant (accent active, no custom icon). Turning the
    toggle off, or resetting the accent color, regenerates and re-applies the same icon in the
    stock green (`applyAccentIconTint()`'s `DEFAULT_ICON_COLOR`, matching dark theme's
    `--color-green-500`) instead of leaving the last tint color stuck — `hasAppliedGeneratedIcon`
    tracks whether this session ever actually overrode the icon, so a launch where the accent is
    never touched at all leaves the true exe-embedded default completely alone.
  - **Pinning the app to the Windows taskbar showed the exe's built-in default icon instead of the
    runtime-applied one (custom or accent-tinted), even while the app was running.** First attempt:
    Windows groups a pinned shortcut and its running window into one combined taskbar button by
    "Application User Model ID" (AUMID), and without one explicitly set both fall back to an
    *implicit* AUMID derived from the exe's path — theorized that pinning keyed the combined
    button's icon off that implicit identity. Set an explicit AUMID via
    `SetCurrentProcessExplicitAppUserModelID`, first thing in `main()` (`apps/app/src/main.rs`,
    Windows only, before any window is created), plus a safety net in `App.vue` re-applying the
    current icon on every focus change. **Confirmed by testing not to fix it** — the AUMID wasn't
    the actual mechanism (left in place regardless; a stable explicit AUMID is harmless and
    arguably correct on its own merits, it just wasn't the fix for this).

    Root cause is simpler: a taskbar/Start-menu shortcut shows the icon from its *own* `.lnk`
    file's `IconLocation` property, which has nothing to do with what the running window sets via
    `setIcon()` — no amount of runtime `setIcon()` calls can ever reach a shortcut's own icon.
    Fixed by writing the current icon out as a real `.ico` file and editing `IconLocation` on any
    `.lnk` in the two folders Windows itself uses for pins/Start-menu entries
    (`%APPDATA%\...\Quick Launch\User Pinned\TaskBar` and `%APPDATA%\...\Start Menu\Programs`)
    whose target actually resolves to *this exact running exe*. Uses the same `IShellLinkW`/
    `IPersistFile` COM APIs Windows uses to create shortcuts in the first place
    (`apps/app/src/api/studio_pinned_icon_windows.rs`, Windows-only module, `#[cfg(windows)]` in
    `api/mod.rs` per the existing `ads_occlusion_windows` pattern) — deliberately **not** patching
    the installed `.exe`'s own icon resource, which was the initial plan: rewriting a locked,
    running PE file's resources carries real risk of corrupting the install for a purely cosmetic
    feature, whereas editing a `.lnk` (a small, simple, disposable shortcut file the person can
    always re-pin) can't damage anything if it goes wrong. New `studio_apply_pinned_icon` Tauri
    command (`studio.rs`) takes the icon as `.ico` bytes and delegates to the Windows module; no-op
    on other platforms. The frontend wraps the already-generated 256×256 PNG into a minimal
    single-entry `.ico` itself (`wrapPngAsIco()` in `use-studio-appearance.ts`) — Vista+ accepts a
    PNG-encoded image directly as an ICO entry's payload, so this is pure byte-wrapping, no
    re-encoding needed. Called from `applyAccentIconTint()` right after the live `setIcon()` call,
    so a pinned shortcut picks up whatever the running window just did (default green, a chosen
    accent tint, or reverting back to green when the toggle/accent turns off) the next time it's
    read, without needing a relaunch to take effect for a shortcut that's re-pinned or re-read
    afterward.

    First working version matched candidate shortcuts by filename alone (stem contains "modrinth"),
    on the theory that a stray same-named shortcut in these specific per-user folders wasn't a
    realistic concern — wrong in practice: testing with a dev build (`cargo run`, running from
    `target/debug/...exe`) alongside a separately-installed release build repainted the *release
    build's* pinned shortcut, since name-only matching can't tell two different installs of this
    app apart. Fixed by adding a real target check (`shortcut_targets_exe()`): reads the `.lnk`'s
    raw bytes and looks for `std::env::current_exe()`'s path both as UTF-16LE (how modern LNKs
    store it) and as plain ASCII (the legacy `LinkInfo.LocalBasePath` encoding), case-insensitively
    — a byte-level check chosen specifically to avoid needing `IShellLinkW::GetPath`'s exact
    signature (real uncertainty at the time; simpler to sidestep than get precisely right without a
    compiler to check against). A shortcut now needs to pass *both* the name check and this target
    check before its icon gets touched.

    Separately, running a dev build and a pinned release build at the same time can look like the
    running window's live icon "isn't reaching" the taskbar even once this is all working: the
    global explicit AUMID set in `main()` gives *every* build the same Windows Shell identity, so
    Windows may group the dev window into the same combined taskbar button as the release build's
    pin — showing the pinned shortcut's icon on the persistent button while the live, correctly-set
    icon only shows up in that button's hover-preview header. Real end users only ever have one
    installed copy, so this specific confusion is a dev-environment artifact, not a bug in the
    feature itself — testing against a single build (or temporarily unpinning the other one) gives
    a clean read on whether the actual fix is working.

    Next symptom after both of the above: with only the dev build's window in the taskbar, its live
    icon showed correctly tinted in the title bar, alt-tab, and Task Manager — everywhere that reads
    the window's *live* icon — but the persistent taskbar button itself stayed on the untinted
    default. This is a documented Windows/Tauri quirk, not specific to this feature: the taskbar
    button's icon resolution is known to cache more aggressively than those other surfaces,
    including by file path. Both `studio_set_generated_app_icon` and this module's own `.ico` write
    used a single fixed filename overwritten in place on every change — precisely the shape of bug
    `copy_into_studio_dir` already has a comment about, just not one this generated-icon path had
    been given the same treatment for yet. Fixed the same way: each write now gets a fresh,
    timestamped filename (`generated-icon-<ms>.png`, `pinned-icon-<ms>.ico`), with best-effort
    cleanup of the previous one — cleanup ordered to run only *after* every shortcut has already
    been repointed to the new file, so a COM failure partway through can never leave a shortcut
    referencing a file that's already been deleted.

    Still not updating after that — dropped the filename check (stem contains "modrinth") entirely,
    matching purely on `shortcut_targets_exe()`. That check was added for a real reason (see above)
    but is *also* wrong in the other direction: a shortcut Windows auto-generates from "Pin to
    taskbar" on a running window is named from the exe's own version-info metadata, not anything
    this code controls, so a dev/debug build's pin may never contain "modrinth" and would be
    silently skipped before the target check ever ran. The target check alone is sufficient
    correctness-wise (it's already the check that actually matters) and dropping the name
    pre-filter only costs reading a handful of small `.lnk` files once per icon change — not a
    real cost. `studio_apply_pinned_icon` now also returns a plain-text summary (`.lnk` files
    scanned, how many matched by target, how many were actually repointed, and the last COM error
    if any), always logged to the devtools console (`[modrinth-studios] pinned icon apply: ...`) —
    so the next time this doesn't visibly work, the actual failure point (nothing scanned, nothing
    matched, or matched-but-a-specific-COM-call-failed) is a console log away instead of another
    guess.

    First real signal it was actually working: the devtools console log added above showed
    `target_matched=1 icon_updated=1` right after pinning — the COM code genuinely did rewrite the
    shortcut's `IconLocation` on disk. Two things followed from that same log capture:

    1. **A real, separate bug the unique-filename fix had introduced**: interleaved with the
       success lines, `[modrinth-studios] failed to apply accent-tinted app icon: The system cannot
       find the file specified` — a race, not a Windows quirk. `state.accentColor` fires many
       updates a second while the color picker is being dragged, each one re-entering
       `applyAccentIconTint()`; since every call writes `generated-icon-<timestamp>.png` and then
       deletes every *other* file with that prefix, a newer call's cleanup step could delete an
       older, still-in-flight call's own file out from under its pending `setIcon()`. Fixed two
       ways in `use-studio-appearance.ts`: a generation counter that every call checks after each
       `await`, so a superseded call quietly stops instead of racing (the actual correctness fix);
       and a 200ms debounce on the watcher that triggers this from accent/toggle changes specifically
       (`applyAccentIconTint()` itself stays direct/unthrottled for the startup and
       focus-change call sites), which also cuts out the wasted repeated 45-shortcut scans and PNG
       renders while a slider is mid-drag.
    2. **The taskbar bitmap still not visibly updating despite `icon_updated=1`** is consistent with
       a separately documented Explorer/taskbar icon-cache behavior — it's known to cache more
       aggressively than a shortcut's on-disk `IconLocation`, and rewriting those bytes doesn't by
       itself force a repaint. Added a call to `SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, ...)`
       after a successful update — the standard Shell API for telling Explorer to refresh its icon
       cache for a changed association, and a notification-only call with nothing to corrupt if it
       turns out not to help.

    This remains the most speculative Rust of this feature (low-level COM/Shell interop, largely
    unverified against a real compiler in this environment) — every failure mode is still just "the
    pinned icon doesn't update," never anything destructive, but further compile-fix passes on a
    real machine are more likely than not.

    Confirmed working for the accent-tinted icon. Two follow-ups once it was:

    - **The custom (user-picked) window icon never reached a pinned shortcut at all** — the
      pinned-icon push lived entirely inside `applyAccentIconTint()`, which bails out immediately
      whenever a custom icon is set (a custom icon always wins over tinting), so nothing was ever
      calling it in that case. Pulled the pinned-icon push into a shared `pushPinnedIcon()` helper
      and call it from `applyStudioWindowIcon()` too, right after its own `setIcon()` succeeds. The
      picker only allows `.png`/`.ico`; a picked `.ico` is passed straight through unwrapped (it may
      already be multi-resolution itself) while a `.png` gets read via `@tauri-apps/plugin-fs` and
      wrapped the same way as the generated icon. The two functions now share one generation counter
      (renamed `iconTintGeneration` → `iconApplyGeneration`) so they can't race each other either,
      not just themselves.
    - **The generated accent icon looked noticeably softer than the app's real default.** The
      pinned `.ico` only ever had one entry, always the same 256px render regardless of where
      Windows actually needed to show it — a 16px taskbar icon was a naive downscale of that 256px
      image, which looks blurrier than a real app icon's dedicated small-size art. Fixed by
      rendering the ring mark natively at each of several standard sizes (16/24/32/48/64/128/256 —
      `renderAccentIconSet()`) and packing all of them into one proper multi-resolution `.ico`
      (`wrapPngSetAsIco()`, replacing the old single-entry `wrapPngAsIco()`) — since it's vector
      path data being redrawn at each size rather than a bitmap being scaled, every size is exactly
      as crisp as any other. The custom-icon path benefits from the same multi-entry packer when
      wrapping a `.png` (`getPngPixelSize()` reads the real width/height out of the file's own
      `IHDR` chunk so the single entry is labeled correctly) — a picked `.ico` is left untouched
      since it may already be multi-resolution. The live running-window icon (`setIcon()`) is
      unaffected either way — Tauri's `setIcon()` takes one image, not a size set, so that path
      still gets a single `ACCENT_ICON_SIZE` (256px) render as before.

    Custom icons still didn't reach the pinned shortcut after both of those — a permissions gap,
    not a logic bug. `applyStudioWindowIcon()`'s new pinned-icon push reads the custom icon file via
    `@tauri-apps/plugin-fs`'s `readFile()`, which is subject to the fs plugin's own scope allowlist
    (`apps/app/capabilities/plugins.json`'s `fs:scope`) — and that scope only covered
    `$APPDATA/profiles` and `$APPCONFIG/profiles` (plus `$CONFIG/profiles`), not `$APPCONFIG/studio`,
    where `studio_set_app_icon` actually copies the picked file. `readFile()` on that path was
    silently rejected before ever reaching the wrap/push logic — the accent-tint path never hit this
    because it gets its bytes from the canvas render directly, never reading a file back from disk.
    Fixed by adding `$APPCONFIG/studio` and `$APPCONFIG/studio/**` to that same `fs:scope` allow
    list, alongside the existing `profiles` entries.

    Once the permission fix let it through, the custom icon reached the pinned shortcut but looked
    low-resolution — the exact same "one image stretched to every icon slot" problem the accent icon
    had, just reached a different way: the custom-icon path was still wrapping the picked `.png` as a
    *single* ICO entry, labeled with whatever the source file's actual pixel size happened to be
    (`getPngPixelSize()`, reading it straight out of the PNG's own `IHDR` chunk). A single entry
    still means Windows scales that one image for every size it needs, same as before. Fixed the
    same way the accent icon was, just starting from a raster image instead of vector paths: a new
    `buildResizedIconSet()`/`resizePngToSize()` resamples the picked PNG down (or up) to each size in
    the same shared `PINNED_ICON_SIZES` list (renamed from the accent-icon-specific
    `ACCENT_ICON_SET_SIZES`, since both paths use it now) via `drawImage()` with
    `imageSmoothingQuality: 'high'`, cover-fit and center-cropped to a square in case the source
    isn't one. Falls back to the old single-entry behavior only if resizing produces nothing at all
    (no canvas/`createImageBitmap` support, an undecodable file) — still better than no pinned icon.
    An already-`.ico` custom icon is untouched either way, since it may already be its own
    multi-resolution file.

- **The "critical announcement" and "can't reach auth servers" banners in `App.vue` had no way to
  dismiss.** Both use the shared `Admonition` component, which already has a built-in dismiss
  button (`dismissible` prop, `@dismiss` event, top-right `X`) — it just wasn't turned on for either
  of these two. Added a `criticalErrorDismissed` / `authUnreachableDismissed` ref each, gating the
  existing `v-if`. `criticalErrorMessage` only ever gets set once per session (a single fetch on
  mount), so a plain dismissed flag is enough for it. `authUnreachable` is a computed tied to a
  query that refetches every 5 minutes and can flip true again later on a fresh, unrelated network
  blip — a `watch` on it clears the dismissal specifically when a *new* occurrence starts (false →
  true), so dismissing one outage doesn't permanently silence a real future one. Also passed
  `center-content` (an existing prop, previously unused here) to both — without it the icon and
  dismiss `X` align to the top of the row instead of centering against the header+body text block,
  which looked off whenever the body wrapped to two lines.

- **Instances survive being renamed/moved in Explorer, and a new Settings → Storage page shows
  disk usage per instance.** Prompted by a friend who keeps 30+ modpacks, reorganizes them by hand
  in the `profiles` folder, and duplicates them from templates constantly (hitting Modrinth's
  generic ` (1)`/` (2)` collision suffix every time). Three pieces:

  1. **Rename resilience.** Instances were only ever tracked by a SQLite row (`id` + a `path`
     column pointing at the folder) — nothing on disk recorded which instance a folder actually
     was, so renaming or moving a folder outside the app silently orphaned it. Every instance
     folder now gets a small marker file, `.modrinth-studio-instance.json` (`{"id": "local:..."}`),
     written at creation time (`write_instance_marker()` in
     `packages/app-lib/src/state/instances/commands/reconcile_instance_paths.rs`, called from
     `create_instance.rs` — this covers duplication too, since duplication goes through the same
     creation path). On every app startup, `reconcile_instance_paths()` walks the instances
     directory, reads each folder's marker, and — if a folder's current name no longer matches
     what the database has for that id — updates the database's `path` column to match
     (`update_instance_path()` in `instance_rows.rs`) rather than the other way around. This runs
     once in `State::init()`, right before the existing folder watcher starts. A folder with no
     marker (pre-existing instances from before this fix) is left alone; it'll get a marker the
     next time something writes to it through the normal creation path, but existing instances
     aren't retroactively patched — only new ones are guaranteed to survive a rename from the
     start.
  2. **Storage computation.** `packages/app-lib/src/api/instance/storage.rs` (new file) walks every
     instance's folder and sums file sizes — iteratively (an explicit stack, not recursive
     `async fn`) to avoid `Box::pin` boilerplate and any stack-depth risk on a deeply nested
     folder, and skipping symlinks entirely (never followed, never counted) so shared/symlinked
     content can't be double-counted or loop forever. Exposed as `instance_storage_usage()`,
     wrapped as the `studio_instance_storage_usage` Tauri command in `studio.rs` (registered in
     `build.rs` like every other command here — see the three-places note above), and called from
     the frontend via `fetchInstanceStorageUsage()` in `use-studio-instance-storage.ts`, which also
     has `formatStorageSize()` for the binary/1024-based "1.4 GB" display format.
  3. **Storage settings page.** New `StorageSettings.vue` tab (Settings → Instances → Storage,
     `DatabaseIcon`), registered in `AppSettingsModal.vue`. Lists every instance sorted
     largest-first, each with a bar sized relative to the single largest instance (floored at a
     visible sliver so a tiny instance is still clickable) and an exact-size tooltip; clicking a
     row calls the existing `showInstanceInFolder()` helper to jump straight to that folder. The
     same page also holds a toggle — `showInstanceStorageUsage` in
     `use-studio-appearance.ts` (off by default, since the underlying measurement is a real
     folder walk) — that turns on a storage-size metadata item next to the total-playtime one on
     an instance's own page header (`pages/instance/components/page-header/index.vue` and its
     server-instance variant), fetched and shown only when the toggle is on.

  None of the new Rust here (the marker/reconciliation module, the storage walk, the new command)
  could be compile-checked in this environment — no `cargo` available — so give it a real
  `cargo check`/build before relying on it.

  **Follow-up fixes after this shipped:**

  - **Clicking the Storage tab froze the whole Settings modal** (couldn't close it, couldn't
    switch tabs). Root cause was a plain typo: `StorageSettings.vue`'s loading state rendered
    `formatMessage(commonMessages.loading)`, but that message is actually named
    `commonMessages.loadingLabel` — `commonMessages.loading` is `undefined`, and `formatMessage()`
    on `undefined` throws. Since that's the very first thing the component renders (the loading
    state is the initial value of `loading`), the throw happened immediately on mount, mid-render,
    which is what took the whole modal's reactivity down with it rather than just failing quietly.
    Fixed by using the correct message name.
  - **The Storage page took ~15s to load with 20-30 instances.** `instance_storage_usage()` was
    walking each instance's folder one at a time in a loop — since each walk is I/O-bound (many
    small `read_dir`/`metadata` calls, not CPU work), doing them sequentially meant the total time
    was the *sum* of every instance's walk. Switched to `futures::future::join_all()` so every
    instance is walked concurrently instead; wall-clock time is now roughly the single slowest
    instance's walk, not the sum of all of them.
  - **The storage-next-to-playtime header value showed up late and inconsistently.** It was
    calling the same full-library `fetchInstanceStorageUsage()` the Storage page uses, just to
    read off the one entry it needed — so opening any instance page paid for walking *every*
    instance's folder, and how long that took (and therefore how long the header stayed blank)
    depended on the size of the whole library, not just the instance actually being viewed. Added
    a dedicated single-instance path: `instance_storage_usage_single(instance_id)` on the Rust side
    (looks up just that one instance via `get_instance()` and walks only its folder), wrapped as
    the `studio_instance_storage_usage_single` command, called via
    `fetchInstanceStorageUsageSingle()` in `use-studio-instance-storage.ts`. That same file also
    added a small in-memory `Map<instanceId, bytes>` cache — shared across both the header and the
    Storage page (the full-list fetch populates it too) — read synchronously via
    `getCachedInstanceStorageUsage()` so a previously-seen size shows instantly while a fresh walk
    updates it in the background, instead of the header going blank on every navigation.
  - **The relative bar wasn't self-explanatory** ("i dont understand what u mean with the bar
    underneath it"). Reworded the Storage page's description text to spell out what bar length
    means (relative to your single largest instance, not an absolute percentage of disk) instead
    of relying on it being obvious from the UI alone.
  - **"modrinth now takes ages to load anything"** — a second, more severe performance bug in
    `reconcile_instance_paths()` itself, separate from the storage-walk one above: it checked
    every tracked instance's folder one at a time (a `metadata()` call, then a marker read, then —
    every single boot, even when nothing had changed — a marker *write*), each fully awaited
    before moving to the next. With 20-30 instances that's 20-30 sequential disk round trips on
    literally every app launch. Fixed the same way as the storage walk: every instance's check now
    runs concurrently via `join_all()`, and the marker file is only actually written the first
    time (or if it's ever found to be wrong) — an instance that already has a correct marker costs
    one cheap read, not a read *and* a write, on every subsequent boot.
  - **The other half of the rename request** — re-reading the original ask, "when i rename it in
    modrinth it renames the folder" was about the *opposite* direction from what shipped above:
    not "renaming the folder in Explorer shouldn't break Modrinth" (reconciliation, already done),
    but "renaming the instance's Name field in Settings should rename the actual folder to match."
    That genuinely wasn't happening before — `edit_instance()` only ever touched the `name` column,
    leaving the folder's name permanently stuck at whatever it was given at creation. Added
    `rename_instance_folder_for_name_change()` (new file,
    `state/instances/commands/rename_instance_folder.rs`), called from `api/instance/lifecycle.rs`'s
    `edit()` right before the existing `edit_instance()` call whenever `patch.name` is set. It
    reuses `create_instance.rs`'s own `resolve_instance_path()` (widened from private to
    `pub(crate)`) for the exact same sanitize + " (1)"-style collision-avoidance logic a brand new
    instance's folder gets, so a renamed folder can never collide with or overwrite another
    instance's. Refuses (returns an error, touches nothing) if the instance is currently running —
    never rename a live instance's folder out from under it.

    This also explains a separate complaint ("i cant rename anything while the modrinth app is
    open... not even in the same instance tab"): on Windows, `watch_instance_folder()` opens a
    directory watch handle on *every* tracked instance's folder for as long as the app runs
    (`watch_instances_init()` watches the whole library at startup, not just whatever's currently
    open) — an open watch handle is by itself often enough for Windows to refuse a rename,
    regardless of which page has focus. Added a matching `unwatch_instance_folder()` in
    `watcher.rs`; the new rename function drops the watch on the old folder first, does the actual
    `crate::util::io::rename_or_move()`, updates the `path` column via `update_instance_path()`,
    then re-watches the new location — re-establishing the old watch again if the rename itself
    failed, so a failed attempt doesn't also silently kill live-update tracking until next launch.
    This only fixes renaming *through the app*; an external rename in Explorer while the app is
    running still hits the same lock, since that path was never something this app's own code
    could sidestep — closing the app first remains the workaround there.

    The frontend side needed one more fix once this was wired up: `general-settings.vue`'s Name
    field watcher used to call `edit_instance()` on every single keystroke, which was harmless
    when that only wrote a DB column but would now trigger a real filesystem rename dozens of
    times while someone is mid-typing. Debounced the save (1s after typing stops, flushed
    immediately on unmount so a change isn't dropped by closing the modal mid-debounce) rather than
    firing on every keystroke.

    **This one is meaningfully riskier than everything else in this section** — it's the first
    Modrinth Studios change that renames an existing folder full of real, potentially irreplaceable
    save data, rather than only adding new files or reading existing ones. It could not be
    compile-checked here (no `cargo`), and unlike the read-only reconciliation/storage-walk code,
    a bug here has real consequences if wrong. Test it on a low-stakes instance first before
    trusting it on anything that matters.

    **First real-world test hit `Access is denied (os error 5)`** on every attempt, regardless of
    the target name — a different failure than the sharing-violation-style lock this was written
    against. Two changes: (1) `edit_instance()` returning a plain `Result<(), Error>` while
    `crate::util::io::rename_or_move()` returns an `eyre`-based `Result` needed an explicit
    `.into()` at the error site (a real compile error, since none of this could be checked before
    landing) — fixed. (2) Added a short retry loop (up to 5 attempts, increasing delay) around the
    actual rename call, on the theory that `unwatch_instance_folder()`'s call to stop the OS-level
    watch may not release its directory handle the exact instant it returns, so a rename attempted
    immediately after can still lose that race. If Access Denied persists even with retries, that
    points at something outside this code entirely — most commonly Windows' Controlled Folder
    Access (Windows Security → Virus & threat protection → Ransomware protection) blocking an
    unlisted app from touching files in a folder it protects, or third-party antivirus doing the
    same — worth ruling out by testing whether the exact same rename also fails in Explorer with
    Modrinth Studio completely closed.

    **Resolved** — confirmed working end to end after a full rebuild: renaming the Name field in
    Settings now renames the folder live while the app is running, and renaming the folder in
    Explorer (app closed) shows up as both the new path and the new display name on next launch
    (see `update_instance_path_and_name()` above). The retry-with-backoff was very likely the
    actual fix; the earlier failures were probably hitting a binary from a partial rebuild, before
    both the `.into()` compile fix and the retry logic had landed together.

    **A second, more damaging bug in the same feature turned up later: it could misfire and rename
    a brand new modpack instance out from under its own installation, failing the install
    entirely** ("i tried downloading a modpack... it loaded for quite a while but after it finished
    it just said no content", reproduced from an install-job error report showing `Failed while
    resolving pack (internal_error): moving "Iron Frost (1)" to "Iron Frost (2)" ... Access is
    denied (os error 5)`, at `packages/app-lib/src/util/io.rs:248`). Installing a modpack creates
    the instance first (landing on whatever path `resolve_instance_path()` picks — `"Iron Frost"`,
    or `"Iron Frost (1)"` if that plain name was already taken by something else), then, once the
    pack's manifest is read, calls `edit()` with `name: Some("Iron Frost")` to apply its real
    title — the exact same name the instance was already just given. `edit()`'s
    rename-folder-on-name-change logic (added just above) re-runs `resolve_instance_path()` to
    figure out the target folder, but that function's collision check (`path_available()`) had no
    way to know "the folder that's already sitting at this exact candidate path is this very
    instance's own current folder" — it just saw the folder physically existing and treated it as
    taken, same as it would for any unrelated instance. So it always walked past the instance's own
    correct path to the *next* one (`"Iron Frost (1)"` → `"Iron Frost (2)"`) and tried to rename
    into it — a completely unnecessary move, since the name hadn't actually changed. Usually this
    was merely wasteful; here it lost the race against the instance folder still being busy from
    the install itself (the freshly-created folder's file watcher, and/or the pack extraction still
    finishing up), Windows refused the rename with Access Denied, and — per this feature's own
    "never let the name and the folder end up disagreeing" design — `edit()` propagated that as a
    hard failure rather than silently keeping the mismatched name, which aborted the whole install
    and rolled the instance back. Whatever "no content" the person saw was never actually a content
    tab rendering issue — the instance behind it had already been deleted by the rollback by the
    time they looked.
    
    Fixed at the root: `resolve_instance_path()` (and its `path_available()` helper) now take an
    `exclude_instance_id: Option<&str>`, checked against the database (not the filesystem) first —
    a path already owned by that exact instance is always available *to it*, regardless of what
    physically exists there, since resolving back to a no-op is exactly the case that needs to work.
    `create_instance()` passes `None` (a brand new instance can never collide with itself);
    `rename_instance_folder_for_name_change()` passes the instance being renamed, so an edit that
    doesn't actually change the effective name now correctly resolves back to that instance's
    existing path and short-circuits via the pre-existing `if new_path == instance.path` check,
    without ever attempting a rename — not "a rename that now succeeds," but no rename attempted at
    all, which is the actually-correct outcome here. This could not be compile-checked in this
    environment either; worth confirming with the exact repro (install a fresh modpack whose title
    matches, or resolves via `resolve_instance_path`'s collision logic to, the name the instance was
    already created with) before trusting it.

- **Launching the same instance twice, and stopping one window at a time**: the Play button only
  ever launches one window of an instance — clicking it again while already running is a no-op
  (`launch_minecraft()` in `packages/app-lib/src/launcher/mod.rs` rejects any launch while a
  process already exists for that instance). Added `allow_multiple: bool`, threaded end-to-end
  from a new "Launch another instance" entry in the instance page's `...` overflow menu, down
  through `instance::run()` → `run_credentials()` → `launch_minecraft()`, where it's the one thing
  that bypasses that existing-process check. `RpcServerBuilder` is already per-process/ephemeral-
  port scoped, so two concurrent launches of the same instance don't collide — no other backend
  change was needed. The Tauri command (`instance_run`) and the JS `run()` wrapper both default
  this to `false`, so every existing call site is unaffected; only the new menu action passes
  `true`.

  On the frontend, the instance page's Stop button becomes a "Stop all" split-button with a
  dropdown (reusing the same `SplitButton`/`OverflowMenuOption[]` pattern already used for the
  server-instance Play button) once more than one process is running for the instance; the
  dropdown lists each running window by relative start time and stops just that one via the
  already-existing `process_kill(uuid)` command (distinct from `instance_kill(instanceId)`, which
  stops all of them — used by "Stop all" and by the plain single-process Stop button). This needed
  no new backend work, only wiring `layout.vue`'s process list down to `page-header/index.vue`.

  One real bug came up while wiring this: the process-list cache was being *overwritten* with a
  `[true]` placeholder (for instant "now playing" UI feedback, ahead of the backend's `launched`
  event confirming the real process list) both right after calling `run()` and again when that
  `launched` event arrived. With two concurrent launches, the second launch's placeholder would
  silently wipe the first launch's entry out of the cache, undercounting how many windows were
  actually running. Fixed by (1) appending to the existing cached array instead of replacing it
  for the optimistic placeholder, and (2) having the `launched` event handler invalidate/refetch
  the real list instead of writing another placeholder, since by the time that event fires the
  process really is registered and a refetch will return the true, complete list.

  Like the rest of this session's Rust changes, none of this could be compile-checked here (no
  `cargo` in this environment) — worth a `cargo check`/`turbo run dev` before relying on it.

  **First `turbo run dev` caught two real compile errors**, both call sites of
  `instance::run()`/`launch_minecraft()` that existed before this feature and were missed when
  `allow_multiple` was added to their signatures: `apps/app/src/api/worlds.rs`'s
  `start_join_singleplayer_world`/`start_join_server` (the "join a world/server from the Worlds
  tab" commands, unrelated to the instance page) were still calling the old 2-argument
  `instance::run()`. Fixed by passing `false` at both call sites, same as every other pre-existing
  launch path. A useful reminder that "add a parameter to a function with multiple call sites"
  needs a project-wide search for that function's name, not just updating the call sites this
  feature happened to touch directly.

- **Shared Minecraft folders — removed.** This fork briefly had a NoRiskClient-style "shared
  folder" feature (an instance could opt into sharing its worlds/config/resource packs/options.txt
  with other instances via junctions/symlinks). Modrinth added their own official shared-folder
  functionality upstream, so this fork's version was removed entirely rather than maintained
  alongside a duplicate/conflicting implementation — back to stock per-instance folders, same as
  before this feature ever existed.

  Removing it safely (without silently deleting anyone's only copy of a world or config that had
  come to live only inside a `shared_profiles/<id>/` folder) was the actual hard part. A one-shot
  migration can't be trusted alone here — if it fails partway, is skipped, or runs against a
  database from a friend's install that saw a different history of this feature, the safety
  property has to hold regardless. So instead of a migration, `state::shared_folder_reversion`
  (`packages/app-lib/src/state/shared_folder_reversion.rs`) runs on every app launch, unconditionally,
  and always re-checks real disk/DB state rather than trusting any previous run succeeded ("never
  trust an attempt, only check reality"): for every instance that was still linked to a shared
  folder, it copies each shared item's real current files back into that instance's own local
  folder (never leaves an instance with missing saves), removes the link, and only once it has
  independently reconfirmed *zero* instances still reference a given shared folder does it delete
  that now-orphaned `shared_profiles/<id>/` directory. All the feature's other touch points were
  removed alongside it: the `shared_profile` Tauri plugin/API (backend and frontend), the General
  settings UI, the Storage page's "Shared folders" section and its usage-accounting backend, and the
  3 shared-profile-specific entries in `STUDIO_MIGRATIONS` (safe to drop since the reversion routine
  above never depended on that list — it queries `sqlite_master` directly).

- **Fixed a "ghost" instance card lingering in the Library after deletion.** Deleting an instance
  (`state::remove_instance()` in `packages/app-lib/src/state/instances/commands/remove_instance.rs`)
  always deleted the database row first, then removed the instance's folder from disk
  (`io::remove_dir_all`), and only *then* did `lifecycle.rs::remove()` emit the `instance` `Removed`
  event the frontend listens for to refetch and drop the card from the Library
  (`Index.vue`'s `useAppEvent('instance', fetchInstances)`). If the folder removal failed — most
  plausibly a file inside it (a log, a world's `session.lock`, a `.jar`) still briefly held open by
  a just-exited game process, antivirus scan, or search indexer, right as the person deletes the
  instance — the whole `remove()` call returned an error via `?` *before* ever reaching that emit.
  The database row was already gone by that point (deletion can't be rolled back after the fact),
  so the instance was really and permanently removed from the app's perspective, but the frontend
  was never told — leaving its card sitting in the Library, with clicking it throwing "Unknown
  instance" (since that's genuinely true), until some unrelated event happened to trigger a refetch.
  Fixed by no longer letting a folder-removal failure block the removal from being reported as
  successful: `remove_instance()` now retries removing the folder a handful of times with a short
  backoff (absorbs the common transient-lock case), and if it's still stuck after that, logs a
  warning and moves on rather than propagating the error — the instance is unconditionally gone from
  the library's perspective either way, so the event now always fires promptly. The one trade-off:
  in that persistent-lock edge case, the instance's folder can be left behind on disk (logged, so
  it's at least discoverable) rather than the app pretending the delete never happened.

- **Found and fixed the real cause of card dragging (and some group accordions) going dead.**
  After the reorder attempt below was reverted with no change, the person pulled the DevTools
  console, which had the actual answer the whole time: an uncaught `TypeError: Cannot read
  properties of undefined (reading 'toLowerCase')` inside `@dnd-kit/dom`'s own `isFocusable`
  accessibility check, thrown from `Draggable.set [as element]` — i.e. from dnd-kit's *own* internal
  code reacting to a card's draggable element being (re)assigned, not from anything Studio wrote.
  `instance-card.vue` (identical to upstream Modrinth's own code — confirmed by diffing directly
  against `origin/main`) hands `useDraggable` a **Vue component ref** (`ref="instanceCardElement"`
  on `<InstanceCardView>`); `@dnd-kit/vue` resolves that down to `.$el` internally via its own
  `unrefElement` helper, which — if `.$el` isn't a real mounted DOM node at the exact instant it's
  read (a brief window around mount, a `KeepAlive` reactivation, a hot-reload) — falls back to
  handing back the raw component instance object instead of `undefined`. dnd-kit's accessibility
  check then tries to read `.tagName` off that non-element object and throws. Being an *uncaught*
  error inside dnd-kit's own reactive effect system, it doesn't stay contained to just that one
  card: it showed up cascading into `Unhandled error during execution of app errorHandler` and
  `Cannot read properties of null (reading 'parentNode')` a moment later, in the same component
  subtree — consistent with some group accordions in the same render tree also going unresponsive
  to clicks, not just dragging failing.

  First fix in `instance-card.vue`: added `instanceCardDomElement`, a computed that does the `.$el`
  extraction itself and only hands it to `useDraggable`'s `element` if it looks like a real DOM node,
  `undefined` otherwise (checking `el.nodeType === 1` rather than `instanceof HTMLElement` — the
  latter tests the prototype chain against *this exact script context's* `HTMLElement` global, and a
  perfectly real DOM node handed across a realm boundary can fail that check despite being completely
  valid; `nodeType` is a plain data property every real node has regardless of which realm built it).
  This closed the original crash, but dragging *still* didn't work afterward, with a totally clean
  console — no error at all, on every card, in every test. That absence of any error was itself the
  clue: a real failed drag attempt should show *something*.

  Tracked it down with temporary logging (added directly to `instanceCardDomElement` and to
  `instance-group-dnd.vue`'s `handleDragStart`, both removed once the cause was found) that printed
  what `.$el` — and Vue's own internal `$.subTree.el`, which `.$el` is supposed to mirror — actually
  resolved to for every card. Both were consistently an **empty Text node**, never the real `<div>`.
  That meant Vue itself was compiling `instance-card-view.vue`'s template with more than one root node
  (turning it into a Fragment component) and picking the wrong one as `.el` — not a `.$el`-resolution
  quirk on `instance-card.vue`'s end at all, which is why hardening the extraction there could never
  have fixed it. The culprit: a long multi-line HTML comment sitting directly inside `<template>`,
  immediately above the root `<div>` (documenting the `transition-all` → narrowed-transition fix from
  task #53). Something about a comment in exactly that position — long, immediately pre-root, with
  blank-line-sensitive indentation — was compiling into a real (if empty) sibling node ahead of the
  div, making the div no longer the template's sole root. `instance-card.vue`'s `useDraggable` was
  handing dnd-kit that placeholder instead of the actual card element on every single card, every
  time — with no error, since a placeholder text node is still a valid-enough JS object to pass
  around silently, it just isn't draggable. This explains the whole saga end to end: identical to
  upstream's *drag* code because it genuinely was, "worked in the last pushed version" because that
  comment (or its current long-and-multi-line form) was introduced after that push, and no crash on
  the second try because the fix there was solving a real but different bug.

  Fixed by moving that entire comment out of `<template>` into `<script setup>` (documenting the
  `transition-[...]` class on the div instead of floating above it) — a `<script>`-level comment can
  never affect the compiled template, regardless of what the actual Vue/compiler-level trigger for
  this turns out to be. The `instanceof` → `nodeType` hardening in `instance-card.vue` stays; it's a
  real, separate fix for a real crash, independent of this. **Confirmed working by the person.**

- **Tried adding drag-to-reorder cards; reverted.** A new "Custom order" sort mode plus a
  `vuedraggable`-based per-card reorder was added to `use-library.ts`/`instance-group/index.vue`/
  `instance-card.vue`/`sort-menu.vue`, alongside a further tweak to the existing `@dnd-kit/vue`
  drag-onto-a-group feature's `disabled` conditions. Reported as still not draggable after that
  change, and removed again at the person's request rather than continuing to debug blind — all of
  it (the new sort mode, the `customOrder` storage, the `Draggable` wiring, and the `disabled`
  tweaks) is fully reverted; those two files' drag behavior is back to exactly what it was before
  this was attempted. If card dragging/reordering is revisited, it's worth first confirming in the
  running app *why* the existing drag-onto-a-group feature doesn't respond at all — e.g. whether
  `displayState.value.group` is actually `'Group'` for this person, or whether pointer events are
  being swallowed by something else entirely — before building more on top of a system that may
  itself not be firing.

- **Fixed Studio breaking the official Modrinth app's database migrations.** Studio never changed
  the shared Tauri `identifier` (`ModrinthApp` in `apps/app/tauri.conf.json`), so it has always
  used the exact same AppData folder — and therefore the exact same `app.db` SQLite file — as the
  official, unmodified Modrinth app. That's intentional (it's what lets both apps see the same
  instances), but it meant Studio's own extra schema changes (shared folders, playtime correction,
  the sidebar-default tweak) were being applied through `sqlx::migrate!()`, the same mechanism
  upstream's own migrations use, and recorded in the same `_sqlx_migrations` tracking table inside
  that shared database. The official app's own compiled binary only knows about upstream's
  migrations, so the moment it opened a database that also had Studio's migration versions recorded
  in that table, it refused to start at all, with "Error while applying migrations: migration ...
  was previously applied but is missing in the resolved migrations."

  Fixed by moving Studio's 5 own `.sql` files out of `packages/app-lib/migrations/` (the folder
  `sqlx::migrate!()` embeds and tracks) into a new `packages/app-lib/studio-migrations/` folder, and
  adding `packages/app-lib/src/state/studio_migrations.rs` to apply them itself, tracked in a
  separate `studio_migrations` table that only Studio's own code ever looks at. The official app
  doesn't care that an extra, unrecognized table exists in the database, so it's completely
  invisible to it — `_sqlx_migrations` now only ever contains rows both apps recognize. For anyone
  upgrading an existing Studio install (who already has these 5 versions recorded as successful
  under the old scheme, with their tables/columns/data already in place), the new code detects that
  on first launch, copies the "already applied" fact over into `studio_migrations` without
  re-running any SQL, and deletes the now-redundant rows from `_sqlx_migrations` — clearing the
  official app's error the next time it's opened. Kept everything on one shared AppData folder per
  the person's request, rather than splitting Studio onto its own separate folder/identifier.

  Caveat: like all Rust changes this session, this could not be compiled or run in this sandbox (no
  Rust toolchain available here), and this fix went through two real, person-found bugs before it
  actually worked:

  1. **Compile error.** The first version called `sqlx::raw_sql(sql).execute(&mut *tx)`, which
     failed to build with `cargo` reporting "implementation of `sqlx_core::executor::Executor` is
     not general enough" on two unrelated `#[tauri::command]` functions elsewhere in the crate — a
     known `sqlx::raw_sql` limitation
     ([launchbadge/sqlx#3581](https://github.com/launchbadge/sqlx/issues/3581)): its `execute()`
     lacks a lifetime bound `sqlx::query()`'s has, which the `tauri::command` macro's generated
     futures need. Fixed by calling it the other way around, `tx.execute(sqlx::raw_sql(sql))`
     instead — functionally identical, resolves the trait bound the way `query()` already does.

  2. **Runtime error, once it compiled — against Studio itself, not the official app.** Studio's own
     dev build immediately hit the same "migration ... was previously applied but is missing in the
     resolved migrations" error this whole fix exists to solve. Cause: `sqlx::migrate!().run(&pool)`
     validates *before applying anything new* that every row already in `_sqlx_migrations`
     corresponds to a migration it has resolved — and the legacy-row cleanup was running *after*
     that call, too late to matter, since Studio's own `sqlx::migrate!()` no longer resolves its 5
     migrations either (they moved out of the tracked folder). Fixed by splitting
     `studio_migrations.rs` into `reconcile_legacy_rows()` (strips Studio's versions out of
     `_sqlx_migrations`, called **before** `sqlx::migrate!()`) and `apply_pending()` (applies
     anything genuinely new, called **after**, since it needs upstream's tables to already exist).

  3. **Second runtime error, a different migration version, same class of bug — the other
     direction.** After fix #2, Studio's dev build failed again, now over migration
     `20260819120000` — one of 3 upstream migrations
     (`instance-synced-options`/`screenshot-groups`/`screenshot-editor-state`) that the *official*
     Modrinth App (apparently updated more recently than Studio's own fork point) has already
     applied to the shared database, but that Studio's own checked-out
     `packages/app-lib/migrations/` doesn't have yet — confirmed by diffing against `origin/main`.
     Two independently-versioned binaries sharing one `_sqlx_migrations` table means either one can
     end up "ahead" of the other at any time, in either direction, indefinitely — this isn't a
     one-time fix-up, it'll keep happening as both Modrinth and this fork keep shipping. Fixed with
     sqlx's own built-in support for exactly this: `Migrator::set_ignore_missing(true)` (a
     documented option for "multiple applications sharing the same database"), so Studio's own
     `sqlx::migrate!()` tolerates rows it doesn't resolve instead of refusing to start. This doesn't
     replace `reconcile_legacy_rows`/`apply_pending` — `ignore_missing` only helps *this* binary
     tolerate rows it doesn't know about; it can't make the official app's own unmodified,
     non-ignoring binary tolerate Studio's 5 extra rows, which is still what that pair of functions
     handles.

  **Confirmed working by the person** — both Studio's own dev build and the separate official
  Modrinth App open cleanly against the shared database now. Still worth periodically pulling
  upstream's newer migrations into Studio's own `packages/app-lib/migrations/` when convenient —
  `ignore_missing` prevents a hard crash, but Studio staying badly behind means its own schema won't
  actually have upstream's newer columns/tables until it catches up.

- **Fixed a scary "An error occurred" dialog on launch for a specific modpack, even though the game
  launched fine.** Reported by one of the person's friends: launching a particular modpack always
  showed a blocking I/O error dialog ("The system cannot find the path specified (os error 3)",
  pointing at that instance's `logs` folder), with no other feedback that anything was happening —
  but the game opened anyway a moment later. Cause, in `packages/app-lib/src/state/process.rs`'s
  `insert_new_process`: the Minecraft process is spawned first, but the very next thing that
  function did was create the instance's `logs` folder and open `launcher_log.txt` for the in-app
  Logs tab — and any I/O failure there (here, seemingly an instance folder name Windows can't
  resolve a sub-path under, likely a trailing space) `?`-propagated straight out of the function.
  Since the actual game process was already running by that point, the failure never stopped the
  launch — it just meant Studio's own process tracking (registering it in `process_manager`, the
  `Launched` event, Discord RPC, etc.) got skipped entirely, on top of showing an alarming dialog for
  something that was never fatal. Fixed by making the logs-folder/log-file setup best-effort — a
  failure now just logs a warning and the function continues on to register the process normally,
  matching how every other log-write failure in this same file (elsewhere in `process.rs`) was
  already handled. Didn't chase the underlying "why does Windows reject this specific path" question
  (folder-name sanitization on instance/modpack creation is upstream, untouched-by-Studio code, and
  this only ever surfaced for one specific pack) — this fix removes the actual harm (broken process
  tracking, scary dialog) regardless of that root cause. Not build-verified in this sandbox, same
  caveat as all Rust changes this session.

- **Customizable Discord Rich Presence.** Stock Modrinth App always shows a fixed "Playing
  &lt;instance name&gt;" Discord status with no way to change it. Studio now lets the person pick
  from Settings → Privacy → Discord Rich Presence → Customize:
  - **Mode**: `Default` (unchanged upstream behavior), `Detailed` (adds a second line with the
    loader + Minecraft version, e.g. "fabric 1.21.11"), `Minimal` (just "Playing Minecraft", hides
    which modpack), or `Custom` (their own text for both lines, with `{instance}`/`{loader}`/
    `{version}` placeholders — an empty template falls back to that line's `Default` text).
  - **Activity verb** (Playing/Listening to/Watching/Competing in — cosmetic, changes the word
    Discord shows before the status).
  - Toggle for showing elapsed time.
  - Custom idle text, shown whenever nothing is running.
  - Up to 2 custom buttons (label + URL) on the Discord activity card.

  Deliberately **does not** offer a custom large/small image — that would need a Rich Presence art
  asset key already uploaded to *Modrinth's own* Discord application (the real, shared app ID this
  client always authenticates as), which Studio has no access to upload to; setting an unrecognized
  key just silently shows nothing, so it's left out rather than offered as a setting that looks
  broken.

  Implementation: a new `studio_discord_rpc_settings` singleton table (own migration, same
  SQLX_OFFLINE-driven reason `studio_playtime_corrections`/`studio_shared_profiles` avoid touching
  upstream-queried tables) backing `packages/app-lib/src/api/discord_rpc.rs`. `state/discord.rs`'s
  `DiscordGuard` was rewritten around a shared `apply_activity` builder, replacing the old fixed
  `set_activity`/`force_set_activity` calls with settings-aware `set_instance_activity` (called from
  `launcher/mod.rs` at launch, with loader/version already on hand there) and `set_idle`;
  `clear_to_default` (called on app startup and right after the person saves new settings) now looks
  up loader/version fresh via a join against `instance_content_sets` and reuses the running
  process's actual `start_time` for accurate elapsed time instead of resetting the clock. Exposed to
  the frontend via a `discord-rpc` Tauri plugin (`apps/app/src/api/discord_rpc.rs`) and
  `helpers/discord-rpc.ts`; UI lives in
  `components/ui/settings/account/DiscordRpcSettingsModal.vue`. Not build-verified in this sandbox,
  same caveat as all Rust changes this session.

- **Storage page redesign: a Steam-style system overview, per-instance composition bars, and
  in-app instance actions.** The Settings > Storage page used to be just a list of instances with a
  bar scaled relative to the largest one, and clicking a row opened its folder in Explorer. Redesigned
  after feedback that it should look more like Steam's own storage manager:
  - A new overview bar at the top of the page segments the *whole drive* Studio's data lives on into
    Modrinth instances, shared folders, worlds, resource packs, shaders, mods, replays, everything
    else on that drive ("Non-Modrinth"), and free space — with a legend showing each category's exact
    size. `packages/app-lib/src/api/instance/storage.rs`'s new `system_storage_overview()` gets the
    drive's total/free space via `sysinfo::Disks` (the same lookup `state::dirs`'s `get_disk_usage`
    already used elsewhere in this fork) and derives "Non-Modrinth" as a remainder
    (`total − free − everything Modrinth accounts for`) rather than actually walking the rest of the
    drive.
  - Each instance's own bar is no longer sized relative to the biggest instance — it's now a
    composition bar of *that instance's own* categories (worlds/resourcepacks/shaders/mods/replays/
    everything else), reusing a new `StorageBreakdown` the per-instance walk (`categorize_instance_dir`)
    now returns alongside the existing total. Categories are recognized by their standard, unambiguous
    top-level folder name (`saves`, `resourcepacks`, `shaderpacks`, `mods`, `replay_recordings`/
    `flashback`) — same symlink/junction skip as the existing walk, so a shared folder's data is never
    miscounted as instance-owned. The same six colors are reused between the overview bar and every
    instance's own bar, so a color always means the same category no matter which bar it's in.
  - Clicking an instance row now opens the instance itself (matching how every other instance list in
    the app behaves) instead of opening its folder in Explorer. A "..." menu (same
    `TeleportOverflowMenu` component and card-click pattern the Library's own instance cards already
    use — a `SmartClickable` wrapping an invisible `router-link` plus a `data-no-card-click` region for
    the menu button) now holds Show in explorer, Instance settings, and Delete. "Instance settings"
    deep-links into the instance page via a `?studioOpenSettings=1` query param that
    `pages/instance/layout.vue` watches for once the instance loads, opens the settings modal, then
    strips the param back out of the URL so a refresh doesn't reopen it. Delete reuses the existing
    `ConfirmDeleteInstanceModal`.

  Backend: `StorageBreakdown`/`SystemStorageOverview` structs and `system_storage_overview()` added to
  `packages/app-lib/src/api/instance/storage.rs`, re-exported from `api/instance.rs`, wrapped by a new
  `studio_system_storage_overview` Tauri command in `apps/app/src/api/studio.rs` (registered in both
  its `init()` and `build.rs`'s `InlinedPlugin` list, same as every other Studio command). Frontend:
  `fetchSystemStorageOverview()` in `use-studio-instance-storage.ts`; the page itself is entirely
  rewritten in `StorageSettings.vue`. Not build-verified in this sandbox, same caveat as all Rust
  changes this session.

- **Upstream base commit shown in Settings.** The official Modrinth App doesn't carry a real
  version number in its own source — `apps/app-frontend/package.json`, `apps/app/Cargo.toml`, and
  `packages/app-lib/Cargo.toml` all read the placeholder `1.0.0-local` upstream, with a real
  version only stamped in at Modrinth's own internal release-build time, and `modrinth.com/app`
  itself just labels it a rolling "Beta Release" with no version shown anywhere on the page. So
  rather than inventing a number to compare against, the Settings footer (below the existing
  "Modrinth Studio {version}" / OS line, in `AppSettingsModal.vue`) now shows the exact upstream
  commit this fork was branched from and last synced with: "Based on Modrinth App @ e33ef5f25
  (2026-08-25)", clickable via `openUrl()` (the same `@tauri-apps/plugin-opener` pattern
  `PrideFundraiserBanner.vue` already uses) straight to that commit on
  `github.com/modrinth/code`. `UPSTREAM_BASE_COMMIT`/`UPSTREAM_BASE_DATE` are two plain constants
  at the top of the component — bump them by hand whenever upstream is actually pulled into this
  fork (`git describe --tags --always <sha>` on the new base is an easy way to sanity-check which
  monorepo `v0.x.x` tag it lands closest to, though that tag versions the whole website+backend+app
  monorepo, not the desktop app specifically).

## Pulling in upstream changes

Studio last merged upstream at `v0.19.1` (commit `5d4759430`, 2026-08-27), 22 commits and ~495
files past the original fork point (`e33ef5f25`). Note `origin/main` and `origin/prod` are not
useful merge targets on their own — at least as of this merge, both had a history rewrite somewhere
past our fork point (`git merge-base --is-ancestor e33ef5f25 origin/main` returns false), so a
recent `v0.x.x` **tag** that *does* have our current base as an ancestor
(`git merge-base --is-ancestor <our current UPSTREAM_BASE_COMMIT> <tag>`) is the reliable way to
find the next real merge target, not just fetching the branch tip.

Ten files conflicted on that merge, all resolvable by hand without needing to redo any Studio
feature from scratch — worth knowing about since they'll likely conflict again next time too:

- **`AppSettingsModal.vue`, `App.vue`, `instance/index.ts`, `instance/layout.vue`,
  `page-header/index.vue`** conflicted only because Studio and upstream both added independent
  tabs/imports/computeds near the same lines (Replays next to upstream's new Screenshots tab,
  Discord RPC/Storage settings tabs next to upstream's Synced settings tab, etc.) — always safe to
  keep both sides, just watch tab ordering (upstream's Screenshots splices in at index 2, before
  Worlds; Replays pushes after Worlds, before Logs).
- **`InstanceItem.vue`**: upstream's "menu refactor" (#7307) rewrote this whole card to open a
  right-click `ContextMenu` instead of always-visible buttons, and renamed the shared option-list
  type from `OverflowMenuOption` to `ButtonMenuOption` app-wide (also hit `page-header/index.vue`'s
  Stop-All dropdown). Took upstream's structure wholesale; Studio's only actual change here was one
  color token (`color="green"` → `color="brand"` on the Play button, matching `WorldItem.vue`'s
  existing convention) — reapply that one-line swap onto whatever upstream's Play button looks like
  next time, don't try to keep Studio's old pre-refactor card markup.
- **`SplashScreen.vue`**: upstream made the splash screen properly light/dark theme-aware
  (`${theme.active}-mode`, its own light cube art and tint colors via new `--splash-tint-top/
  -bottom`, `--splash-overlay` variables in `variables.scss`). Studio still deliberately hardcodes
  `class="splash-screen dark"` (documented in the file) so the splash looks the same regardless of
  theme — `useTheme()` was intentionally not pulled in. Kept upstream's new CSS-variable structure
  (cleaner than the old hardcoded rgba gradient) but override `--splash-tint-top` back to
  `color-mix(in srgb, var(--color-brand) 45%, transparent)` on `.splash-screen.dark` so it still
  follows the accent color instead of upstream's plain green. If Studio ever wants a real light-mode
  splash, this hardcoding is the first thing to revisit.
- **`MultiSelect.vue`**: same line Studio touched for the focus-ring clipping fix (#77) also got
  touched by upstream's menu refactor, which moved the search input's spacing from the outer
  container's `py-1.5` onto the `Input` itself (`wrapper-class="grow m-2"`, 8px margin all around) —
  that already fixes the clipping on its own, so took upstream's side outright rather than doubling
  up padding.
- **`packages/app-lib/src/state/process.rs`**: NOT a real conflict — upstream's side of this hunk
  was byte-for-byte identical to the pre-fork base; the conflict only fired because Studio's own fix
  (#126, warn-and-continue instead of `?`-propagating launcher-log I/O errors so a locked log path
  can't block the actual game launch) touched adjacent lines. Kept Studio's side entirely.
- **`packages/app-lib/src/api/instance.rs`**: both sides just added a new `mod` line
  (`storage`/`synced_options`) right next to each other — kept both.

Also worth knowing: this merge revealed the official app dropped its old "Default game options" app
settings tab (`GameIcon`, `DefaultInstanceSettings.vue`) in favor of a new "Synced settings" tab
(`InstancesSyncedSettings.vue`) as part of a broader `instance-sync` feature (screenshots syncing
across instances, `synced_options` on the instance record) — that tab and its whole `context-menu/`
component folder were removed cleanly, no Studio code referenced either.

After merging, remember to bump `UPSTREAM_BASE_COMMIT`/`UPSTREAM_BASE_DATE` in
`AppSettingsModal.vue` to the new tag's commit — see the "Upstream base commit shown in Settings"
entry above.

### v0.20.0 merge (2026-09-08)

Merged again, this time up to the `v0.20.0` tag (commit `96ea36c72`) — 54 commits / ~757 files past
`v0.19.1`. `main` was fast-forwarded to the tag first (`git merge --ff-only v0.20.0`), then merged
into `studio` as a normal three-way merge.

This local clone turned out to be a **shallow** clone, which briefly looked exactly like the
"history rewrite past our fork point" situation the v0.19.1 notes above warn about —
`git merge-base --is-ancestor` was returning false-negatives for real ancestors. The actual fix was
`git fetch origin --unshallow`, not switching merge targets; worth checking `.git/shallow` first next
time a merge-base check looks wrong, before assuming a real rewrite happened.

Eight files conflicted, all resolved without redoing any Studio feature from scratch:

- **`App.vue`, `instance/layout.vue`** (first hunk): independent-addition conflicts — Studio and
  upstream both added imports/composables/queries near the same lines. Kept both sides.
- **`instance-group/index.vue`**: kept Studio's `onActivated`/`onDeactivated` resize-observer hooks
  alongside upstream's fuller `onMounted`/`onUnmounted`/`watch`, but took upstream's entire new
  virtualized, absolute-positioned `TransitionGroup` grid layout wholesale — it's a more complete fix
  for the exact resize-wobble bug class Studio's own simpler "drop `1fr`" fix (see #54 above) was
  working around, making that fix redundant now.
- **`instance-card-view.vue`**: kept Studio's wider `transition-[color,background-color,border-color,
  filter,transform]` list over upstream's narrower one — the `transform` and `color` are load-bearing
  for the click/drag scale and hover/selection color from fix #53 above.
- **`AppSettingsModal.vue`**: upstream moved every settings tab to `defineAsyncComponent` lazy
  loading (and added its own new `FeaturesSettings` tab, and moved `InstancesSyncedSettings`'s import
  to a directory index). Ported Studio's own `StorageSettings` tab into that same pattern so it keeps
  working, rather than leaving it as the one remaining static import.
- **`new-icon-editor-notification/apply-new-icons-modal.vue`** (modify/delete): upstream deleted the
  whole feature (and its caller) outright; Studio's only change here had been a rebrand string. Took
  the deletion.
- **`main.js`**: upstream replaced the old eager `Sentry.init`/`VueScanPlugin` boilerplate with a
  `setupErrorReporting()` helper and a lazily-`import()`ed `VueScanPlugin`. Took upstream's structure,
  keeping Studio's early `applyStudioAppearance`/`applyStudioWindowIcon`/`applyAccentIconTint`/
  `initializeStudioBackgroundRotation` calls ahead of app creation.
- **`instance/layout.vue`** (second hunk): upstream replaced the old sync-options-driven Screenshots
  tab visibility (a `globalSyncedOptionsQuery`-based `instanceTabs.splice(...)`) with simple per-tab
  `appSettings.showXTabInInstances` toggles for Files/Screenshots/Worlds. Took upstream's toggle-based
  approach, removed the now-dead sync-options query/logic, and re-inserted Studio's Replays tab
  addition in the same relative spot (after Worlds, before Logs).
- **`launcher/mod.rs`**: kept Studio's `allow_multiple` bypass around the running-process check;
  upstream's side was the same check without it.

Everything else auto-merged cleanly. Same as the v0.19.1 merge, none of this could be
compile-checked in this sandbox (no Rust toolchain, and running `pnpm install` against the real
Windows `node_modules` was avoided as risky) — a `cargo check`/frontend typecheck pass is still
needed before this ships.

## Modpack install download speed

"i just had to wait like 5 mins to download 200mb, i dont feel like thats normal even for my
internet" — for a modpack specifically (not a regular single mod/dependency install). Found a real,
concrete cause: `MODPACK_CONTENT_DOWNLOAD_CONCURRENCY` in
`packages/app-lib/src/api/pack/install_mrpack.rs` hardcoded a 4-file concurrency cap on a modpack's
content downloads (`loading_try_for_each_concurrent(..., Some(MODPACK_CONTENT_DOWNLOAD_CONCURRENCY),
...)`), independent of and stricter than the general "max concurrent downloads" setting
(`Settings::max_concurrent_downloads`, default 10 — the same one `state.fetch_semaphore` is sized
from) that everything else in the app already respects. Regular mod/dependency installs
(`api/instance/projects.rs`) have no such extra cap — they just use the shared semaphore directly —
so this was specific to modpacks. For a pack made of many small-to-medium files against a CDN where
per-connection throughput (not raw bandwidth) is the limiting factor — very common, since TLS/HTTP
overhead dominates a small file's transfer time — only 4 files in flight at once caps total install
speed well below what the connection could otherwise sustain, regardless of how fast that connection
actually is. Raised the constant to 10, matching the app's general default instead of silently
overriding it with a stricter one.

Worth knowing this isn't wired to the *live* setting — it's still a plain constant, not a read of
`Settings::max_concurrent_downloads` at install time — so someone who's turned their own
concurrent-downloads preference below 10 in Settings won't have that respected here. Threading the
real value down to `install_mrpack.rs` would be the correct long-term fix if that gap ever matters;
raising the constant was the pragmatic fix for the immediate complaint. Also worth knowing this
can't fully rule out the person's own connection or Modrinth's CDN being part of the story too —
this fixes a real, confirmed app-side ceiling, but doesn't guarantee it's the *only* factor if
downloads are still slow after this change.

**Turned out the concurrency cap wasn't the dominant factor in the specific "5 min vs 20 sec"
comparison that prompted this.** The person had been comparing the official Modrinth App against
Studio launched via `pnpm tauri dev` — a dev build, compiled fully unoptimized (no LTO, no
optimizations, debug assertions on). CPU-bound work done per downloaded file — SHA1 verification,
manifest/version JSON parsing, zip extraction — runs 10-50x slower than a release build under that,
which alone accounts for a gap that size with no code bug involved. Confirmed once they tried an
actual `pnpm tauri build` output instead: much faster, as expected. The concurrency fix above is
still real and worth keeping (it's a genuine ceiling below the app's own general default, regardless
of build type), but it was not what produced the dramatic gap in that particular comparison — that
was purely dev-vs-release. **Lesson for next time a performance complaint comes in: ask how the
build was run before assuming the reported number reflects real-world (release-build) performance.**
That build hadn't been made yet with any of this session's fixes in it (the shared-folder redesign,
the modpack-install rename fix just above, or this concurrency bump) — a fresh release build is
needed to actually test all three together before pushing.

## Replays tab freeze (Screenshots thumbnail work reverted)

**Replays tab freezing for ~5 seconds on open, on an instance with ~1000 recordings.** Root cause
was two things compounding: `replays/index.vue` rendered every entry in `filteredReplays`
unconditionally via a plain `v-for` — no virtualization at all — and each `ReplayItem` row
independently calls `getReplayThumbnail(...)` inside its own `onMounted()` hook, which on the Rust
side (`get_replay_thumbnail` in `replays.rs`) does a `spawn_blocking` zip read to extract an
embedded thumbnail. With ~1000 replays that meant mounting all ~1000 rows at once and firing all
~1000 of their thumbnail-fetch IPC calls simultaneously — that's what froze the app.

Fixed by virtualizing the list using the existing `useVirtualScroll`/`useScrollViewport` composable
(`packages/ui/src/composables/virtual-scroll.ts` — the same generic, fixed-item-height virtualizer
`Table.vue` already uses, applied here to a plain flex list instead of a table). `replays/index.vue`
now renders only the rows within (plus a buffer around) the viewport, so only a bounded number of
rows are ever mounted at once regardless of how many replays an instance has, and it can never fire
more than a small, fixed number of concurrent thumbnail fetches.

Two more real, Replays-specific fixes came out of the same investigation:

- **`list_replays` had an unbounded-concurrency problem on the backend**, separate from the frontend
  virtualization above: it fired one `spawn_blocking` per replay to read zip metadata with no limit
  on how many could race at once — a real disk/CPU burst on its own, before any row ever renders.
  Fixed with `REPLAY_METADATA_READ_CONCURRENCY = 16` and `futures::stream::iter(...)
  .buffer_unordered(16)` in `packages/app-lib/src/api/replays.rs`, the same bounded-concurrency
  pattern used for the modpack download concurrency fix.
- **`decoding="async"` added to `ReplayItem.vue`'s thumbnail `<img>`**, so decode work never
  competes with the scroll gesture.

Two more fixes, found during this same investigation, are generic and benefit every virtualized list
in the app (Replays included), so they're still in place even though they were originally motivated
by chasing scroll-jank on the Screenshots tab:

- **Scroll handler throttled to `requestAnimationFrame`** in `useScrollViewport`
  (`packages/ui/src/composables/virtual-scroll.ts`) — the native `scroll` event can fire many times
  per animation frame during a fast scroll, and each firing did a layout-forcing
  `getBoundingClientRect()` read plus a Vue ref write; coalescing to once per frame matches the
  browser's own paint cadence.
- **`overflow-anchor: none` applied directly to the real scroll container**, also in
  `useScrollViewport`'s `syncScrollState()` — the browser's native scroll-anchoring nudges
  `scrollTop` to compensate whenever content mutates above the fold, which is constantly true of a
  virtualized list; setting this on whatever `findScrollableAncestor` actually resolves to (rather
  than on some unrelated inner div) fixes it for every virtualized list using this composable.

**Everything else that grew out of this investigation was Screenshots-thumbnail-generation work —
new Rust-side thumbnail decode/resize/encode/cache, a screenshot-specific fetch concurrency limiter,
CSS blur-exclusion for the loading skeleton, and several rounds of chasing a scroll-position bug
through `cardHeight` measurement drift and animated group-repositioning — all of which has since been
fully reverted.** See "Shared Minecraft folders — removed" above for the general reasoning; the
short version is the same here: this fork no longer touches the Screenshots tab at all, so
`packages/app-lib/src/api/instance/screenshots/operations.rs`,
`apps/app-frontend/src/components/ui/screenshots-page/{card,group,index}.vue`, and every other
Screenshots-thumbnail-specific file were restored to their exact pre-Studio (stock upstream) state.
Nothing about the Screenshots tab in this fork is Studio code anymore.

## Replays tab: metadata cache, sort/group/bulk-delete, and why there's no "launch into a replay"

The virtualization/concurrency fixes above stopped the Replays tab from freezing, but didn't stop
it from being slow *every single time* it was opened — `list_replays` still opened and parsed every
replay's zip metadata on every visit, with no memory of having done that same work last time. Fine
for a handful of replays, still a multi-second wait for someone with ~900 of them, every time they
click the tab.

Fixed the same way the Screenshots tab already solves this for itself (`reconcile_source_screenshots`
in `packages/app-lib/src/api/instance/screenshots/reconciliation.rs`): a persistent, on-disk index
instead of a full re-scan every time. `studio_replay_metadata_cache` (new table, own Studio migration
— see `studio-migrations/20260909120000_studio-replay-metadata-cache.sql`) stores each replay's parsed
metadata keyed by `(instance_id, kind, file_name)`, with `file_size`/`modified_at` as a cheap staleness
check. `list_replays` (`packages/app-lib/src/api/replays.rs`) now only opens a replay's zip when it's
new or its size/mtime changed since it was last cached; everything else comes back from one indexed
`SELECT`. Unlike Screenshots' own index, this doesn't do SHA1-based rename detection — a renamed
replay is simply treated as new and its zip re-read once (the old row for the vanished name gets
pruned the same pass), which is an acceptable trade-off for something that isn't renamed nearly as
often as it's just *opened*. First load after upgrading (or after adding a big batch of replays at
once) still pays the full parse cost, bounded by the existing `REPLAY_METADATA_READ_CONCURRENCY = 16`;
every visit after that is one fast query.

Also added: **sort** (newest/oldest/name/duration/file size) and **group by** (server/world, or date)
controls, plus **multi-select with bulk delete** — `replays/index.vue`, `ReplayItem.vue` (now takes
`selected`/`selection-active` props and a circular checkbox mirroring the Library instance cards' own
selection toggle), and a new `ReplayGroupHeader.vue`. Deliberately mirrors the Screenshots tab's own
toolbar (`Combobox` sort/group dropdowns, `FloatingActionBar` for the bulk-action bar) rather than
inventing a different pattern. Group headers ride the *same* virtualized list as ordinary rows (an
item's `type` field picks which component renders) rather than a second, variable-height virtualizer
— `useVirtualScroll` assumes a single fixed item height for everything in its array, so headers are
just given the same row height as a replay card; see the comment on `ReplayGroupHeader.vue` for why
that trade-off was made instead of writing a new virtualizer.

**Why there's still no "launch straight into a replay" button.** Investigated this properly before
deciding against it. Neither ReplayMod nor Flashback expose any command-line flag, config option, or
file association for opening a specific replay directly — both only support picking one by hand from
their own in-game screen after Minecraft has already started, confirmed against ReplayMod's own docs,
forums, and source, and Flashback's repo. Vanilla Minecraft's own `--quickPlaySingleplayer` (what the
*Worlds* tab's launch button actually uses — see `QuickPlayType::Singleplayer` in
`packages/app-lib/src/launcher/args.rs`) doesn't transfer either: it loads an existing folder from
`saves/`, and a replay isn't backed by one — both mods read straight from the `.mcpr`/replay zip
through their own custom screen, never registering it as a real world. That leaves two real ways to
fake it from outside the mod: simulate the in-game clicks needed to open the replay viewer and pick
the file (works without touching the mods, but is coordinate/resolution/mod-version-fragile and
untestable from here), or ship a companion mod that hooks ReplayMod's/Flashback's *internal*
(non-public) classes to open a replay on startup (more reliable once working, but ReplayMod alone
ships a separate build per Minecraft version with no shared API — "compatible with every version"
would mean an unbounded number of builds chasing every mod update forever, not a one-time cost).
Decided neither was worth shipping; sort/group/bulk-delete above is the answer to "make the tab more
useful" instead.

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
   and publishes a GitHub release — to the separate **public releases repo**
   (`speedzing/modrinth-studios-releases`), not this private source repo — with the files the
   in-app updater expects.
4. Everyone running the app gets the update prompt automatically (Modrinth's built-in updater,
   pointed at `speedzing/modrinth-studios-releases` releases instead of Modrinth's own servers).

### Windows update install mode

`apps/app/tauri-release.conf.json`'s `plugins.updater.windows.installMode` controls how the NSIS
installer runs when "Reload to update" installs the downloaded update on exit
(`apps/app/src/main.rs`'s `RunEvent::Exit` handler calls `update.install(data)`, then `app.restart()`
if the person asked to reload). This was `"quiet"` (fully invisible, no window at all) through
0.1.4, and at least one person's "Reload to update" just closed the app with no relaunch and no
error dialog — reinstalling the `.exe` manually fixed it. Nothing in the app's own logs pointed at
why (see below), and a fully invisible installer with no UI at all is a known rough edge for NSIS
quiet-mode installs on Windows (easy for it to fail silently, or for antivirus/SmartScreen to
quietly interfere with a background process that never shows a window) — so this was changed to
`"passive"`: still no clicking required, but it shows a small progress window while it runs, which
both sidesteps that class of silent failure and makes a real failure visible instead of just
vanishing. This only affects updates built *after* the change ships — it can't retroactively fix
anyone already stuck on a version where "Reload to update" is dead; they still need to grab the
latest installer manually and run it once.

If it still fails, the launcher's own logs are the next thing to check —
`%APPDATA%\ModrinthApp\launcher_logs\session_<timestamp>.log` (most recent one from before the
failed update), search for `Pending update install`. `Ok` vs the exact error on `Err` narrows it
down a lot faster than guessing.

### Why releases live in a separate public repo

The source stays in this private repo, but release *artifacts* (the installer, `latest.json`,
signatures) are published to a second, public, code-free repo. Two reasons this is necessary,
not just a preference:

- **Friends without GitHub access to this repo need a plain download link.** A public repo's
  release page and `releases/download/...` URLs work for anyone, logged in or not.
- **The in-app updater can't authenticate against a private repo anyway.** Tauri's updater sends
  a plain unauthenticated request to `releases/latest/download/latest.json`. GitHub does not
  support token auth on that URL shape for private repos at all (only its separate
  asset-by-ID REST API supports tokens, which the built-in updater doesn't use) — so a private
  releases repo would break auto-updates for everyone, including you.

Making *this* repo public instead was the simpler alternative (Modrinth App's own license is
GPL-3.0, so there's no real confidentiality being protected by staying private), but the
two-repo split was chosen instead so the source stays private.

### One-time repo setup for releases

**1. Create the public releases repo.** On GitHub, create a new repository named
`modrinth-studios-releases` under the `speedzing` account, set to **Public**, initialized with a
README (so it has a `main` branch — the release workflow needs one to exist). Nothing else needs
to go in it; it only ever holds releases created by the workflow below.

**2. Create a token that can publish releases there.** The workflow's default `GITHUB_TOKEN` is
auto-scoped to the repo the workflow runs in (this private repo) and can't touch a different
repo, so a personal token is needed instead:

- GitHub → Settings (your account, not the repo) → Developer settings → Fine-grained tokens →
  Generate new token.
- Resource owner: `speedzing`. Repository access: "Only select repositories" →
  `modrinth-studios-releases`.
- Permissions: **Contents: Read and write** (that's the only one release creation needs).
- Set an expiration you're comfortable renewing later, generate it, and copy the token.

**3. Add it as a secret on the *private* (source) repo** — Settings → Secrets and variables →
Actions → New repository secret:

- Name: `RELEASE_REPO_TOKEN`
- Value: the token from step 2

Plus the two secrets that were already required:

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

**Giving friends the download link**: once a release has published, send them
`https://github.com/speedzing/modrinth-studios-releases/releases/latest` — no GitHub account
needed to download from a public repo's release page.

## Licensing note

Modrinth App is source-available but not permissively licensed for redistribution (see
`COPYING.md`/`LICENSE` in this repo). Since this fork stays private and is only ever shared
directly with people you know, that's a non-issue in practice — just don't make the repo public
or distribute builds beyond your friend group.
