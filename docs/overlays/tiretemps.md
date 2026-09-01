# Detailed tyre temperatures overlay

## Scope and files

- Entry: `tiretemps.html`
- Renderer/style: `src/tiretemps.ts`, `src/tiretemps.css`
- Shared heatmap: `src/temperature-colors.ts`
- Shared-memory conversion: `src-tauri/src/telemetry/lmu_bridge.cpp`, `src-tauri/src/telemetry/lmu.rs`
- OBS route: `/tiretemps`
- Cadence: 50 Hz base telemetry

This panel complements Damage + Tyres with independently selectable surface,
inner-layer and carcass temperatures for every wheel plus selectable brake
temperatures. Damage and wear remain in their own overlays.

## Sources and semantics

- Wheel order is front-left, front-right, rear-left, rear-right.
- Names match TinyPedal's widgets: Tyre surface temperature, Tyre inner layer
  temperature, Tyre carcass temperature and Brake temperature.
- `mTemperature[0/1/2]` supplies the surface temperature at the physical
  left/centre/right of each tyre. These are not inside/centre/outside indices.
- `mTireInnerLayerTemperature[0/1/2]` supplies the corresponding
  left/centre/right samples from the innermost rubber layer before the carcass.
- `mTireCarcassTemperature` supplies one rough carcass average per tyre.
- Every tyre field and `mBrakeTemp` is converted from Kelvin to Celsius.

## Presentation and invariants

- Each wheel shows one row per enabled depth, ordered like TinyPedal: surface,
  inner layer and carcass. Surface and inner-layer rows contain their three
  physical left/centre/right values; carcass contains its single average. No
  visible text labels are used; each band shows its own rounded Celsius value. The
  heatmap follows TinyPedal's compound targets: wet/intermediate 50 C, soft 80 C,
  medium 90 C and hard 100 C. Its nine bands change at `-30/-20/-10/0` and
  `+10/+20/+30/+40 C` around that target, expressed with BlackRack's own palette.
  Damage + Tyres reuses this same compound-aware scale.
- Brake discs sit between the tyres, aligned by axle, with their numeric Celsius
  value and a 100 C stepped cold-to-hot scale matching TinyPedal's behavior.
- Each of the four reading groups can be enabled independently. All four are
  visible by default; at least one group must remain visible.
  Hiding a tyre reading removes its column without changing the panel height.
- Round values before selecting a heatmap band, as TinyPedal does. Cold blue bands
  use light text; the remaining brighter bands use dark text for legibility.
- The panel uses the compact dark surface, neutral outline and 2 px lime accent.
  It omits wheel-corner and temperature labels; position and the fixed
  surface/inner-layer/carcass order provide the context. Its default footprint is
  258 x 136 px. Puncture and
  detachment affect the tyre outline without hiding live readings.
- Cache DOM nodes through the static document and skip unchanged text, title and
  CSS-property writes in the 50 Hz renderer.
- Native composite mode has no animation, transition or backdrop filter.

## Verification focus

Test all surface, inner-layer and carcass samples independently, the direct
left/centre/right zone order and Kelvin conversion, every visibility combination,
brake values and colors, puncture/detachment outlines, locale labels, native
composite projection and the `/tiretemps` OBS route.
