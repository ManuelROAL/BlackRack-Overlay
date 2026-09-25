# Shared overlay behavior

This document contains rules shared by every overlay. Overlay-specific behavior,
telemetry semantics and file ownership live under `docs/overlays/`; use
`docs/overlays/README.md` to select the relevant document.

## Visual language

Monitor selection has General and Per overlay scopes. General mode projects all
visible overlays onto one effective monitor without overwriting stored individual
assignments; per-overlay mode restores those assignments.

- Use the current Standings and Relative treatment as the visual reference:
  compact dark surfaces, thin neutral outlines, restrained lime accents,
  separated data surfaces, condensed typography and selective emphasis of the
  primary value. Adapt the language to the overlay rather than cloning a table.
- Use the bundled Roboto Condensed font throughout overlays and the control panel.
- Optimize for legibility while driving. Keep spacing compact and give important
  values a deliberate hierarchy.
- Keep overlay descriptions and configuration-option labels in the control panel
  readable at a glance; secondary text must retain clear contrast against cards.
- Overlay-specific configuration panels render text in sentence case at one
  12 px size. Their selects share the same dark surface, neutral border, and lime
  focus treatment while retaining widths suited to each panel layout.
- Keep each overlay's CSS in its own file. Shared primitives belong in
  `src/styles.css` only when they are genuinely shared.
- Do not set `color-scheme: dark` on an embedded document root; WebView2 can paint
  the unused iframe canvas as an opaque rectangle during resize.
- Embedded native documents suppress CSS animations, transitions and backdrop
  filters so WebView2 presents only for bounded telemetry or UI changes instead
  of continuously at the monitor refresh rate. Express native warning motion as
  cadence-driven state changes when it is essential; standalone and OBS pages may
  retain decorative motion. New overlays must preserve this shared safeguard.
- Embedded native documents also replace wide blurred panel shadows with a short
  one and declare `contain: layout paint style` on the document body, which the
  scaled, clipped overlay already satisfies. Both bound the rectangle WebView2
  rasterises for a panel repaint. Standalone and OBS pages keep the full shadow.
- Panel scale is quantised to 1/64 so a resize reuses one raster scale instead of
  producing a new one for every pointer movement.

## Host and interaction

- Tauri groups panels by their effective monitor. Each monitor with at least one
  visible panel gets one transparent WebView host, while monitors without visible
  panels have no host and contribute no composed surface. General mode projects
  every panel onto one monitor; per-overlay mode changes one panel's assignment.
  Both the native host and WebView use explicit transparent RGBA backgrounds.
- In game mode the host covers only the bounding box of the visible panels plus a
  24 px margin, not the whole display: the transparent surface the compositor
  puts over the game costs the game on every present. Edit mode restores the full
  monitor because panels are dragged anywhere on it.
- The layout is measured against the monitor, never against the host's own
  viewport, which stops describing the surface as soon as the host shrinks. The
  backend returns the monitor in CSS pixels with every bounds update and the
  composite offsets its stage by the same rounded origin the host was moved to,
  so panel coordinates stay monitor coordinates.
- Explicit deactivation removes the panel document. When the last visible panel
  leaves a monitor, its host is closed; when the first panel arrives, the host is
  created on demand. LMU-driven automatic visibility hides or shows existing
  hosts without rebuilding active panels.
- Every run starts in click-through game mode. Edit mode is entered explicitly
  from the control panel or global shortcut. Entering edit mode may focus the
  control panel; when the global shortcut locks the overlays again on Windows,
  the window that was active before editing regains focus so simulator controls
  such as Escape continue to reach the game.
- Every overlay card has a configurable hotkey to show or hide that overlay.
  General settings also provide a configurable global shortcut (default
  `Ctrl+Shift+H`) that temporarily hides or shows every active overlay. This
  runtime hide state leaves profile visibility unchanged and composes with
  automatic LMU hiding: hosts show only when neither hide state is active and
  at least one overlay is desired.
  The shared capture flow persists each `hide_<overlay-id>` binding and mirrors
  external `overlay://visibility` events in the card and active profile. The
  default is empty (inactive); Delete or Backspace removes an assigned binding.
