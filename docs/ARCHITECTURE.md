# Architecture

## Runtime flow

```text
LMU shared memory                LMU local REST          RaceControl/RaceOS
       |                              |                        |
       v                              v                        v
sim/lmu/bridge.cpp -> LmuSnapshot -> LmuTelemetrySource <- async/cached enrichments
                                      |
                                      v
                                TelemetryFrame
                                      |
                  +-------------------+-------------------+
                  |                   |                   |
Tauri events    analysis JSONL/strategy CSV    browser-source SSE
                   |                                       |
        one composite host per active monitor           OBS browser
                   |
        delta/standings/fuel/flags/rejoin panels
```

## Backend ownership

- `src-tauri/src/lib.rs`
  - Creates one borderless transparent overlay host per monitor that has visible
    overlays. Hosts are created and closed on demand; monitors without a host
    contribute no composed surface.
  - Provides the initial composite layout seed and single-panel reset geometry,
    fitted to the logical size of the assigned monitor so authored defaults never
    seed off-screen on a smaller or scaled display; the frontend then persists
    panel geometry and monitor assignment in `localStorage`.
  - Forces both the native window and WebView backgrounds to transparent RGBA;
    this is explicit because release WebView2 builds must not fall back to an
    opaque black surface.
  - Tracks each overlay's desired visibility separately from temporary global
    shortcut hiding and automatic LMU hiding. Hosts show only when at least one
    overlay is desired and neither temporary hide state is active. Losing LMU
    focus to the control panel keeps the host visible so configuration can be previewed.
  - Applies click-through interaction mode.
  - Registers configurable global shortcuts without crashing if a binding is
    already occupied.
  - Exposes Tauri commands for overlays, logging, shortcuts, dependencies and the
    browser source.
  - Sends telemetry to the monitor hosts over their IPC channels as raw bytes:
    one batch per cycle, naming the overlays due and carrying only their fields
    from `src/overlay-telemetry-fields.json`.
- `src-tauri/src/telemetry/mod.rs`
  - Defines `TelemetryFrame`, `StandingEntry` and the warnings, and stays
    simulator agnostic: the source contract lives in `telemetry/sim`.
  - Asks `sim::detect` for the source at startup. What it gets back follows
    whichever simulator is running, so the active one can change while the app
    is up; the control panel refreshes its answer when the frame reports a
    different source.
  - Owns the 50 Hz scheduler, per-overlay emission rates and payload grouping.
    Standings/Relative share one batch when due, Track Map uses its stripped batch,
    and the remaining active overlays share the base-frame batch.
  - Owns JSONL analysis logging and top-level performance samples.
  - Emits bounded chat snapshots on `chat://update` only when the REST worker
    has a new snapshot; chat text stays out of telemetry frames and analysis logs.
- `src-tauri/src/telemetry/strategy_log.rs`
  - Aggregates the existing player frame in memory and writes one user-facing CSV
    row per complete lap for external stint and strategy analysis.
- `src-tauri/src/telemetry/delta_records.rs`
  - Reconstructs distance-sampled laps and selects best/optimal references for
    overall, session, stint and last-lap comparisons.
  - Keeps the 50 Hz calculation in memory and sends boundary records to a
    dedicated SQLite worker.
- `src-tauri/src/telemetry/sim/lmu/bridge.cpp`
  - Opens the official `LMU_Data` mapping read-only.
  - Copies the shared-memory object while holding the SDK lock.
  - Normalizes the required subset into fixed C-compatible structs.
- `src-tauri/src/telemetry/sim/lmu/source.rs`
  - Converts the snapshot to stable application semantics.
  - Maintains session, vehicle identity, lap, pit, standings and warning state.
  - Calculates resource usage, total-lap estimates and DR gain estimates.
  - Owns the shared-memory layout, the per-car trackers and the source struct;
    the models are split into `lmu/` child modules that read those private
    types directly, under `source/`: `frame.rs` assembles a frame and the rest hold one family
  each (`standings.rs`, `warnings.rs`, `session.rs`, `fuel.rs`, `driver_rank.rs`,
  `vehicle.rs`, `weather.rs`, `tests.rs`).
