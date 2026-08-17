# Cross-cutting product and engineering decisions

This file records decisions shared by the product. Overlay-specific decisions live
in `docs/overlays/` and should not be duplicated here.

## Product

- Windows is the primary platform. Linux UI support remains desired, while live
  telemetry under Proton still needs validation.
- Information density and legibility while driving take priority over decoration.
- Overlays remain independently selectable/configurable but share one transparent
  host WebView per monitor to reduce renderer cost.
- Game mode is click-through and every run starts there. Edit mode is explicit.
  Escape is not intercepted because it conflicts with LMU controls.
- Positions, proportional sizes, monitor assignments and user preferences persist.
  Panels may cross monitor edges deliberately while retaining a recoverable strip.
- Roboto Condensed and all required flags, logos and badges remain bundled.

## Host and configuration

- Each host and its WebView use explicit transparent RGBA backgrounds; the
  transparent flag alone is insufficient in release WebView2 builds.
- General/per-overlay transparency and monitor modes preserve their independent
  saved values while a common value is active.
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
  The app expects LMU's own shared-memory plugin and reports whether it is found.
- Tauri embeds `frontendDist`; the installer does not deploy a duplicate `web/`
  directory.
- OBS browser source is localhost-only, optional and off by default.
- Shortcut conflicts are non-fatal and visible/configurable.
- `startup.log` is replaced each run and is the first diagnostic for startup
  failures on another computer.
- Release checksums accompany artifacts but are not required to run the installer.

## Documentation ownership

- `docs/ARCHITECTURE.md`: runtime and build structure.
- `docs/TELEMETRY.md`: shared sources, identity and integrations.
- `docs/OVERLAYS.md`: shared visual, host and control-panel contracts.
- `docs/overlays/*.md`: authoritative behavior for one overlay.
- `docs/PERFORMANCE.md`: active measurement method and next validation.
- `docs/PERFORMANCE_HISTORY.md`: completed performance investigations.
