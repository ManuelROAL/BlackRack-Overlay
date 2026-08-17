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
             Tauri events       analysis JSONL      browser-source SSE
                  |                                       |
       one composite host per monitor                   OBS browser
                  |
       dashboard/standings/fuel/flags/rejoin panels
```

## Backend ownership

- `src-tauri/src/lib.rs`
  - Creates one borderless transparent overlay host for each detected monitor.
  - Migrates former window-state geometry into the composite layout seed.
  - Forces both the native window and WebView backgrounds to transparent RGBA;
    this is explicit because release WebView2 builds must not fall back to an
    opaque black surface.
  - Tracks desired visibility separately from automatic hiding.
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
  - Starts only when enabled and serializes frames only with connected clients.

## Frontend ownership

- `src/main.ts`: control panel and persisted settings.
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
- `src/overlay-fit.ts`: scales the complete design when a window is resized.
- `src/overlay-interaction.ts`: drag/click-through behavior.
- `src/overlay-appearance.ts`: transparency persistence and application.
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

- Base source cycle, Dashboard, Trailing + Pedal and tyres: 20 ms (50 Hz).
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

- Control-panel geometry: `tauri-plugin-window-state`.
- Overlay position, size and monitor assignment: composite-layout `localStorage`.
  A separate monitor-selection record stores general/individual mode and the
  individual assignments retained while general mode is active.
- Browser source and shortcuts: JSON under the application config directory.
- Overlay choices, columns and transparency: WebView `localStorage`. Transparency
  stores individual values separately from its general/individual scope.
- Learned consumption profiles: application data `consumption-profiles/`.
- Startup log and optional JSONL analysis logs: application data directory.

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
