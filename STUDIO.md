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
