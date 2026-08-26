# Architecture

## Runtime flow

```text
LMU shared memory                LMU local REST          RaceControl/RaceOS
       |                              |                        |
       v                              v                        v
lmu_bridge.cpp -> LmuSnapshot -> LmuTelemetrySource <- async/cached enrichments
                                      |
                                      v
                                TelemetryFrame
                                      |
                  +-------------------+-------------------+
                  |                   |                   |
Tauri events    analysis JSONL/strategy CSV    browser-source SSE
                   |                                       |
        one composite host on the selected monitor      OBS browser
                   |
        delta/standings/fuel/flags/rejoin panels
```

## Backend ownership

- `src-tauri/src/lib.rs`
  - Creates one borderless transparent overlay host on the selected monitor and
    moves/resizes it when the selection changes; monitors without the host
    contribute no composed surface.
  - Provides the initial composite layout seed; the frontend then persists panel
    geometry in `localStorage`.
  - Forces both the native window and WebView backgrounds to transparent RGBA;
    this is explicit because release WebView2 builds must not fall back to an
    opaque black surface.
  - Tracks desired visibility separately from automatic hiding. Losing LMU focus
    to the control panel keeps the host visible so configuration can be previewed.
  - Applies click-through interaction mode.
  - Registers configurable global shortcuts without crashing if a binding is
    already occupied.
  - Exposes Tauri commands for overlays, logging, shortcuts, dependencies and the
    browser source.
  - Emits telemetry to the monitor hosts as filtered `telemetry://batch` events.
    Each batch serializes one frame once and carries the overlay IDs that consume
    that payload variant.
- `src-tauri/src/telemetry/mod.rs`
  - Defines `TelemetryFrame`, `StandingEntry`, warnings and `TelemetrySource`.
  - Owns the 50 Hz scheduler, per-overlay emission rates and payload grouping.
    Standings/Relative share one batch when due, Track Map uses its stripped batch,
    and the remaining active overlays share the base-frame batch.
  - Owns JSONL analysis logging and top-level performance samples.
- `src-tauri/src/telemetry/strategy_log.rs`
  - Aggregates the existing player frame in memory and writes one user-facing CSV
    row per complete lap for external stint and strategy analysis.
- `src-tauri/src/telemetry/delta_records.rs`
  - Reconstructs distance-sampled laps and selects best/optimal references for
    overall, session, stint and last-lap comparisons.
  - Keeps the 50 Hz calculation in memory and sends boundary records to a
    dedicated SQLite worker.
- `src-tauri/src/telemetry/lmu_bridge.cpp`
  - Opens the official `LMU_Data` mapping read-only.
  - Copies the shared-memory object while holding the SDK lock.
  - Normalizes the required subset into fixed C-compatible structs.
- `src-tauri/src/telemetry/lmu.rs`
  - Converts the snapshot to stable application semantics.
  - Maintains session, vehicle identity, lap, pit, standings and warning state.
  - Calculates resource usage, total-lap estimates and DR gain estimates.
- `lmu_rest.rs`
  - Polls local REST on background threads and exposes only fresh cached values.
- `driver_ranks.rs`, `event_split.rs`, `racecontrol.rs`
  - Authenticate and enrich online sessions without blocking the hot loop.
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

## Frontend ownership

- `src/main.ts`: control panel and persisted settings.
  The support action asks the Rust backend to open the fixed project Ko-fi URL
  in the system browser; no remote page is loaded inside the application WebView.
  Known browser-server and shortcut failures are localized from stable backend
  kinds/codes; raw system details are logged rather than rendered as UI copy.
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
  abstraction.
- `src/frontend-diagnostics.ts`: persists uncaught errors, unhandled promise
  rejections and explicit `console.error` calls through the startup log, while
  retaining the original browser-console output and bounding duplicate reports.
- `src/overlay-fit.ts`: scales the complete design when a window is resized.
- `src/overlay-interaction.ts`: drag/click-through behavior.
- `src/overlay-appearance.ts`: transparency and text-size persistence and application.
- `src/overlay-performance.ts`: optional five-second frontend render metrics.

Every overlay has a separate CSS file. `src/styles.css` contains only genuinely
shared overlay primitives; the control panel uses `src/control-panel.css`.
Overlay-specific frontend and backend ownership is indexed in
`docs/overlays/README.md` and documented in each overlay file.

## Release compilation

The Cargo release profile enables fat link-time optimization with one codegen
unit and strips symbols from release binaries. This applies to production/Tauri
release builds only; development builds keep their normal fast incremental
profile and diagnostics.

## Scheduling and freshness

- Base source cycle, Delta, Timing compacto, Trailing + Pedal and tyres:
  20 ms (50 Hz).
- Fuel overlay and active flags: 20 ms (50 Hz).
- Full standings: 100 ms (10 Hz), and only when requested by an active Standings
  panel or a connected browser-source client.
- Relative: 50 ms (20 Hz) while its panel is active. Cycles coinciding
  with Standings reuse the same constructed roster.
- Track Map: 33 ms (approximately 30 Hz) with a lightweight coordinate-only roster; it does
  not request the enriched Standings construction.
- Detailed damage and pit-stop estimate: 50 ms (20 Hz).
- Active Rejoin warning: 50 ms; inactive flag/rejoin warning: 250 ms.
- Automatic visibility: 250 ms.
- Control-panel status: 500 ms.
- Local REST standings: 200 ms, maximum accepted age 1 second.
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
- Overlay position and size: composite-layout `localStorage`. The selected
  overlay monitor index persists as `overlay-monitor.json` under the application
  config directory.
- Browser source and shortcuts: JSON under the application config directory.
- Overlay choices, columns, transparency and text size: WebView `localStorage`.
  Transparency and text size store individual values separately from their
  general/individual scopes.
- The selected UI locale: WebView `localStorage` under
  `blackrack-overlay.locale.v1`; configuration schema 8 also exports it as
  `ui.locale`.
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
