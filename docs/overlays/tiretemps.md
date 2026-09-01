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
- The three existing `mTemperature[0/1/2]` samples correspond respectively to
  surface, inner layer and carcass temperature for each wheel.
- All three tyre samples and `mBrakeTemp` are converted from Kelvin to Celsius.

## Presentation and invariants

- Each wheel shows its enabled temperatures in one horizontal line, ordered like
  TinyPedal: surface, inner layer and carcass. No visible text labels are used;
  each band shows its own rounded Celsius value. The
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

Test all surface, inner-layer and carcass samples independently, direct `0/1/2`
depth mapping and Kelvin conversion, every visibility combination, brake
values and colors, puncture/detachment outlines, locale labels, native composite
projection and the `/tiretemps` OBS route.