- On Windows, edit-mode hit testing follows the global cursor independently on
  every monitor. A host accepts input only while the cursor is over one of its
  visible panel rectangles; transparent gaps pass input to the application
  underneath, including applications owned by another process. Native window
  style updates must run without holding the shared hit-test state lock because
  Windows may synchronously wait for the host UI thread while applying them. The
  hit test resolves the regions in place and copies only one decision per window
  into a reused buffer, because it repeats every few milliseconds while editing.
- Panel hit-test regions are reported by the composite host in physical pixels
  using the WebView's own `devicePixelRatio`, so they match the rendered surface
  on any display scale factor; the native side only adds the window origin and
  clamps to the monitor.
- Embedded panel content is inert: no clicks, focus, selection, native dragging or
  context menu. Edit-mode pointer input moves panels or operates resize handles.
- A panel may be deliberately cropped at the monitor edge, but movement retains a
  32 px visible strip so it remains recoverable.
- Resize scales the complete design proportionally. When configurable content
  changes the reported design size, preserve the user's explicitly persisted
  visual scale while updating host geometry. Do not infer that scale again from
  transient iframe dimensions during startup.
- The minimum panel size (120 px wide, 90 px for damage, 32 px tall) is enforced
  while the user resizes, against the design size shown at that moment. Restoring
  a persisted scale only clamps it to the monitor, never to that floor: an
  overlay whose design shrinks with its content —the empty standings reports its
  72 px minimum— would otherwise have the stored scale raised and saved
  again on every launch, and would reappear oversized once the roster arrived.
- Automatically hide overlays when LMU is not foreground, the player is inactive,
  the game is not realtime or the player is in the garage. Session phases alone
  —including the pre-session server wait, the session timer and a
  checkered/finished phase—never hide overlays.
  When LMU explicitly reports `SME_END_SESSION`, invalidate its retained scoring
  and telemetry snapshot so previous-session values clear as the game returns to
  the menu. Do not use phase 0 as a substitute; the pre-session server wait also
  uses that phase.
  Spectator and team modes permit non-realtime viewing. Spectator mode treats the
  currently watched entry as the reference vehicle; team mode keeps the player's
  registered team car as the reference regardless of the active camera. They are
  mutually exclusive; garage hiding still applies.
  Garage hiding follows that single reference vehicle, not the local player's
  garage state when spectating a different car.
  In spectator mode, the control panel disables Lift & Coast, Stint History and
  Fuel/Energy and unmounts their native panels. Their saved profile visibility is
  preserved, including when modes share a profile; game and team modes restore
  their normal visibility. Show/hide-all skips these restricted panels.
  Keep overlays visible while the control panel has focus so configuration changes
  can be previewed; the remaining automatic visibility conditions still apply. Do
  not capture Escape for visibility.

## Control panel and persistence

- General opens with a collapsible getting-started guide (language/monitor,
  overlay selection, edit/game mode and automatic visibility). Acknowledging it persists
  locally under `blackrack-overlay.getting-started.v1`; the summary always lets
  users reopen it, and its guide action opens the existing overlay help dialog.
- Configuration profiles are user-created; the panel has no recommended presets.
  Previously saved profiles remain available as ordinary editable profiles.
- Integrations provides a copyable support summary with an explicit field allowlist:
  app version, simulator selection/connection/dependency availability, locale,
  performance/follow mode, enabled overlay IDs, control-screen size/scale and selected
  overlay monitor index. Unknown values remain null. It never reads logs or includes
  personal paths, profile names, telemetry identities or credentials. The summary
  remains visible for manual copying when clipboard access fails.
- These onboarding/support controls live in `index.html`, `src/main.ts`,
  `src/control-panel.css` and the shared ES/EN catalogs. Verify with `npm.cmd run build`
  plus first-open/acknowledgement and clipboard success/fallback checks in the
  control panel.
  Verified for this addition: production frontend build, ES/EN browser rendering,
  acknowledgement surviving reload, reopening the overlay guide and clipboard
  success. Clipboard-denied behavior remains pending native verification.

