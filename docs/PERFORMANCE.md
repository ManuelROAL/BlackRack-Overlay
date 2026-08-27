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
7. Check flags, Rejoin, driver swaps, GAP/INT, pit timers, host count, geometry,
   click-through behavior and Track Map geometry/pit prediction.

Do not perform another speculative optimization until this moving capture
identifies the largest remaining cost.

## Likely measured follow-ups

- Reduce String/HashMap churn in full roster construction only if it remains the
  dominant cost.
- Make browser-source roster demand route-aware if unrelated OBS pages still force
  enriched Standings data.
- Re-measure WebView2 memory/GPU composition for one- and multi-monitor hosts.
- Investigate large image decode/memory cost without breaking offline assets.

These are hypotheses, not approved changes.

## Principles

- Measure before and after with the same workload.
- A successful build is not performance evidence.
- Match cadence to human-visible need without reducing safety-warning response.
- Cache static data and derive slow values outside 50 Hz.
- Avoid serialization when no consumer exists.
- Prefer bounded channels and stale-data fallbacks over blocking the hot loop.

Completed investigations and exact historical baselines are in
`docs/PERFORMANCE_HISTORY.md`.
