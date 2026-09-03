# Damage + Tyres overlay

## Scope and files

- Entry: `tires.html`
- Renderer/style: `src/tires.ts`, `src/tires.css`
- Settings: `src/tires-settings.ts`
- Engine assets: `src/assets/lmu-icons/damage/engine-motor.svg`,
  `oil.svg`, `temperature-water.svg`
- Shared-memory conversion: `src-tauri/src/telemetry/sim/lmu/bridge.cpp`,
  `src-tauri/src/telemetry/sim/lmu/source.rs`, `src-tauri/src/telemetry/sim/lmu/source/frame.rs`
- REST supplement: `src-tauri/src/telemetry/sim/lmu/rest.rs`
- OBS route: `/tires`
- Cadence: 50 Hz base telemetry

This is the localized schematic. Percentage-only aggregate reporting belongs to
Detailed Damage (`damage.md`).

## Tyre and brake semantics

- Wheel order is front-left, front-right, rear-left, rear-right.
- Match Dox's LMU tyre reading: 34% carcass temperature plus 22% from each of the
  three inner-layer samples, then convert Kelvin to Celsius.
- Replicate LMU's HUD by rendering the wider tyre as three contiguous bands from
  the surface `mTemperature[3]` samples in physical left/center/right order. Keep
  the bands seamless, with no gaps or borders, and interpolate continuously
  between each zone's compound-aware heatmap color so temperature steps do not
  create a hard edge. The numeric reading remains the Dox/MFD-style internal
  composite above rather than any surface sample.
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
- Tyre and brake colors reuse the shared compound-aware heatmaps in
  `src/temperature-colors.ts`, including rounded values before band selection.

## Chassis and suspension mapping

- Keep eight independent chassis zones and four independent wheel modules. Render
  the chassis as a thin game-style perimeter: three front segments, two side
  segments and three rear segments, without filled bodywork or cockpit detail.
- Map raw `mDentSeverity` to the top-down 3-2-3 layout: front `1/0/7`, centre
  sides `2/6`, rear `3/4/5`. Indices 0 and 4 are centre sections; never bind the
  array sequentially as four left/right pairs.
- Each suspension shape uses its matching per-wheel REST wearable value. Missing
  REST data remains unavailable and must not fall back to body damage.
- Suspension coloring begins at 2% and uses `2/15/40/80` thresholds. A detached
  wheel has its independent detached state; use 100% as the numeric suspension
  fallback only when REST wearables are unavailable.
- Keep rear wheel modules aligned with the raised rear axle and aggregate damage
  inside the central outline; do not restore a footer.

## Rear wing and alerts

- Keep the rear-wing SVG neutral gray. Do not infer its loss from shared-memory
  `mDetached`, rear-centre dent severity or aggregate REST aero wear: `mDetached`
  covers every non-wheel detachable part and aero wear can exceed 200% while the
  rear wing remains attached. LMU's REST `detachableParts[]` is vehicle-specific
  and does not provide stable semantic part identifiers.
- Continue exposing aggregate REST aero wear through the wing tooltip without
  presenting it as a detached rear wing.
- Standalone and OBS pages blink only the affected tyre shape: orange for
  puncture, red for detachment, with detachment taking priority. The native
  composite holds the same alert color statically so an alert cannot force
  WebView2 to present at monitor refresh. Numeric readings stay visible.
- Show a compact engine SVG inside the chassis. Keep it neutral normally and turn
  it red only from shared memory's official `mOverheating` engine-warning signal;
  zero RPM, ignition state and body damage must not infer an engine failure.
- Oil and water temperature are independent, optional readings sourced directly
  from `mEngineOilTemp` and `mEngineWaterTemp` in Celsius. Place their supplied
  SVG icons and compact values around the engine icon inside the chassis: oil
  above and water below. Both are hidden by default, persist independently and
  mirror to the OBS route. Keep aggregate damage below the lower active reading.
  Space every visible item in this central stack evenly, including when either
  optional temperature is hidden.

## Presentation and hot path

- Use the same restrained 2 px lime left-edge accent as Trailing + Pedal. Keep it
  visible in click-through mode as the product mark without changing panel width.
- Each label-free corner stack has fixed order: tyre temperature, remaining tread,
  flat spot and disc temperature. Each reading is independently optional, persists
  and mirrors to OBS; all four remain enabled by default. Give visible readings
  the same large, heavy numeric treatment.
- At larger text sizes, widen and separate the four corner stacks, scale the
  chassis schematic and its engine/oil/water SVGs with the panel's vertical text
  expansion, and keep the tyre graphics at their established visual size. Keep
  the chassis centered on both axes of the adaptive panel instead of anchoring it
  to fixed top or left offsets. Each reading track must grow with the full font
  scale, even when the panel itself uses a more compact expansion ratio, and its
  line height must remain at least large enough for the scaled glyphs.
- Reserve a visible gap between each suspension and the chassis at the minimum
  layout width. Migrate the former 174 px default to 194 px so the wider
  three-band tyres cannot push the suspension into the car outline.
- Suspension damage is color-only on its SVG. Keep compound in the wheel tooltip.
  Keep each suspension glyph subordinate to its tyre and brake graphic.
  Keep tyre, brake and suspension shapes in separate tracks with a small visible
  gap between each shape.
  Do not restore corner cards, suspension percentages, compound icons or repeated
  corner/value abbreviations.
- Cache nodes and skip unchanged text, attributes, datasets and CSS variables in
  the 50 Hz renderer.
- Keep the chassis interior limited to the engine status and aggregate damage; do
  not show the tyre-life estimate there.

## Verification focus

Test all four corners and three tyre bands independently, temperature conversion,
remaining tread, flat-spot reset, suspension source mapping,
puncture/detachment priority, wing false positives, engine-warning state and
REST-unavailable fallback. Verify the four independent wheel-reading toggles and
oil/water visibility, reset/import/export persistence and OBS preference mirroring.

## Localization

Labels, detailed tyre/chassis tooltips and accessibility text use the bundled
locale. Translation does not add catalog construction or DOM churn to the 50 Hz
renderer. Its OBS route follows the shared saved locale or a page-local `?lang=`
override.
