# Performance workflow

## Goal and comparison method

Compare BlackRack Overlay with TinyPedal under the same moving LMU replay/race segment
and comparable visible content. Follow `tools/performance/README.md`.
Record the selected performance profile and use the same one in every BlackRack
comparison; Smooth is the full-cadence baseline for captures predating profiles.

Primary external metrics include BlackRack Overlay's WebView2 child processes:

- CPU average and P95
- private memory average and maximum
- GPU average and P95

Do not enable detailed telemetry logging during the primary external comparison;
file writes add overhead. Use separate internal-diagnostic passes.

## Internal instrumentation

Analysis JSONL records five-second Rust/frontend samples:

Its control is collapsed under advanced diagnostics in the normal interface. It
remains accessible for requested support and development captures, separate from
the visible per-lap strategy CSV.

- `performance_sample`: cycle frequency, source/logging/visibility/emission/work
  timing, overruns and per-overlay emission counts.
- `standings_source`: due/requested/skipped cycles and average cycles with/without
  enriched standings construction.
- `source_stage_performance`: snapshot, REST reception, session processing,
  standings-state update, full roster build, warnings and frame/strategy work.
- Frontend samples: average/max render duration and item count.

## Pending measurement

Cycle-aligned cadences, panel/body containment, short native shadows, the removal
of root `text-rendering: geometricPrecision` and the quantised panel scale are
implemented but not yet measured. So are three later changes, whose expected
effects differ and must be read separately in the same capture:

- Shared Chromium arguments for every webview. Expect private memory to fall and
  CPU/GPU to stay flat; confirm the process count and that the control panel
  still repaints correctly after being covered by the game.
- The host bounded to the visible panels in game mode. This is the change aimed
  at the game's own frametime, so measure GPU average/P95 and, preferably,
  LMU frametime with and without the overlay rather than only the overlay's CPU.
  Confirm click-through, edit mode, monitor switch and a panel cropped at the
  monitor edge.
- Pre-rasterised country flags and manufacturer logos (2.8 MB of SVG to 270 KB of
  PNG). Expect lower memory and a cheaper first Standings/Relative paint; confirm
  the icons still look correct at 100%, 125% and 150%. The moving capture below must report GPU
average/P95 before and after, and the release language pass must confirm the text
metric change at 100%, 125% and 150%.

## Current validation target

The stationary all-overlay pass validated grouped native batches and per-overlay
field projection. The next publication-quality check is a comparable moving race
or replay capture.

1. Use a real Tauri production build with embedded `frontendDist`; a plain Cargo
   release build can retain the development URL and is invalid for WebView2 tests.
2. Use the same Standings/Fuel configuration and visible overlays as the baseline.
3. Disable detailed logging for the external comparison, then run a separate
   internal diagnostic pass.
4. Generate the external CSV with `tools/performance/compare-overlays.ps1`.
5. Confirm the source loop is near 50 Hz with zero or near-zero overruns.
6. With Standings visible, requested roster cycles should be about one fifth of
   source cycles; with Relative, about two fifths. With neither nor OBS demand,
   requested cycles should be zero.
   An OBS route other than Standings/Relative must also leave roster demand at zero.
7. Check flags, Rejoin, driver swaps, GAP/INT, pit timers, host count, geometry,
   click-through behavior and Track Map geometry/pit prediction.

Do not perform another speculative optimization until this moving capture
identifies the largest remaining cost.

## Likely measured follow-ups

- Reduce String/HashMap churn in full roster construction only if it remains the
  dominant cost.
- Re-measure WebView2 memory/GPU composition for one- and multi-monitor hosts.
- Investigate large image decode/memory cost without breaking offline assets.

These are hypotheses, not approved changes.

## Principles

- Measure before and after with the same workload.
- A successful build is not performance evidence.
- Match cadence to human-visible need without reducing safety-warning response.
- Keep the native composite free of CSS animation, transition and backdrop-filter
  work that causes WebView2 to present at monitor refresh. Native motion must be
  driven by bounded overlay cadence; standalone/OBS decoration is independent.
- Keep every delivery cadence an exact multiple of the profile's fast cadence, in
  whole source cycles. Overlapping updates share one presented frame; drifting
  ones multiply the presents the transparent host costs the game.
- Bound the rasterised area per repaint: containment on the panel and the embedded
  body, short shadows instead of wide blurs, and a quantised panel scale.
- Cache static data and derive slow values outside 50 Hz.
- Avoid serialization when no consumer exists.
- Prefer bounded channels and stale-data fallbacks over blocking the hot loop.

Completed investigations and exact historical baselines are in
`docs/PERFORMANCE_HISTORY.md`.
