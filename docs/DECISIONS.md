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
  Flags and manufacturer logos are pre-rasterised to small PNGs by
  `tools/rasterize-icons.mjs` and committed, because the overlays draw them into
  boxes of a few pixels and WebView2 would otherwise parse a whole vector
  document to fill one. The SVG sources stay in the repository as the masters
  and as the fallback for an icon that has not been rasterised yet.

## Simulators

- The telemetry source is chosen at runtime from a registry in
  `telemetry/sim`, not by `#[cfg]` at the point of use. A simulator's SDK may
  still be a compile-time gate inside its own module, but nothing outside
  `sim/<id>/` names a simulator.
- Selection is continuous, not a startup decision. The app is normally launched
  before the game, and closing one simulator to open another must not need a
  restart, so a disconnected source re-probes the candidates every two seconds
  and adopts the first one that reports itself available. A connected source is
  never displaced.
- A source declares what it can *ever* report as capabilities on its
  descriptor, and the frame carries them. A source still being built out turns
  a capability on only once it fills the frame fields behind it, so a partial
  simulator advertises less rather than more. That is a different question from
  the per-session availability the frame already expressed through
  `virtual_energy_active`, `rest_weather_available` and the `-1` sentinels;
  neither replaces the other. The control panel retires an overlay a
  simulator can never feed instead of offering one that stays empty.
- Work the app needs off the telemetry thread — the official track outline
  and the dependency probe — is reached through function pointers on the
  descriptor rather than through `&mut self` on the source.
- Visible copy names the active simulator through a parameter. Overlay window
  titles carry no simulator prefix at all, because the app is meant to serve
  more than one.
- Settings, profiles and the exported configuration are still global. With a
  second simulator in the tree the migration is now owed; it is tracked with the
  other open items in `docs/SIMULATORS.md`.

## Host and configuration

- Each host and its WebView use explicit transparent RGBA backgrounds; the
  transparent flag alone is insufficient in release WebView2 builds.
- Every webview passes the same Chromium arguments. WebView2 keeps one browser
  process per user data directory, so the environment created first decides them
  for the whole application. They restore wry's own defaults, which the option
  replaces and drop browser subsystems the overlay never uses. They bound
  neither the renderer count nor the V8 heap. Sharing one renderer would put the
  control panel on the overlay host's main thread, and a 192 MB heap cap held
  memory near 605 MB instead of 1.2 GB but produced 42-48% CPU collection bursts
  every ~80 s that the uncapped build never showed. Native window occlusion stays
  enabled so the control panel stops rendering behind the game.
- The overlay host is bounded to its visible panels in game mode and restored to
  the full monitor for editing. Panel geometry is expressed in monitor
  coordinates in both states.
- Overlay configuration is grouped into named profiles, and game, spectator and
  team mode each bind to one of them. Selecting a mode applies the bound profile.
  A profile owns only overlay-facing state: visibility, layout, transparency,
  text size and per-overlay settings. Monitor, performance profile, locale,
  shortcuts and the browser source stay global, so changing mode never moves the
  host or alters cadence.
- The active profile is the live configuration rather than a copy: the existing
  `localStorage` keys stay authoritative and the profile store is refreshed from
  them. Overlays and OBS routes therefore need no knowledge of profiles, and
  edits cannot be lost to a forgotten save step.
- General/per-overlay transparency preserves its independent saved values while a
  common value is active.
- General/per-overlay text size preserves its independent saved values while a
  common value is active. Increasing text may expand the reported design surface
  so compact panels do not clip while preserving the user's visual scale.
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
- Overlay cadences are counted in source cycles and every period is an exact
  multiple of the profile's fast period, so updates land on shared cycles. Free
  timers drifted into neighbouring cycles and made the transparent host present
  close to the monitor refresh rate even when each overlay changed far less often.
  An active flag or rejoin warning follows the fast cadence so alignment never
  delays a safety warning.
- User-facing strategy capture is a separate opt-in per-lap CSV. It keeps invalid,
  non-green and pit laps marked, skips partial laps and never exposes internal
  performance diagnostics as strategy data.
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
- Each run creates a unique log under `%APPDATA%\BlackRack Overlay\diagnostics`;
  the latest 20 sessions are retained without one launch overwriting another.
  Uncaught errors, unhandled promise rejections and explicit frontend
  `console.error` calls are persisted there with duplicate/rate limiting. Rust
  panics record their cause, source location and backtrace, normal exits carry an
  explicit end marker, and known credential fields are redacted.
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