- `sim/lmu/rest.rs`
  - Polls local REST on background threads and exposes only fresh cached values.
  - Polls `/rest/chat/` on a separate, demand-gated 0.5 Hz worker while the native
    Chat panel or its OBS route is active. Snapshots are capped at eight messages.
- `sim/lmu/driver_ranks.rs`, `sim/lmu/event_split.rs`, `sim/lmu/racecontrol.rs`
  - Authenticate and enrich online sessions without blocking the hot loop.
- `sim/lmu/trackmap.rs`, `sim/lmu/install.rs`
  - Read the official track outline and locate LMU's local installation data for
    event enrichment. Both are reached through LMU-specific modules, never from
    the agnostic modules.
- `src-tauri/src/telemetry/sim/mod.rs`
  - Holds `TelemetrySource`, `SourceDescriptor`, `SourceCapabilities`, the
    `CANDIDATES` table and the source that keeps following the running
    simulator. See `docs/SIMULATORS.md`.
- `browser_source.rs`
  - Optional localhost-only HTTP/SSE server at `127.0.0.1:47636`.
  - Serves OBS pages through Tauri's embedded `frontendDist` asset resolver; it
    does not read an installed `web/` directory.
  - Injects the selected locale and browser-only preferences before overlay
    modules run. `?lang=es|en` overrides only that page, while the localized route
    index keeps the resolved override in its links.
  - Returns stable browser-server error kinds to the control panel and retains
    raw operating-system details only for diagnostics.
  - Starts only when enabled and serializes frames only with connected clients.
  - The server thread waits on the frame channel instead of polling on a fixed
    interval, and keeps a tighter accept poll only while it is serving requests.
  - Frames published while that thread is behind are dropped on purpose; the
    count is reported in the browser source status and in the startup log.
  - Chat uses a separate named SSE event on the same demand-filtered `/api/events`
    connection and is serialized only for a browser client requesting `/chat`.

## Frontend ownership

- `src/main.ts`: control panel and persisted settings.
  The control panel also bundles the release manifests into a localized changelog
  view, while the update card uses the same release-note selection logic.
  The support actions ask the Rust backend to open the fixed project Ko-fi or
  PayPal URL in the system browser; no remote page is loaded inside the
  application WebView.
  Known browser-server and shortcut failures are localized from stable backend
  kinds/codes; raw system details are logged rather than rendered as UI copy.
- `src/lap-records.ts`: the control panel's LAP TIMES view, which lists and
  deletes the learned references stored per track and car.
- `browser.html` and `src/browser-index.ts`: catalog-driven OBS route index. The
  backend serves this built entry and injects the same locale metadata used by
  overlay pages; it does not maintain a second translated route catalog.
- `src/composite.ts`: per-monitor host, iframe lifecycle, drag/resize chrome and
  routing between Tauri events and embedded overlay documents. Each host listens
  to the single native telemetry batch and forwards frames only to locally mounted
  overlay documents named in that batch. Before the same-origin `postMessage`, it
  projects the shared native frame onto a reused overlay-specific object containing
  only the fields consumed by that renderer, reducing structured-clone work.
- `src/composite-layout.ts`: persisted position, size and monitor assignment.
- `src/telemetry-types.ts`: TypeScript mirror of serialized Rust types.
- `src/runtime-events.ts`: Tauri, composite-frame messaging or browser-source SSE
  abstraction. The Chat route uses a separate named SSE event and never opens a
  telemetry frame subscription.
- `src/frontend-diagnostics.ts`: persists uncaught errors, unhandled promise
  rejections and explicit `console.error` calls in the per-session diagnostic log,
  while retaining the original browser-console output and bounding duplicate
  reports. Rust panics add their cause, location and backtrace to that same log;
  the backend retains the latest 20 session files.
- `src/release-notes.ts`: imports the checked-in `release/*/manifest.json` files,
  orders them by version and selects localized notes for the control panel.
  The changelog offers rollback actions only for releases older than the
  installed version; the frontend sends only the selected version to the Rust
  updater, which fetches and verifies that release's manifest and installer.
- `src/backend-errors.ts`: translates the stable snake_case codes commands reject
  with. Backend `Result::Err` payloads returned to a webview are codes, never
  prose; `startup_log::command_error` records the English detail in the session
  diagnostic log and hands the code back, so user-facing wording lives only in
  the i18n catalogs. Keep this table in step with the `Err` payloads in
  `src-tauri/src`.
