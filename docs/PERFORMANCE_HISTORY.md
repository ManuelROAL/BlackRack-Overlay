# Performance investigation history

This file preserves completed measurements. The active procedure and next target
live in `docs/PERFORMANCE.md`.

## Implemented optimizations

- Vite assets are not inlined; production JS uses moderate Terser settings.
- Vehicle identity and Standings DOM nodes are cached; renderers update only
  changed values.
- Full Standings is built on demand at 10 Hz, or 20 Hz for Relative, rather than
  every 50 Hz source cycle. Coincident consumers reuse it.
- REST/RaceOS work runs on background threads; browser delivery is bounded and
  inactive without clients.
- Panels share one WebView per monitor instead of one renderer per overlay.
- Track Map uses a lightweight approximately 30 Hz roster, static SVG geometry,
  HTML markers and direct transforms without continuous transitions/filters.
- Dashboard and Trailing + Pedal removed 50 Hz CSS transitions. Driving preserves
  50 Hz samples while repainting its canvas at 25 Hz.
- Damage + Tyres skips unchanged writes in its 50 Hz path.
- Native telemetry is grouped by payload variant, then projected to reused
  per-overlay field allowlists before iframe `postMessage`.

## Single overlay host on the selected monitor — 2026-08-18

Removed per-overlay monitor assignment. The app now creates one transparent host
window on the user-selected monitor and moves/resizes it when the selection
changes; monitors without the host contribute no composed surface. Configuration
schema moved to v5 (`overlays.monitor` instead of per-overlay `monitorSelection`;
layout placements no longer carry a monitor). Legacy v1-4 configurations and the
old localStorage monitor-selection record are migrated on first run. Rust config
validation now accepts schema versions 1-5.

Rationale: with per-monitor hosts, any visible overlay showed a full-screen
transparent WebView2 host on every monitor, including empty hosts on secondary
monitors. DWM had to alpha-composite that surface at the secondary refresh rate
(for example 60 Hz), degrading game FPS. A single host on the selected monitor
removes that surface entirely.

A controlled A/B capture comparing this build with the previous release on the
180 Hz / 60 Hz two-monitor setup is still pending.

## Track Map GPU diagnosis — 2026-08-13

Continuous 110 ms marker transitions kept WebView2's GPU process around 1.67% CPU
with Task Manager peaks near 3.2%. Direct 30 Hz updates reduced that process to
0.365% in a 15-second sample; the complete BlackRack Overlay/WebView2 tree measured about
1.08%. This isolated development diagnosis confirmed the cause but was not a
publication comparison.

## Invalid Cargo-only release sample

A stationary `cargo build --release` measurement was discarded because the
binary retained Tauri's development URL and rendered a localhost error page.
Production measurements require `tauri build` (optionally `--no-bundle`) so
`frontendDist` is embedded.

## Verified production stationary comparisons

All eleven overlays were visible in the same stationary pit-lane scene.

- Installed 0.4.0: 10.402% average CPU, 11.983% P95 CPU, 2.533% average GPU and
  3% P95 GPU.
- First optimized build: 9.609% average CPU, 11.596% P95 CPU, 2.073% average GPU
  and 3% P95 GPU: about 7.6% lower average CPU and 18.2% lower average GPU.
- Reinstalled 0.3.2 reference: 10.353% average CPU, 12.102% P95 CPU, 2.7% average
  GPU, 3% P95 GPU, 1012 MB average private memory and nine processes.

Memory was excluded because cache warm-up and run order were not controlled.

## Grouped native batches

An immediate fresh 0.4.0 baseline measured 10.298% average CPU, 11.897% P95 CPU,
2.143% average GPU and 3% P95 GPU. The grouped build measured 6.081% average CPU,
7.332% P95 CPU, 2.083% average GPU and 3% P95 GPU; a second pass reproduced 6.081%
average and 7.243% P95 CPU. This was about 40.9% lower average CPU and 38.4% lower
P95 CPU; GPU was effectively unchanged.

Source CSV stems:

- `overlay-performance-20260813-202255`
- `overlay-performance-20260813-202458`
- `overlay-performance-20260813-202614`

## Iframe delivery experiments

Direct same-origin cross-realm `CustomEvent` delivery measured 6.207% and 6.267%
average CPU, worse than the 6.081% grouped `postMessage` baseline, so it was
discarded.

Keeping `postMessage` and projecting only consumed fields measured 5.969% and
5.807% average CPU with 7.075% and 7.140% P95 CPU. The combined 5.888% average was
about 3.2% below the grouped baseline and 42.8% below the immediate installed
0.4.0 baseline.

Source CSV stems:

- `overlay-performance-20260813-204942`
- `overlay-performance-20260813-205052`
- `overlay-performance-20260813-210044`
- `overlay-performance-20260813-210154`

All stationary results remain directional. A controlled moving replay is still
required for publication-quality confirmation.