- Keep top-level separation between overlays, general settings and integrations.
  General is the leftmost tab and the default view when the control panel opens.
  Visibility stays immediately accessible. Per-overlay monitor, transparency,
  reset and content options live in that overlay's single settings disclosure.
- Present general settings as labelled full-width blocks that each open with the
  shared section heading: Display (the monitor, transparency and text size with
  their general-value sliders, plus the global temperature and speed units), Mode (the
  follow modes with the configuration profiles, their per-mode bindings and the
  per-session bindings that only game mode shows),
  Performance, Application (interface language and the global shortcuts) and
  Backup. Keep the controls inside a block consistently aligned, preserve
  readable secondary copy and stack fields on narrow windows instead of leaving
  partial rows or unused columns. A block heading already names its content, so
  do not repeat it on the row below.
- General settings expose Smooth, Balanced and Efficiency performance profiles.
  Profiles change delivery/render cadence while the source,
  delta engine and strategy calculations remain sampled at 50 Hz. Smooth preserves
  full-cadence behavior; Balanced is recommended; Efficiency also removes the
  Track Map pulse. Every profile retains the complete Track Map roster.
  Every cadence is a whole number of 20 ms source cycles and an exact multiple of
  the profile's fast cadence, so a cycle that repaints a slow overlay repaints the
  fast ones too. Smooth delivers at 20/40/40/40/100 ms, Balanced at
  40/80/80/80/160 ms and Efficiency at 60/120/120/120/240 ms
  (fast / relative / Track Map / secondary / standings).
- Keep the control panel legible at arm's length from a driving position: no
  user-facing copy below 11 px, secondary text at a contrast that stays readable
  over the dark surface, and selection controls large enough to hit without
  aiming. Selected performance profiles, follow modes and filter chips share one
  loud lime treatment so the current choice is never inferred from a faint
  border.
- Keep the overlay catalog scannable in two columns at the normal control-panel
  width: the card title shares its row with the visibility switch, the
  description spans the full card below them with up to three lines, and the
  configuration disclosure reads as a full-width action button with the
  contextual help action beside it. Collapse the catalog to one column when the
  window is narrow.
- Keep an overlay's visibility readable without inspecting its switch: the
  catalog header reports how many overlays of the total are open, and the filter
  row exposes an Active chip that narrows the catalog to the open overlays and
  refreshes whenever a card's visibility changes.
- The overlay catalog provides a global guide action and one contextual help
  action per card. Both open the same bundled, accessible dialog; contextual
  help selects that overlay directly. `src/overlay-guide.ts` owns the complete
  ordered overlay-to-copy mapping, while localized user-facing explanations live
  in `src/i18n/catalogs.ts`. Keep this guide complete when adding an overlay and
  keep technical implementation detail in `docs/overlays/` rather than exposing
  it verbatim to users.
- Use the same hierarchy and help-text treatment in every overlay configuration
  disclosure. Expanded controls and reorder grids must reflow without horizontal
  overflow at the narrow control-panel breakpoint.
- Present integrations as clearly introduced local tools, with matching section
  headers using the same lime accent, explicit status badges and a visible
  disclosure affordance. Start all integration disclosures collapsed whenever
  the control panel opens.
  On short integration views, keep the support card and shortcut footer anchored
  at the bottom of the control panel instead of leaving unused space below them.
- General and per-overlay transparency and text-size modes remain independent.
  General mode must preserve saved individual values. Text size ranges from 75%
  to 200%; increases may expand the design surface without scaling non-text
  elements, while composite layout preserves the user's visual scale. Expand
  column spacing and row height independently: columns retain a minimum margin,
  while vertical growth remains limited to the space required by larger lines.
  Each overlay declares only the horizontal and vertical text space it needs;
  non-tabular overlays must retain their compact base surface.
- Monitor selectors use the Windows-reported friendly model name and resolution;
  they do not expose an app ordinal as though it were the unrelated number shown
  by the Windows Settings Identify action. Fall back to the native display name
  when Windows does not provide a friendly name.