- `src/overlay-fit.ts`: scales the complete design when a window is resized.
- `src/overlay-interaction.ts`: drag/click-through behavior.
- `src/overlay-appearance.ts`: transparency and text-size persistence and application.
- `src/overlay-profiles.ts`: profile/binding storage model and validation. The
  control panel owns capture and application; the composite host and overlays
  never read profiles, only the live keys a profile writes.
- General settings expose a configurable global shortcut (default `Ctrl+Shift+H`)
  that temporarily hides or shows all active overlays without changing profile
  visibility. It composes with automatic LMU hiding.
- The control panel exposes one `hide_<overlay-id>` shortcut per overlay card,
  using the shared `get_shortcut_settings`/`set_shortcut` contract and reflecting
  external `overlay://visibility` events. These bindings are empty and inactive
  by default; Delete or Backspace clears an assigned overlay binding.
- General settings also expose an optional `cycle_delta_mode` global shortcut,
  empty by default and cleared the same way, that advances the Delta reference
  mode. The application does not read game controllers itself.
- `src/overlay-performance.ts`: optional five-second frontend render metrics.

Every overlay has a separate CSS file. `src/styles.css` contains only genuinely
shared overlay primitives; the control panel uses `src/control-panel.css`.
Overlay-specific frontend and backend ownership is indexed in
`docs/overlays/README.md` and documented in each overlay file.

## Release compilation

The NSIS installer reads `src-tauri/terms-of-use.txt` through `bundle.licenseFile`.
This UTF-8 BOM document contains the English and Spanish terms in one scrollable
page after Welcome. `MUI_LICENSEPAGE_CHECKBOX` requires explicit acceptance before
continuing in interactive installation; NSIS localizes the surrounding controls
to the selected installer language. Existing passive/silent installation behavior
is unchanged and does not record interactive acceptance. Keep the terms aligned
with the public resource description. Verify packaging with `npm.cmd run tauri build`;
native UI verification must check the unchecked/checked Next button and accents.

The Cargo release profile enables fat link-time optimization with one codegen
unit and strips symbols from release binaries. This applies to production/Tauri
release builds only; development builds keep their normal fast incremental
profile and diagnostics.

## Scheduling and freshness

- The base source cycle, lap reconstruction and consumption learning remain at
  20 ms (50 Hz). Overlay-only view models, strategy scenarios and warning scans
  run only with native or route-specific OBS demand. Performance profiles change
  delivery/rendering cadence: Smooth uses 20 ms for fast overlays, Balanced 40 ms
  and Efficiency 60 ms.
- Fuel follows the selected fast-overlay cadence. Active flags and rejoin warnings
  retain their safety-oriented cadence independently of the profile.
- Full standings: Smooth 100 ms, Balanced 160 ms, Efficiency 240 ms; it is only
  requested by an active Standings/Relative panel, their matching OBS route or
  the explicit DR-estimate logger.
- Relative: Smooth 50 ms, Balanced 80 ms, Efficiency 120 ms while active. Cycles coinciding
  with Standings reuse the same constructed roster.
- Track Map: Smooth 33 ms, Balanced 60 ms and Efficiency 120 ms. Every profile
  retains the complete lightweight coordinate roster.
- Detailed damage and pit-stop estimate: Smooth 50 ms, Balanced 80 ms and
  Efficiency 120 ms.
- Stint history: 250 ms; its model is prepared only with native or OBS demand.
- Active Rejoin warning: 50 ms; inactive flag/rejoin warning: 250 ms.
- Automatic visibility: 250 ms.
- Control-panel status: 500 ms.
- Local REST standings: 200 ms, maximum accepted age 1 second.
  Standings, supplement and weather workers sleep independently when none of their
  native or route-specific OBS consumers is active.
- Local REST session/usage/pit estimate: 1 second, maximum age 3 seconds.

Network and local REST work runs outside the telemetry thread. Do not introduce
blocking HTTP calls into `next_frame()`.

## Persistence

- Shared WebView2 user data (the `EBWebView` profile): `%LOCALAPPDATA%\BlackRackOverlay`,
  set explicitly on every webview window so it does not follow the technical
  bundle identifier. All Tauri WebViews share this one user data folder.
