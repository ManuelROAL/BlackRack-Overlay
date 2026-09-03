# Dashboard overlay

## Scope and files

- Entry: `dashboard.html`
- Renderer/style: `src/dashboard.ts`, `src/dashboard.css`
- Preferences: `src/dashboard-settings.ts`
  (`blackrack-overlay.dashboard.v1`, event `dashboard://settings`)
- Source fields: `src-tauri/src/telemetry/sim/lmu/bridge.cpp` and
  `src-tauri/src/telemetry/sim/lmu/source/frame.rs`
- Capability: `car_electronics`; the control-panel card is disabled for a source
  that does not publish it
- OBS route: `/dashboard`
- Cadence: the secondary overlay cadence (20 Hz Smooth), the same one Detailed
  Damage and Pit-stop use. Every value here is a discrete driver selection or a
  slow-moving system reading, so a faster cadence would only add repaints.

## Data semantics

- The electronics come from the selected vehicle's own telemetry, so spectator
  and team mode read the followed car rather than the player's.
- Each selectable system is published as a current value and the car's maximum:
  engine map (`mMotorMap`), traction control (`mTC`), its slip and cut trims
  (`mTCSlip`, `mTCCut`), ABS (`mABS`), brake migration (`mMigration`) and the
  front/rear anti-roll bars (`mFrontAntiSway`, `mRearAntiSway`). Brake bias is
  the existing `brake_bias_percent`, already expressed as the front share.
- `car_electronics_available` is derived from the published maxima rather than
  from the values: a neutral setting and an absent one are both zero, so only a
  non-zero maximum proves the car actually publishes the system. Every maximum
  stays at zero until the car is on track with its setup loaded.
- Battery charge reads `mSoC` when the car fills it and falls back to
  `mBatteryChargeFraction`. The two do not share a scale — the fraction is
  documented as 0..1 while the state of charge has been seen as a percentage —
  so the bridge normalises whichever one is present and clamps it to 0..100.
- `hybrid_available` is true when the car reports an electric boost motor state
  other than `unavailable` or any charge at all. The GT and LMP2 cars sharing an
  endurance grid report neither, so the whole hybrid block is removed for them
  instead of drawing an empty battery a driver would read as a flat one.
- The motor state maps LMU's `mElectricBoostMotorState`: 1 inactive, 2
  propulsion, 3 regeneration. State 0 (unavailable) hides the state pill.
  Regeneration power is `mRegen` in kW and the motor temperature is
  `mElectricBoostMotorTemperature`.
- The limiter, headlight and wiper lamps read `mSpeedLimiterActive`,
  `mHeadlights` and `mWiperState`.

## Presentation

- One compact shell with up to three stacked sections: the electronics grid, the
  hybrid row and the lamp row. A section with nothing visible is removed, and a
  panel with all three empty shows the waiting copy instead.
- The electronics grid wraps rather than reflowing through a computed column
  template, so hiding a value simply lets the rest of the row grow.
- The cell whose value has just changed stays highlighted in the accent colour
  for a bounded number of telemetry updates. The decay is counted in frames, not
  kept on a timer: the native composite host must repaint from telemetry only.
- The battery bar turns amber below 40% and red below 15%.
- Every value is independently optional. `tcslip`, `tccut`, the motor
  temperature and the lamp row start hidden: the default is the shortlist a
  driver changes from the wheel during a stint. These preferences also apply to
  the OBS route and to configuration import/export.
- Use a 300 × 150 px base surface and keep the shared 2 px translucent lime
  accent along the shell's left edge.

## Invariants

- Never infer availability from a value alone. Map 0, TC 0 and 0% charge are all
  legitimate settings; the maxima and `hybrid_available` are the only signals.
- The overlay owns presentation only. Normalisation of the charge scale, the
  availability flags and the motor state clamp stay in Rust and the bridge.

## Verification focus

Check a Hypercar and a GT car in the same session: the hybrid block must appear
for one and be absent for the other. Confirm the maxima appear once the car is on
track, that changing a rotary highlights only its own cell and that the highlight
clears without the panel repainting in between. Check the OBS route and a
configuration export/import round trip.

## Localization

Labels, motor state, lamps, waiting copy, document title and accessibility text
use the bundled locale; values remain format-only. Its OBS route follows the
shared saved locale or a page-local `?lang=` override.
