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

- Tauri groups panels into one transparent full-monitor host WebView per detected
  monitor. Both the native host and WebView use explicit transparent RGBA
  backgrounds.
- Explicit deactivation removes the panel document. LMU-driven automatic
  visibility hides or shows the host without rebuilding active panels.
- Every run starts in click-through game mode. Edit mode is entered explicitly
  from the control panel or global shortcut.
- Embedded panel content is inert: no clicks, focus, selection, native dragging or
  context menu. Edit-mode pointer input moves panels or operates resize handles.
- A panel may cross monitor edges for deliberate cropping, but movement retains a
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
- Monitor selectors show the Windows display number from `\\.\DISPLAYn`; the
  positional host index remains internal so existing assignments stay valid.
- Configuration reset affects only the selected panel's defaults. Position reset
  affects only its geometry on its assigned monitor. Neither reset changes
  visibility or another overlay; confirmation uses the styled in-panel dialog.
- Import/export uses one versioned JSON document with visibility, transparency,
  monitor selection, geometry and content preferences. Use native dialogs,
  validate before applying, and map unavailable monitors to the primary display.
  Exclude learned telemetry, diagnostic logs, session state and credentials.

## Composite delivery and OBS

- Composite hosts consume grouped `telemetry://batch` events, forward only named
  locally mounted targets, and project each frame onto the reused per-overlay
  allowlist in `src/composite.ts` before same-origin `postMessage`.
- Update that allowlist whenever a renderer consumes a serialized field. Do not
  restore per-overlay native listeners or direct cross-realm object events.
- The optional browser server listens only on `http://127.0.0.1:47636`, uses a
  single SSE endpoint, and remains completely inactive when disabled.
- Its route catalog must match `docs/overlays/README.md`. Mirror every configurable
  preference that an OBS page needs because browser pages do not share the Tauri
  WebView's `localStorage`.
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