- Configuration reset affects only the selected panel's defaults. Position reset
  affects only its geometry on its assigned monitor. Both actions sit under an
  explicit localized reset label, neither changes visibility or another overlay,
  confirmation uses the styled in-panel dialog, and neither action changes the
  selected top-level control-panel view.
- A bulk reset card closes the Overlays view with one configuration and one
  position action that apply the same per-overlay defaults to every overlay at
  once. It reuses the single-overlay reset paths, keeps the same in-panel
  confirmation dialog with its own localized message, reports progress in a
  polite live region, and changes neither visibility, monitor selection,
  language, general transparency/text scopes nor the selected control-panel
  view. The configuration action reloads the panel once after every overlay has
  been restored; the position action restores geometry sequentially without a
  reload.
- Import/export uses one versioned JSON document with visibility, transparency,
  monitor selection, geometry, performance profile and content preferences. Use native dialogs,
  validate before applying, and map unavailable monitors to the primary display.
  Exclude learned telemetry, diagnostic logs, session state and credentials.
  Imports retain known values from older product/schema versions, fill settings
  introduced later with current defaults and tolerate unknown additive fields.
- The **Novedades** view lists localized release notes and offers installation
  for published versions older than the installed version. Confirm the selected
  version before starting the verified download and in-place installer flow.
- A common support card remains below the visible content in Overlays, General
  and Integrations, immediately above the shortcut footer. Its Ko-fi and PayPal
  buttons open their fixed project URLs in the system browser and never embed
  remote content.

## Composite delivery and OBS

- Native overlay documents use the bundled locale selected by the control panel.
  Translate static HTML, dynamic status, tooltips and accessibility text through
  `src/i18n/`; keep telemetry values and stable IDs semantic. A locale change
  reloads the composite and mounted documents instead of adding work to hot
  telemetry render paths.
- Composite hosts consume grouped `telemetry://batch` events, forward only named
  locally mounted targets, and project each frame onto the reused per-overlay
  allowlist in `src/composite.ts` before same-origin `postMessage`.
- Native visibility and the exact connected OBS route jointly define backend
  demand. Disabled overlays do not build their presentation models; an unrelated
  OBS page must not activate another overlay's calculations.
- Update that allowlist whenever a renderer consumes a serialized field. Do not
  restore per-overlay native listeners or direct cross-realm object events.
- The optional browser server listens only on `http://127.0.0.1:47636`, uses a
  single SSE endpoint, and remains completely inactive when disabled.
- OBS routes receive the application locale before their modules render. A valid
  `?lang=es` or `?lang=en` query overrides the locale for that page without
  changing the saved browser-source preference or another scene.
- Its route catalog must match `docs/overlays/README.md`. Mirror every configurable
  preference that an OBS page needs, including effective text size, because browser pages do not share the Tauri
  WebView's `localStorage`.
- The `/` route serves the catalog-driven `browser.html` entry. Keep its links in
  sync with this route catalog; do not duplicate translated labels in Rust.
- Serve browser assets through Tauri's embedded `frontendDist` resolver; do not
  install a separate `web/` resource directory.

## Adding or changing an overlay

1. Read this file and the overlay's document under `docs/overlays/`.
2. For a new overlay, create `docs/overlays/<overlay-id>.md` in the same change
   and register it in `docs/overlays/README.md` with its entry HTML, OBS route and
   normal cadence. At minimum, document scope/files, sources and cadence,
   behavior/invariants, and verification focus.
3. Treat that file as the overlay's living source of truth. Record there every
   rule or decision that arises in its dedicated task/chat; keep central documents
   limited to genuinely shared contracts.
4. Keep its entry HTML, renderer, stylesheet, composite registration, settings,
   serialized fields and OBS route synchronized as applicable.
5. Update the overlay document when behavior, sources, cadence or architecture
   changes. Do not duplicate those details in `AGENTS.md`.
6. Run the verification listed in `AGENTS.md` and any overlay-specific checks.
7. Verify the native composite has no continuous CSS animation, transition or
   backdrop filter capable of raising presentation cadence above its telemetry
   cadence; use a comparable PresentMon capture for performance-sensitive motion.
