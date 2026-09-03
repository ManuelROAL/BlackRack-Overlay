# Dashboard overlay

## Scope and files

- Entry: `dashboard.html`
- Renderer/style: `src/dashboard.ts`, `src/dashboard.css`
- Preferences: `src/dashboard-settings.ts`
  (`blackrack-overlay.dashboard.v1`, event `dashboard://settings`)
- Shared helpers: `src/temperature-colors.ts` for the tyre and brake ranges
- Source fields: `src-tauri/src/telemetry/sim/lmu/bridge.cpp` and
  `src-tauri/src/telemetry/sim/lmu/source/frame.rs`
- Capability: `car_electronics`; the control-panel card is disabled for a source
  that does not publish it
- OBS route: `/dashboard`
- Cadence: the fast overlay cadence (50 Hz Smooth). Gear, speed and the rev bar
  are the reason; the rest of the panel changes far more slowly but rides along
  on the same emission.

## Layout

One instrument cluster stacked as four bands, each of which disappears when
nothing inside it is visible:

1. **Top** — limiter/lights/wipers lamps, the delta to the active reference,
   and the session block: remaining time, overall position, class position and
   lap.
2. **Main** — the tyre block (2×2 with the fitted compound in the channel on
   the axle line), the core column (rev bar, then the gear beside the speed and
   the air/track temperatures) and the lap times: predicted, last and best, each
   as a label and a time on one line.
3. **Hybrid** — battery bar, regeneration, motor temperature and the deployment
   state. Present only on a car that carries an electric boost system.
4. **Bottom** — the wheel electronics row and the fuel block.

## Data semantics

- Everything comes from the selected vehicle, so spectator and team mode read
  the followed car rather than the player's.
- The panel draws nothing until `player_active`; before that it shows the
  waiting copy rather than a cluster full of dashes.
- Each selectable system is published as a current value and the car's maximum:
  engine map (`mMotorMap`), traction control (`mTC`), its slip and cut trims
  (`mTCSlip`, `mTCCut`), ABS (`mABS`), brake migration (`mMigration`) and the
  front/rear anti-roll bars (`mFrontAntiSway`, `mRearAntiSway`). Brake bias is
  the existing `brake_bias_percent`, already the front share.
- `car_electronics_available` is derived from the published maxima rather than
  from the values: a neutral setting and an absent one are both zero, so only a
  non-zero maximum proves the car publishes the system. Every maximum stays at
  zero until the car is on track with its setup loaded.
- Tyre pressure is `mPressure` in kPa, rendered in psi. Tyre and brake
  temperature reuse the same colour ranges as the Damage and tyres overlay, so a
  corner reads identically in both panels.
- Battery charge reads `mSoC` when the car fills it and falls back to
  `mBatteryChargeFraction`. The two do not share a scale — the fraction is
  documented as 0..1 while the state of charge has been seen as a percentage —
  so the bridge normalises whichever one is present and clamps it to 0..100.
- `hybrid_available` is true when the car reports an electric boost motor state
  other than `unavailable` or any charge at all. The GT and LMP2 cars sharing an
  endurance grid report neither, so the whole hybrid band is removed for them
  instead of drawing an empty battery a driver would read as a flat one.
- The motor state maps LMU's `mElectricBoostMotorState`: 1 inactive, 2
  propulsion, 3 regeneration. State 0 (unavailable) hides the state pill.
  Regeneration power is `mRegen` in kW.
- Overall position is the player's `mPlace`. Class position and class size are
  counted in the bridge over the scoring class name, which is filled for every
  car including those without per-vehicle telemetry, so no car is left out of
  the count.
- The fuel block shows litres, average consumption per lap and the remaining
  range. When virtual energy is active it also shows the energy percentage, and
  the range becomes whichever of the two budgets runs out first, because that is
  the one that actually ends the stint.
- Delta and lap times come from `delta_model` and `timing_model`. Both are built
  on demand, so the source loop treats an active Dashboard as a request for
  them exactly like the Delta and Timing overlays.

## Presentation

- The cell whose value has just changed stays highlighted in the accent colour
  for a bounded number of telemetry updates. The decay is counted in frames, not
  kept on a timer: the native composite host must repaint from telemetry only.
- The battery bar turns amber below 40% and red below 15%; a tyre wear bar turns
  amber below 35% and red below 15%; the rev bar and the gear turn amber from
  90% of the limit and red from 97%.
- Every block and every electronics/hybrid readout is independently optional.
  `tcslip`, `tccut` and the motor temperature start hidden. These preferences
  also apply to the OBS route and to configuration import/export.
- The panel is meant to be read at a glance while the eyes are on the road, so
  it is drawn as tight as it can be and still be legible: a single 7 px label
  scale, no frame or fill around a tyre corner, the gear beside the speed rather
  than above it, and label-and-time on one line for the lap times. Use a 460 px
  base width; the height follows the visible content through
  `fitOverlayToContent` and lands near 170 px with every block on. Keep the
  shared 2 px translucent lime accent along the shell's left edge.

## Invariants

- Never infer availability from a value alone. Map 0, TC 0 and 0% charge are all
  legitimate settings; the maxima and `hybrid_available` are the only signals.
- The overlay owns presentation only. Normalisation of the charge scale, the
  availability flags, the class-position count and the motor state clamp stay in
  Rust and the bridge.
- The panel repeats data other overlays own on purpose — it is the one surface
  meant to be read without moving the eyes. Keep the shared helpers shared
  rather than re-deriving a value here.

## Verification focus

Check a Hypercar and a GT car in the same session: the hybrid band must appear
for one and be absent for the other. Confirm the maxima appear once the car is on
track, that changing a rotary highlights only its own cell, and that the delta
and lap times fill in with the Delta and Timing overlays closed. Check class
position on a multiclass grid, the pressure reading against the car's own MFD,
the range with and without virtual energy, the OBS route and a configuration
export/import round trip.

## Localization

Labels, motor state, lamps, waiting copy, document title and accessibility text
use the bundled locale; values remain format-only. Its OBS route follows the
shared saved locale or a page-local `?lang=` override.
