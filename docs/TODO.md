# Current work and roadmap

## Immediate next step

The stationary all-overlay pass has validated the grouped native telemetry events:
the subsequent per-overlay field projection measured 5.969% and 5.807% average
CPU versus 10.298% for the immediately preceding installed 0.4.0 baseline. The
next publication check is a comparable moving race or replay capture. Use a real
Tauri production build with embedded `frontendDist`; a plain Cargo release build
can retain the development URL and is not a valid WebView2 measurement.

1. Build/run the current code.
2. Enable analysis logging only for the internal diagnostic pass.
3. Keep standings and fuel enabled with the same row/column configuration used in
   the previous comparison.
4. Generate a new external performance CSV using
   `tools/performance/compare-overlays.ps1`.
5. Compare the new JSONL `performance_sample` and `source_stage_performance` events,
   especially source cycles with and without standings. Confirm the base loop is
   near 50 Hz with zero or near-zero overruns.
6. Confirm no regression in flags, rejoin, driver swaps, gap/interval and pit timer.
7. Confirm each monitor has one host, panel geometry survives restart and monitor
   reassignment, and game mode remains click-through across the complete display.
8. Validate Track Map's official type-0/type-1 geometry, lap-distance alignment
   and complete-pit-passage filter on at least one additional circuit.

Do not perform another speculative optimization until the moving capture identifies
the largest remaining cost.

## Likely future optimization candidates

- If the full standings build is still expensive, reduce String/HashMap churn in
  `LmuTelemetrySource::standings` with reusable buffers or stable identity records.
- Make browser-source demand route-aware: currently any connected browser client
  requests a full standings frame, even if it only displays fuel/flags/dashboard.
- Re-measure WebView2 memory and GPU composition with the per-monitor host model,
  including one-monitor and multi-monitor configurations.
- Investigate image decode/memory behavior for the large country/manufacturer asset
  set without breaking offline distribution.

These are hypotheses, not approved changes; use measurements first.

## Product roadmap

- Validate and implement live telemetry under Proton/Linux.
- Add repeatable session capture/replay fixtures for development without LMU.
- Expand Rust model tests for Standings/Relative selection and frontend tests for
  presentation-only formatting.
- Improve failure diagnostics and release automation/CI.
- Continue refining yellow causation if LMU exposes a stable official signal.

## Known constraints

- RaceControl/RaceOS endpoints are undocumented client behavior and may change.
- Yellow causation is inferred because the exact internal signal is not present in
  the currently consumed shared-memory subset.
- A build without the detected LMU SDK silently uses mock telemetry after emitting
  a Cargo warning; verify the warning when preparing Windows releases.
- PowerShell may block `npm.ps1`; use `npm.cmd` in commands and documentation.
