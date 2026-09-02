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
   Record the selected performance profile and use Smooth for comparison with the
   full-cadence captures made before profiles existed.
3. Keep standings and fuel enabled with the same row/column configuration used in
   the previous comparison.
4. Generate a new external performance CSV using
   `tools/performance/compare-overlays.ps1`.
5. Compare the new JSONL `performance_sample` and `source_stage_performance` events,
   especially source cycles with and without standings. Confirm the base loop is
   near 50 Hz with zero or near-zero overruns.
6. Confirm no regression in flags, rejoin, driver swaps, gap/interval and pit timer.
7. Confirm the single host sits on the selected monitor, panel geometry survives
   restart and monitor switch, and game mode remains click-through across the
   complete display. The host now covers only the visible panels in game mode, so
   check the click-through gaps, the return to the full monitor in edit mode and a
   panel deliberately cropped at the monitor edge.
8. Validate Track Map's official type-0/type-1 geometry, lap-distance alignment
   and complete-pit-passage filter on at least one additional circuit.
9. Validate Delta through a clean lap, invalid lap, pit passage, stint/session
   transition and restart; confirm the persisted overall reference reloads for
   the same track and vehicle.

Do not perform another speculative optimization until the moving capture identifies
the largest remaining cost.

## Likely future optimization candidates

- If the full standings build is still expensive, reduce String/HashMap churn in
  `LmuTelemetrySource::standings` with reusable buffers or stable identity records.
- Re-measure WebView2 memory and GPU composition with the single-host model across
  mixed-refresh (for example 180 Hz / 60 Hz) monitor configurations.
- Investigate image decode/memory behavior for the large country/manufacturer asset
  set without breaking offline distribution.

These are hypotheses, not approved changes; use measurements first.

## Product roadmap

- The Spanish/English localization rollout is complete through phase 4: catalog
  parameters and metadata are validated, visible-copy candidates use a documented
  allowlist, native/OBS surfaces share catalogs and contributor steps are recorded.
  Keep the 100%/125%/150% native WebView2 language pass in release validation and
  add later locales through `docs/LOCALIZATION_CONTRIBUTING.md`.
- Validate and implement live telemetry under Proton/Linux.
- Add repeatable session capture/replay fixtures for development without LMU.
- Expand Rust model tests for Standings/Relative selection and frontend tests for
  presentation-only formatting.
- Improve release automation/CI.
- Continue refining yellow causation if LMU exposes a stable official signal.

## Known constraints

- RaceControl/RaceOS endpoints are undocumented client behavior and may change.
- Yellow causation is inferred because the exact internal signal is not present in
  the currently consumed shared-memory subset.
- A build without the detected LMU SDK silently uses mock telemetry after emitting
  a Cargo warning; verify the warning when preparing Windows releases.
- PowerShell may block `npm.ps1`; use `npm.cmd` in commands and documentation.
