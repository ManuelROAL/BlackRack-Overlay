# Cross-cutting product and engineering decisions

This file records decisions shared by the product. Overlay-specific decisions live
in `docs/overlays/` and should not be duplicated here.

## Product

- The product name is **BlackRack Overlay**. Its technical bundle identifier is
  `com.blackrack.overlay`, while user-owned backend data uses the readable
  `%APPDATA%\BlackRack Overlay` directory on Windows. The WebView2 user data
  profile (`EBWebView`) is set explicitly on every webview window to
  `%LOCALAPPDATA%\BlackRackOverlay`, independent of the bundle identifier.
- Windows is the primary platform. Linux UI support remains desired, while live
  telemetry under Proton still needs validation.
- Information density and legibility while driving take priority over decoration.
- Overlays remain independently selectable/configurable but share one transparent
  host WebView on the selected monitor to reduce renderer cost.
- Game mode is click-through and every run starts there. Edit mode is explicit.
  Escape is not intercepted because it conflicts with LMU controls.
- Positions, proportional sizes, the selected monitor and user preferences
  persist. A panel may be deliberately cropped at the monitor edge while
  retaining a recoverable strip.
- Roboto Condensed and all required flags, logos and badges remain bundled.

## Host and configuration

- Each host and its WebView use explicit transparent RGBA backgrounds; the
  transparent flag alone is insufficient in release WebView2 builds.
- General/per-overlay transparency preserves its independent saved values while a
  common value is active.
- All overlays share one host on a single selected monitor. Per-overlay monitor
  assignment was removed because an empty transparent host on a secondary monitor
  forced DWM alpha-composition at that monitor's refresh rate and degraded game
  FPS with mixed-refresh displays.
- Per-overlay configuration and position resets are scoped and never affect
  visibility or another overlay.
- Import/export is a single validated versioned document covering UI preferences
  and geometry, not learned telemetry, diagnostics, session data or credentials.
- Disabled panels remove their documents. Automatic LMU visibility hides/shows
  hosts without rebuilding active panels.

## Data and performance ownership

- Shared memory is authoritative for live telemetry; local REST and RaceOS are
  optional cached enrichments that must degrade gracefully.
- Rust owns telemetry semantics, persistent learning, strategy math and roster
  selection. Frontend renderers own presentation and browser-only state.
- Lap references and lap/stint history use bundled SQLite in the application data
  directory. The hot telemetry path remains memory-only and persistence happens
  asynchronously at semantic boundaries.
- Telemetry cadence follows information needs rather than host grouping. Heavy
  work does not enter the 50 Hz loop without comparable measurement.
- Composite hosts retain grouped native batches and same-origin `postMessage`
  field projection; measured direct cross-realm object events were slower.
- TinyPedal is a behavioral/performance reference only and GPL source is not
  copied. Dox and Go Fast are design/behavior references only.

## External services and security

- RaceOS endpoints are observed client behavior and may change.
- Authentication uses LMU's local short-lived ticket. No long-lived or private
  server key is embedded.
- Tickets, tokens and full RaceOS responses are never logged or persisted.
- Event split resolution uses authenticated overview first, direct my-split
  second and LMU local storage last, as detailed in `docs/TELEMETRY.md`.

## Distribution and diagnostics

- Users install the application normally and do not copy a project DLL into LMU.
- Distribution is installer-only. The control panel may link to the fixed project
  Ko-fi page in the system browser, but it does not embed remote donation content,
  accept arbitrary URLs or make Ko-fi part of telemetry and startup behavior.
- The NSIS finish page retains Tauri's desktop-shortcut and run-app choices and
  adds a Ko-fi option that starts checked but can be cleared. It opens the same
  fixed URL only when still selected on Finish; silent installs never open it.
  The app expects LMU's own shared-memory plugin and reports whether it is found.
- Tauri embeds `frontendDist`; the installer does not deploy a duplicate `web/`
  directory.
- OBS browser source is localhost-only, optional and off by default.
- OBS pages normally mirror the application locale. A supported `?lang=` query is
  deliberately page-local so scenes in different languages can coexist without
  mutating the shared browser-source preference.
- Shortcut conflicts are non-fatal and visible/configurable.
- `startup.log` is replaced each run and is the first diagnostic for startup or
  frontend failures on another computer. Uncaught errors, unhandled promise
  rejections and explicit frontend `console.error` calls are persisted there
  with duplicate/rate limiting. The replaced run remains available as
  `startup.previous.log`, and known credential fields are redacted.
- Release checksums accompany artifacts but are not required to run the installer.

## Documentation ownership

- `docs/ARCHITECTURE.md`: runtime and build structure.
- `docs/TELEMETRY.md`: shared sources, identity and integrations.
- `docs/OVERLAYS.md`: shared visual, host and control-panel contracts.
- `docs/LOCALIZATION.md`: cross-cutting language scope, propagation and rollout.
- `docs/LOCALIZATION_CONTRIBUTING.md`: locale terminology, catalog workflow,
  automated checks and the visual verification matrix.
- `docs/overlays/*.md`: authoritative behavior for one overlay.
- `docs/PERFORMANCE.md`: active measurement method and next validation.
- `docs/PERFORMANCE_HISTORY.md`: completed performance investigations.
