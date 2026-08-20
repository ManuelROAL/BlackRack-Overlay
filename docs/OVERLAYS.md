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
- Keep each overlay's CSS in its own file. Shared primitives belong in
  `src/styles.css` only when they are genuinely shared.
- Do not set `color-scheme: dark` on an embedded document root; WebView2 can paint
  the unused iframe canvas as an opaque rectangle during resize.

## Host and interaction

- Tauri hosts all panels in one transparent full-monitor WebView on the selected
  monitor and moves that host when the selection changes. Monitors without the
  host contribute no composed surface. Both the native host and WebView use
  explicit transparent RGBA backgrounds.
- Explicit deactivation removes the panel document. LMU-driven automatic
  visibility hides or shows the host without rebuilding active panels.
- Every run starts in click-through game mode. Edit mode is entered explicitly
  from the control panel or global shortcut.
- On Windows, edit-mode hit testing follows the global cursor independently on
  every monitor. A host accepts input only while the cursor is over one of its
  visible panel rectangles; transparent gaps pass input to the application
  underneath, including applications owned by another process. Native window
  style updates must run without holding the shared hit-test state lock because
  Windows may synchronously wait for the host UI thread while applying them.
- Embedded panel content is inert: no clicks, focus, selection, native dragging or
  context menu. Edit-mode pointer input moves panels or operates resize handles.
- A panel may be deliberately cropped at the monitor edge, but movement retains a
  32 px visible strip so it remains recoverable.
- Resize scales the complete design proportionally. When configurable content
  changes the reported design size, preserve the user's visual scale while
  updating host geometry.
- Automatically hide overlays when LMU is not foreground, the player is inactive,
  the game is not realtime, the player is in the garage or the session has ended.
  Do not capture Escape for visibility.

## Control panel and persistence

- Keep top-level separation between overlays, general settings and integrations.
  Visibility stays immediately accessible. Per-overlay monitor, transparency,
  reset and content options live in that overlay's single settings disclosure.
- General and per-overlay modes for monitor assignment and transparency remain
  independent. General mode must preserve saved individual values.
- Monitor selectors use the Windows-reported friendly model name and resolution;
  they do not expose an app ordinal as though it were the unrelated number shown
  by the Windows Settings Identify action. Fall back to the native display name
  when Windows does not provide a friendly name.
- Configuration reset affects only the selected panel's defaults. Position reset
  affects only its geometry on its assigned monitor. Neither reset changes
  visibility or another overlay; confirmation uses the styled in-panel dialog.
- Import/export uses one versioned JSON document with visibility, transparency,
  monitor selection, geometry and content preferences. Use native dialogs,
  validate before applying, and map unavailable monitors to the primary display.
  Exclude learned telemetry, diagnostic logs, session state and credentials.
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
- Update that allowlist whenever a renderer consumes a serialized field. Do not
  restore per-overlay native listeners or direct cross-realm object events.
- The optional browser server listens only on `http://127.0.0.1:47636`, uses a
  single SSE endpoint, and remains completely inactive when disabled.
- OBS routes receive the application locale before their modules render. A valid
  `?lang=es` or `?lang=en` query overrides the locale for that page without
  changing the saved browser-source preference or another scene.
- Its route catalog must match `docs/overlays/README.md`. Mirror every configurable
  preference that an OBS page needs because browser pages do not share the Tauri
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