- Control-panel position: `control-window.json` under
  `%APPDATA%\BlackRack Overlay`. Windows' synthetic minimized position is never
  persisted or restored, so closing the application while minimized cannot make
  the panel unreachable on the next launch.
- Overlay position, size and per-overlay monitor assignment: composite-layout
  `localStorage`. The general monitor selection persists as
  `overlay-monitor.json` under the application config directory and acts as the
  target for the “move all overlays” control.
- Browser source and shortcuts: JSON under the application config directory.
- Overlay choices, columns, transparency and text size: WebView `localStorage`.
  Transparency and text size store individual values separately from their
  general/individual scopes.
- The selected UI locale: WebView `localStorage` under
  `blackrack-overlay.locale.v1`; configuration schema 8 also exports it as
  `ui.locale`.
- The selected performance profile: WebView `localStorage`; configuration schema
  9 exports it with overlay settings and reapplies it to the Rust scheduler.
- Overlay profiles and their game/spectator/team bindings: WebView `localStorage` under
  `blackrack-overlay.profiles.v1` and `blackrack-overlay.profile-bindings.v1`.
  A profile stores overlay visibility, layout (including each overlay's monitor),
  transparency, text size and the per-overlay settings; the general monitor
  fallback, performance profile, locale, shortcuts and the browser source stay
  global. The live keys above remain authoritative for the
  active profile, which is refreshed from them on a short debounce and flushed
  before switching mode, session, profile or exporting. Configuration schema 17
  exports both, and a document below that version becomes one profile bound to
  every mode.
- Per-session bindings: WebView `localStorage` under
  `blackrack-overlay.session-bindings.v1`, one optional profile id for practice
  (warmup included), qualifying and race. They apply in game mode only; `null`
  keeps a session kind on the game binding, which is also what an unknown
  session resolves to. The panel classifies the session from the telemetry
  frame it already receives and keeps the live kind in `sessionStorage`, so the
  reload that applies a profile does not resolve back and switch again.
  Configuration schema 19 exports them; a document below that version imports
  with no session bound.
- Spectator mode: WebView `localStorage`; configuration schema 10 exports it and
  reapplies it to the telemetry source when the control panel starts.
- Overlay visual scale: composite-layout `localStorage`; configuration schema 11
  exports it so startup preserves the chosen size while iframe design dimensions
  settle or change with configurable content.
- Configuration import accepts every positive schema version from both the former
  `lmu-overlay-configuration` and current `blackrack-overlay-configuration`
  formats. The frontend migrates known fields to schema 10, fills additive overlay
  and setting fields from current defaults, and ignores unknown future fields.
- Learned consumption profiles: application data `consumption-profiles/`.
- Current and previous startup/frontend failure logs, plus optional JSONL analysis
  logs: application data directory.
- Learned delta references and lap/stint history: application data
  `lap-records.sqlite3`, using bundled SQLite and asynchronous boundary writes.

When adding a setting needed by OBS, mirror it through
`set_browser_source_preferences`; browser WebViews do not share the Tauri
WebView's `localStorage` automatically.

## Build behavior

`src-tauri/build.rs` searches for `SharedMemoryInterface.hpp`, first through
`LMU_SHARED_MEMORY_SDK` and then common Steam libraries. On Windows it defines
`cfg(lmu_sdk)` and compiles the C++ bridge when found. Otherwise Rust selects the
mock source. Do not assume that a successful build necessarily includes live LMU
telemetry; inspect the Cargo warning.

Tauri embeds `../dist` once through `build.frontendDist`. Do not duplicate that
directory in `bundle.resources`; both the app protocol and the optional OBS
server resolve the same executable-embedded assets.

Vite production builds use Terser with two compression passes and top-level name
mangling, but do not mangle object properties because telemetry and persisted
settings depend on stable field names. Source maps and emitted comments remain
disabled. Tauri DevTools are explicitly disabled for configured and dynamic
WebViews, and the capability denies the internal DevTools toggle command.

On MSVC release builds, `build.rs` wraps the Tauri 2.6.x static-VCRuntime COFF
placeholder in a valid `.lib` archive. This preserves static CRT linking with
MSVC 14.44, whose linker rejects the upstream object when it only has a `.lib`
extension.
