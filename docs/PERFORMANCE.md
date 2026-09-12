# Performance workflow

## Goal and comparison method

Compare BlackRack Overlay with TinyPedal under the same moving LMU replay/race segment
and comparable visible content. Follow `tools/performance/README.md`.
Record the selected performance profile and use the same one in every BlackRack
comparison; Smooth is the full-cadence baseline for captures predating profiles.

Primary external metrics include BlackRack Overlay's WebView2 child processes:

- CPU average and P95
- private memory average and maximum, attributed per process role
- GPU average and P95

Private memory is also broken down by Chromium process role (`app`, `browser`,
`gpu`, `renderer`, `utility`). A total says how much the application costs but
never which part holds it, and the overlay host and the control panel are
separate renderers. Read `-processes.csv` before proposing any memory change:
its first row names the process to attack.

Memory needs a warm-up window that CPU does not. The application ramps for about
four minutes and then holds a plateau, so a 300 s capture ends inside the ramp
and reports an arbitrary point on it. Either start the capture with the
application already warm, or pass `-MemoryWarmupSeconds` so the collector
excludes the ramp from the memory figures while CPU keeps using every sample.
Comparing memory between two runs that were not both warm measures the run
order, not the builds. Memory also overshoots before it settles: the ramp peaks
near minute six and then relaxes about 130 MB, so read the last third of a long
capture rather than the first warm sample. `docs/PERFORMANCE_HISTORY.md` records
the measured profile.

Do not enable detailed telemetry logging during the primary external comparison;
file writes add overhead. Use separate internal-diagnostic passes.

Check that a capture is homogeneous before trusting it. Average and median CPU
should be close; a ratio above about 1.5 means the capture mixed states, such as
the session ending or the overlays auto-hiding partway through, and the summary
averages regimes that never coexisted.

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

## Inspecting the overlay host

Debug builds enable the webview inspector and register `Ctrl+Shift+D`, which
opens it on the overlay hosts. The shortcut exists because the host suppresses
its context menu and is click-through in game mode, so right-clicking can never
reach the inspector. Release builds keep both disabled and a failed
registration is only logged, never fatal.

Use `npm run tauri dev` for this: the frontend is unminified, so a heap snapshot
names real classes and functions. Renderer memory is where growth has been
observed; the Rust process stayed at 29 MB across a six-minute session while the
host renderer reached 704 MB.

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
  the icons still look correct at 100%, 125% and 150%.

The Chromium arguments have been measured and are recorded in the history. Two
questions stay open. The bounded host has never been confirmed to act: GPU
memory read about 140 MB across three captures and the WMI GPU counters reported
nothing for the WebView, so it has to be verified from the host window rectangle
in game mode and from PresentMon against the game. And whether the icon and
argument changes lower the plateau, rather than only the ramp, needs the
21-minute warm capture repeated on the build that precedes them. The moving capture below must report GPU
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
4. Generate the external CSVs with `tools/performance/compare-overlays.ps1`,
   passing `-MemoryWarmupSeconds 300` whenever the run is not already warm.
5. Confirm the source loop is near 50 Hz with zero or near-zero overruns.
6. With Standings visible, requested roster cycles should be about one fifth of
   source cycles; with Relative, about two fifths. With neither nor OBS demand,
   requested cycles should be zero.
   An OBS route other than Standings/Relative must also leave roster demand at zero.
7. Check flags, Rejoin, driver swaps, GAP/INT, pit timers, host count, geometry,
   click-through behavior and Track Map geometry/pit prediction.

Do not perform another speculative optimization until this moving capture
identifies the largest remaining cost.

## Next memory step

Attribution is done. The overlay host renderer holds 829 MB and the GPU process
201 MB, together 87% of the application, while the control panel renderer is
58 MB and the Rust process 30 MB. The JS heap is 33 MB, so what has to be
explained is roughly 800 MB of non-script memory in one renderer.

Two hypotheses remain and one experiment separates them. Take two warm captures
with the same overlays and the same content, changing only the layout: one with
the panels spread so the bounded host covers the whole monitor, one with them
clustered so it covers a small fraction of it.

- If the host renderer scales with the host rectangle, the memory is compositing
  tiles. The lever is then the graphics budget, not the V8 heap that failed:
  `--force-gpu-mem-available-mb` and the discardable limit starve a subsystem
  that has no garbage collector to storm, and must be measured for raster churn
  the same way.
- If it stays flat, the memory belongs to the mounted documents rather than the
  painted area, and the lever is what each overlay iframe costs, measured by
  warm plateau per overlay rather than by slope.

A debug build reaches the same answer faster: `Ctrl+Shift+D` opens the inspector
on the host, and the Layers panel lists every composited layer with its size.
Layer structure is the same in debug even though the totals are not.

Do not change any argument or lifetime before one of these two says which.

## Likely measured follow-ups

- Reduce String/HashMap churn in full roster construction only if it remains the
  dominant cost.
- Re-measure WebView2 memory/GPU composition for one- and multi-monitor hosts.
- Investigate large image decode/memory cost without breaking offline assets.
- Destroying the control panel renderer while it is hidden is measured and
  rejected: 58 MB of 1190, for decoupling "hide" from "quit" on the only window
  the application has.

These are hypotheses, not approved changes.

## Demand-driven per-monitor hosts

The composite architecture now creates one transparent host for each monitor
that has at least one visible overlay. A host is closed when its last visible
overlay moves away or is disabled. Multiple overlays on one monitor therefore
continue to share one WebView2 renderer, while independent monitor placement
adds only one renderer per active monitor. Empty secondary hosts are never kept
alive, preserving the mixed-refresh safeguard from the earlier single-host
decision.

This is an implementation change, not performance evidence. The next warm
capture must compare one-host and two-host layouts with the same overlays,
content, refresh rates and five-minute memory warm-up, recording host count,
game FPS, CPU, GPU process and overlay renderer memory.

## Principles

- Measure before and after with the same workload, and with both runs equally
  warm. Memory needs about five minutes of warm-up; CPU does not.
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
