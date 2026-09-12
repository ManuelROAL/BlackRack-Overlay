# Performance investigation history

This file preserves completed measurements. The active procedure and next target
live in `docs/PERFORMANCE.md`.

## Where the plateau lives — 2026-09-02

The first capture with per-process attribution answers what the totals never
could. Twenty-one minutes, memory read from the last sixteen:

- Overlay host renderer: 829 MB average, 982 MB maximum.
- GPU process: 201 MB.
- Control panel renderer: 58 MB.
- Browser process 47 MB, Rust process 30 MB, utilities 22 MB, crashpad 3 MB.
- Total 1190 MB average over the warm window, and 1155 MB over the last nine
  minutes, where it is flat.

The host renderer and the GPU process are 87% of the application. Everything
else together is 160 MB, and the Rust process confirms its 30 MB.

Memory also overshoots before it settles. Per three-minute band the total reads
770, 1219, 1286, 1182, 1149, 1161 and 1153 MB: the ramp peaks near minute six
and then relaxes about 130 MB onto a flat plateau. A window that ends at minute
six therefore reads the peak rather than the plateau, which is the same trap the
ramp already set one step earlier. CPU is homogeneous, 2.88% average against a
2.70% median, and settles at 2.78%.

This retires the control panel as a target. Destroying its renderer while the
panel is hidden would free 58 MB of 1190, about 5%, and would cost decoupling
"hide" from "quit" on the only window the application has.

What the capture does not say is what the 829 MB is. The JS heap measured flat
at 33 MB in the same configuration, so about 96% of the host renderer is not
script: compositing tiles, the image decode cache and Blink's own structures for
the mounted overlay documents.

Source CSV stem: `overlay-performance-20260902-214705`.

## Memory warm-up profile — 2026-09-02

A 21-minute capture of the renderer processes settled the question of whether
memory grows without bound. It does not: the application warms up and then holds
a working set.

- Ramp: 377 to 1002 MB in 4.3 minutes, 163 MB/min.
- Plateau: 16.6 minutes, 1041 MB average, 986 to 1108 MB, residual 2.47 MB/min.
  The two halves of the plateau average 1033 and 1049 MB, which the sawtooth
  alone can account for.

Every 300 s capture therefore ends inside the ramp, so its memory figure is an
arbitrary point on a warm-up curve and depends on how long the application had
been running when the capture started. The historical 909 MB, and the 605 and
807 MB measured the same day, are not comparable to each other for that reason.
CPU is unaffected and valid from the first sample.

This retracts an earlier reading of the same data as a 105 MB/min leak. Three
supporting observations that also dissolve with it: a heap snapshot showed the
JS heap flat at 32.9 to 33.7 MB, which is correct because nothing was leaking;
per-overlay slopes of 7.5 (Standings), 27.3 (Fuel) and 15.8 MB/min (Track Map)
did not add up to the whole, because each measured a different portion of a
different ramp; and the Rust process stayed at 29 MB throughout while the host
renderer held about 700 MB.

## Chromium arguments and the V8 heap cap — 2026-09-02

Explicit browser arguments were added to every webview. Bounding the V8 heap
with `--js-flags=--max-old-space-size=192` held private memory near 605 MB
instead of about 1 GB, but left the host renderer against the limit: CPU burst
to 42-48% of every logical processor for roughly 30 s every 80 s, four times in
five minutes. Removing the cap removed the bursts and produced the lowest CPU of
the three builds compared that day.

- Before the changes: 2.63% median CPU, 6.52% maximum.
- With the heap cap: 2.54% median, 48.76% maximum.
- Without the cap: 2.21% median, 4.39% maximum.

Source CSV stems: `overlay-performance-20260902-154158` (before),
`overlay-performance-20260902-154739` (capped) and
`overlay-performance-20260902-160456` (final).

## Frame cost to the game — 2026-09-02

PresentMon 2.5.1 measured LMU with and without the overlay over the same replay
segment at 177 FPS, uncapped, in Hardware Composed: Independent Flip. The game
keeps that presentation mode with the overlay active.

- MsBetweenPresents: 5.652 to 5.661 ms, +0.008 ms average, +0.386 ms at P99.
- MsGPUBusy: +0.147 ms average. MsCPUBusy: +0.234 ms. MsGPUWait: -0.139 ms.

The overlay does real work and it fits in the slack: with 2.3 ms of GPU wait per
frame there was room for it, so frame delivery barely moved. On a GPU-bound
configuration without that slack the 0.147 ms would show up directly.

A capture of the WebView2 GPU process itself showed two swap chains, both
Composed: Flip: the overlay host at 46.8 presents/s with its interval histogram
peaked exactly on the 20 ms source cycle, and a second window at 2.0/s. The host
presents on the source cadence rather than at monitor refresh, which is what the
cadence alignment is for.

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

## Demand-driven per-monitor hosts — 2026-09-12

The former single-host limitation is replaced by demand-driven grouping. The
control panel persists a monitor on each layout placement, and Tauri creates one
composite host for every monitor that currently has visible overlays. Moving all
overlays to one monitor still creates one host; splitting them across two
monitors creates two hosts; monitors with no visible overlays create none.

This restores independent monitor placement without returning to one native
window/WebView per overlay. It has not yet been performance-captured, so the
next A/B run must verify mixed-refresh game FPS, host count, renderer/GPU memory
and CPU against the single-host baseline above.

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
