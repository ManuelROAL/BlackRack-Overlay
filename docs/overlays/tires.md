# Damage + Tyres overlay

## Scope and files

- Entry: `tires.html`
- Renderer/style: `src/tires.ts`, `src/tires.css`
- Shared-memory conversion: `src-tauri/src/telemetry/lmu_bridge.cpp`,
  `src-tauri/src/telemetry/lmu.rs`
- REST supplement: `src-tauri/src/telemetry/lmu_rest.rs`
- OBS route: `/tires`
- Cadence: 50 Hz base telemetry

This is the localized schematic. Percentage-only aggregate reporting belongs to
Detailed Damage (`damage.md`).

## Tyre and brake semantics

- Wheel order is front-left, front-right, rear-left, rear-right.
- Match Dox's LMU tyre reading: 34% carcass temperature plus 22% from each of the
  three inner-layer samples, then convert Kelvin to Celsius.
- Convert live `mBrakeTemp` from Kelvin to Celsius despite the inherited SDK
  comment.
- `mWear` is remaining tread (`1.0` new), so show `mWear * 100`.
- Estimate flat spot by accumulating actual tread loss while at least half of the
  contact patch is sliding and the tyre's peripheral speed is at least 30% below
  its per-wheel ground speed. This detects localized dragging during both braking
  lockups and unbraked spins without counting a freely rotating lateral slide.
  Because LMU may leave the contact-patch fraction unavailable, active braking is
  the fallback qualifier for a wheel already below the same slip-ratio threshold.
  Reset after a pit tyre change or new session.
- Compound, puncture and detached-wheel states come from each telemetry wheel.

## Chassis and suspension mapping

- Keep eight independent chassis zones and four independent wheel modules.
- Map raw `mDentSeverity` to the top-down 3-2-3 layout: front `1/0/7`, centre
  sides `2/6`, rear `3/4/5`. Indices 0 and 4 are centre sections; never bind the
  array sequentially as four left/right pairs.
- Each suspension shape uses its matching per-wheel REST wearable value. Missing
  REST data remains unavailable and must not fall back to body damage.
- Suspension coloring begins at 2% and uses `2/15/40/80` thresholds. A detached
  wheel overrides only its matching suspension to 100%.
- Keep rear wheel modules aligned with the raised rear axle and aggregate damage
  inside the central body; do not restore a footer.

## Rear wing and alerts

- Keep the rear-wing SVG neutral gray. Do not infer its loss from shared-memory
  `mDetached`, rear-centre dent severity or aggregate REST aero wear: `mDetached`
  covers every non-wheel detachable part and aero wear can exceed 200% while the
  rear wing remains attached. LMU's REST `detachableParts[]` is vehicle-specific
  and does not provide stable semantic part identifiers.
- Continue exposing aggregate REST aero wear through the wing tooltip without
  presenting it as a detached rear wing.
- Blink only the affected tyre shape: orange for puncture, red for detachment,
  with detachment taking priority. Numeric readings stay continuously visible.

## Presentation and hot path

- Use the same restrained 2 px lime left-edge accent as Trailing + Pedal. Keep it
  visible in click-through mode as the product mark without changing panel width.
- Each label-free corner stack has fixed order: tyre temperature, remaining tread,
  flat spot and disc temperature. Give all four readings the same large, heavy
  numeric treatment.
- At larger text sizes, widen and separate the four corner stacks while keeping
  the chassis and tyre graphics at their established visual size. Keep the
  chassis centered on both axes of the adaptive panel instead of anchoring it to
  fixed top or left offsets.
- Suspension damage is color-only on its SVG. Keep compound in the wheel tooltip.
  Keep each suspension glyph subordinate to its tyre and brake graphic.
  Do not restore corner cards, suspension percentages, compound icons or repeated
  corner/value abbreviations.
- Cache nodes and skip unchanged text, attributes, datasets and CSS variables in
  the 50 Hz renderer.

## Verification focus

Test all four corners independently, temperature conversion, remaining tread,
flat-spot reset, suspension source mapping, puncture/detachment priority, wing
false positives and REST-unavailable fallback.

## Localization

Labels, detailed tyre/chassis tooltips and accessibility text use the bundled
locale. Translation does not add catalog construction or DOM churn to the 50 Hz
renderer. Its OBS route follows the shared saved locale or a page-local `?lang=`
override.
