# Cross-cutting product and engineering decisions

This file records decisions shared by the product. Overlay-specific decisions live
in `docs/overlays/` and should not be duplicated here.

## Product

- Monitor scope is persisted separately from layout. Global mode projects every
  overlay onto `globalMonitor`; individual mode restores stored assignments.

- The product name is **BlackRack Overlay**. Its technical bundle identifier is
  `com.blackrack.overlay`, while user-owned backend data uses the readable
  `%APPDATA%\BlackRack Overlay` directory on Windows. The WebView2 user data
  profile (`EBWebView`) is set explicitly on every webview window to
  `%LOCALAPPDATA%\BlackRackOverlay`, independent of the bundle identifier.
- Windows is the primary platform. Linux UI support remains desired, while live
  telemetry under Proton still needs validation.
- Information density and legibility while driving take priority over decoration.
- Overlays remain independently selectable/configurable. Overlays assigned to the
  same monitor share one transparent host WebView, so moving panels between
  monitors does not create one renderer per overlay.
- Game mode is click-through and every run starts there. Edit mode is explicit.
  Entering edit mode may focus the control panel, and returning to game mode on
  Windows restores the window that was active before editing. Escape is not
  intercepted because it conflicts with LMU controls.
- Positions, proportional sizes, each overlay's monitor assignment, the general
  monitor fallback and user preferences persist. A panel may be deliberately
  cropped at the monitor edge while retaining a recoverable strip.
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
- Auto is the default, but the control panel lets a user pin one simulator.
  The pin is frontend state resent to the backend on load, the same pattern the
  performance profile already uses, rather than a file the backend persists
  itself. A pin overrides selection on the one telemetry cycle it changes —
  built from that candidate's own source regardless of whether it currently
  looks available, so a pinned-but-closed simulator reports its own "waiting"
  state instead of silently reading a different one or showing fabricated mock
  data. `try_new` therefore must succeed independently of the `available` probe
  used to rank candidates in Auto mode; only the OS/build gate can fail it.
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
  Game mode subdivides by session kind — practice with warmup, qualifying and
  race — because only the driver's own weekend changes shape between free
  running, a timed lap and the race; spectating and team duty follow one car with
  one configuration. A session kind is bound only when the user asks for it and
  otherwise follows game mode, so the per-mode behavior is unchanged by default
  and the panel never has to guess before telemetry arrives.
  A profile owns only overlay-facing state: visibility, layout (including
  per-overlay monitor assignments), monitor scope, transparency, text size and
  per-overlay settings. The monitor fallback inside that scope, performance
  profile, locale, shortcuts and browser source stay global, so changing mode
  does not alter cadence.
- The active profile is the live configuration rather than a copy: the existing
  `localStorage` keys stay authoritative and the profile store is refreshed from
  them. Overlays and OBS routes therefore need no knowledge of profiles, and
  edits cannot be lost to a forgotten save step.
- General/per-overlay transparency preserves its independent saved values while a
  common value is active.
- General/per-overlay text size preserves its independent saved values while a
  common value is active. Increasing text may expand the reported design surface
  so compact panels do not clip while preserving the user's visual scale.
- Display units are global settings: temperatures default to Celsius and speeds
  to km/h, and the same choices apply to native overlays and OBS. Convert only
  when presenting values; telemetry fields, heatmap thresholds and domain
  calculations retain their canonical Celsius, km/h and SI values. Configuration
  exports include both choices, and older imports keep the current local choices.
- Hosts are grouped by effective monitor: every monitor with at least one visible
  overlay gets one transparent composite host, and monitors without visible
  overlays get none. General mode projects every overlay onto one monitor, while
  per-overlay mode uses each saved assignment. This keeps the renderer count
  proportional to active monitors rather than overlay count and avoids the
  empty-host composition cost that originally motivated removing per-overlay
  assignment.
- Per-overlay configuration and position resets are scoped and never affect
  visibility or another overlay.
- Import/export is a single validated versioned document covering UI preferences
  and geometry, not learned telemetry, diagnostics, session data or credentials.
- Disabled panels remove their documents. Automatic LMU visibility hides/shows
  hosts without rebuilding active panels; host windows are created or closed only
  when the set of active monitor assignments changes.
- The global hide/show shortcut is a separate runtime state from profile
  visibility. Effective host visibility requires at least one desired overlay
  and neither global shortcut hiding nor automatic LMU hiding to be active.

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

- Users install the application normally. Telemetry uses LMU's native `LMU_Data`
  shared memory; no external shared-memory DLL or additional LMU option is required.
- Distribution is installer-only. The control panel may open the fixed project
  Ko-fi and PayPal pages in the system browser, but it does not embed remote
  donation content, accept arbitrary URLs or make either service part of
  telemetry and startup behavior.
- The NSIS finish page retains Tauri's desktop-shortcut and run-app choices and
  adds a Ko-fi option that starts checked but can be cleared. It opens the same
  fixed URL only when still selected on Finish; silent installs never open it.
  The app does not install or require a shared-memory DLL; it reports live
  telemetry status from LMU's native shared memory.
- Tauri embeds `frontendDist`; the installer does not deploy a duplicate `web/`
  directory.
- Automatic update checks use one fixed HTTPS manifest and run from the control
  panel, never from overlay or browser-source windows. The manifest is treated as
  untrusted input: its version, HTTPS URLs, installer name and SHA-256 are
  validated; the package is hashed while downloading and again in the detached
  helper before NSIS is launched. The helper accepts only an installer in a
  dedicated operation directory under the application's update cache, serializes
  downloads and installations with a named process mutex, waits for the parent
  process to exit, and starts the existing current-user installer with `/UPDATE /P /R`.
  A copied `coordinator.exe` takes over before NSIS starts, keeps the active
  installation marker until the installer exits and records a non-zero installer
  result for the next application start.
  Each download uses its own operation directory under the update cache so
  separate app instances cannot overwrite one another's package.
  The localized release history can request an explicitly confirmed rollback by
  version. The backend constructs the versioned GitHub manifest URL itself,
  requires the manifest to match and be older than the installed version, and
  sends the installer through the same download, SHA-256 and helper checks.
  Update failures are non-fatal and do not affect telemetry.
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
  explicit end marker, and known credential fields are redacted. Window
  lifecycle milestones and telemetry shutdown are recorded; the next launch
  reports when the previous session ended without a definitive normal or panic
  marker, which helps identify native crashes without running crash-path code.
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
