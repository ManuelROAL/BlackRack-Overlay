# Shared overlay behavior

This document contains rules shared by every overlay. Overlay-specific behavior,
telemetry semantics and file ownership live under `docs/overlays/`; use
`docs/overlays/README.md` to select the relevant document.

## Visual language

- Use the current Standings and Relative treatment as the visual reference:
  compact dark surfaces, thin neutral outlines, restrained lime accents,
  separated data surfaces, condensed typography and selective emphasis of the
  primary value. Adapt the language to the overlay rather than cloning a table.
- Use the bundled Roboto Condensed font throughout overlays and the control panel.
- Optimize for legibility while driving. Keep spacing compact and give important
  values a deliberate hierarchy.
- Keep overlay descriptions and configuration-option labels in the control panel
  readable at a glance; secondary text must retain clear contrast against cards.
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

- Tauri hosts all panels in one transparent WebView on the selected monitor and
  moves that host when the selection changes. Monitors without the host
  contribute no composed surface. Both the native host and WebView use explicit
  transparent RGBA backgrounds.
- In game mode the host covers only the bounding box of the visible panels plus a
  24 px margin, not the whole display: the transparent surface the compositor
  puts over the game costs the game on every present. Edit mode restores the full
  monitor because panels are dragged anywhere on it.
- The layout is measured against the monitor, never against the host's own
  viewport, which stops describing the surface as soon as the host shrinks. The
  backend returns the monitor in CSS pixels with every bounds update and the
  composite offsets its stage by the same rounded origin the host was moved to,
  so panel coordinates stay monitor coordinates.
- Explicit deactivation removes the panel document. LMU-driven automatic
  visibility hides or shows the host without rebuilding active panels.
- Every run starts in click-through game mode. Edit mode is entered explicitly
  from the control panel or global shortcut.
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
- Automatically hide overlays when LMU is not foreground, the player is inactive,
  the game is not realtime, the player is in the garage or the session has ended.
  Spectator and team modes permit non-realtime viewing. Spectator mode treats the
  currently watched entry as the reference vehicle; team mode keeps the player's
  registered team car as the reference regardless of the active camera. They are
  mutually exclusive; garage and session-end hiding still apply.
  Keep overlays visible while the control panel has focus so configuration changes
  can be previewed; the remaining automatic visibility conditions still apply. Do
  not capture Escape for visibility.

## Control panel and persistence

- Keep top-level separation between overlays, general settings and integrations.
  General is the leftmost tab and the default view when the control panel opens.
  Visibility stays immediately accessible. Per-overlay monitor, transparency,
  reset and content options live in that overlay's single settings disclosure.
- Present general settings as a single compact, full-width group with consistently
  aligned controls. Preserve readable secondary copy and stack fields on narrow
  windows instead of leaving partial rows or unused columns.
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
- Keep the overlay catalog scannable in two columns at the normal control-panel
  width: descriptions may use two lines, configuration must read as an action,
  and enabled state should remain clear without turning every card into a bright
  outline. Collapse the catalog to one column when the window is narrow.
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
- A common support card remains below the visible content in Overlays, General
  and Integrations, immediately above the shortcut footer. It opens the fixed
  project Ko-fi URL in the system browser and never embeds remote content.

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
