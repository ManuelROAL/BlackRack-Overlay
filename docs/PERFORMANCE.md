# Performance context

## Goal and comparison method

The relevant comparison target is TinyPedal with equivalent widgets enabled.
Measure under the same LMU replay/race segment and comparable visual content.
The collection procedure is documented in `tools/performance/README.md`.

Primary external metrics:

- CPU average and P95.
- Private memory average and maximum.
- GPU average and P95.
- Include LMUOverlay's WebView2 child processes.

Do not enable detailed telemetry logging for the primary external comparison;
file writes add overhead. Run additional isolated passes for LMUOverlay and
TinyPedal where possible.

## Internal instrumentation

When analysis logging is enabled, the JSONL contains five-second performance
events from Rust and each active Tauri overlay.

Top-level `performance_sample` includes cycle frequency, source/logging/visibility/
emission/work timings, overruns and per-overlay emission counts.
`standings_source` separates:

- `due_cycles`
- `requested_cycles`
- `skipped_cycles`
- `average_with_standings_us`
- `average_without_standings_us`

`source_stage_performance` further separates snapshot, REST reception, session
processing, standings-state update, full standings build, flags/rejoin and frame/
strategy work. Frontend samples include average/max render duration and item count.

## Optimizations already implemented

- Vite assets are never inlined, avoiding a huge standings JavaScript bundle.
- Static vehicle identity is cached and refreshed on driver swap/session reset.
- Standings row and header DOM nodes are cached and synchronized in place.
- Text and CSS state are only changed when values differ.
- Standings and browser-source tables are emitted at 10 Hz instead of the 50 Hz
  source rate; Relative is emitted at 20 Hz so row changes follow telemetry.
- The roster is constructed only when one of those consumers is due. When
  Relative is visible this can be 20 Hz; coincident 10 Hz consumers reuse it.
- Standings history state is updated independently at 10 Hz.
- Dashboard, driving, tyres, fuel and active flags use raw snapshots at 50 Hz
  without forcing a full table. Rejoin remains at 20 Hz while active.
- DR estimate iteration avoids allocating a temporary opponent vector per driver.
- Browser source stays stopped when disabled and uses bounded/non-blocking frame
  delivery when enabled.
- Local REST and RaceControl calls run on background threads with cached results.
- Overlay panels share one transparent host WebView per monitor. Disabled panels
  remove their iframe/document, while temporary LMU-driven hiding preserves the
  host and active documents. Renderer count therefore scales with monitor count
  instead of overlay count.
- Track Map receives a lightweight coordinate roster at 20 Hz and smooths SVG
  groups with 110 ms compositor transform transitions, enough to bridge an
  isolated delayed update. It has no continuous JavaScript animation loop and no
  per-marker filter effects; only the single player halo retains a lightweight
  opacity animation.
- Its official main-track and pitlane geometry is fetched and parsed once per
  circuit, cached in the backend, and transferred outside telemetry frames. The
  static payload is never polled or repeated at 20 Hz; OBS obtains the same cached
  payload through its local `/api/trackmap` route.

## Latest baseline interpretation

The previous comparison showed a meaningful frontend standings render reduction,
but backend `average_source_us` had regressed from roughly 395 microseconds to
roughly 1532 microseconds. Investigation found that the complete standings table
was still being built every source cycle even though it was emitted more slowly.

That issue has now been changed. A new comparable capture is required before
claiming a measured improvement. The next analysis should check:

1. Source loop remains near 50 Hz with zero/near-zero overruns.
2. With Standings visible, `requested_cycles` is about one fifth of all cycles;
   with Relative visible it is about two fifths.
3. Hidden standings with no OBS client has zero requested cycles.
4. `average_without_standings_us` is close to the pre-regression source cost.
5. `average_with_standings_us` is higher but amortized over 10 Hz normally, or
   runs at 20 Hz while Relative is visible for maximum row responsiveness.
6. UI behavior, flag responsiveness and pit/lap history remain correct.
7. WebView2 renderer count remains near one per monitor plus the control panel,
   and full-monitor transparency does not introduce a GPU regression.

## Optimization principles

- Measure before and after with the same workload.
- Optimize update frequency according to human-visible need; do not reduce safety
  warning cadence merely to improve a benchmark.
- Cache static strings/assets and derive slowly changing values outside 50 Hz.
- Avoid repeated JSON serialization when no browser client exists.
- Prefer bounded channels and stale-data fallbacks over blocking the source loop.
- A successful build is not performance evidence.
