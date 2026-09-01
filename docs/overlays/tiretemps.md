# Detailed tyre temperatures overlay

## Scope and files

- Entry: `tiretemps.html`
- Renderer/style: `src/tiretemps.ts`, `src/tiretemps.css`
- Shared heatmap: `src/temperature-colors.ts`
- Shared-memory conversion: `src-tauri/src/telemetry/lmu_bridge.cpp`, `src-tauri/src/telemetry/lmu.rs`
- OBS route: `/tiretemps`
- Cadence: 50 Hz base telemetry

This panel complements Damage + Tyres with the three live tread temperatures for
every wheel and the four brake-disc temperatures. Damage and wear remain in their
own overlays.

## Sources and semantics

- Wheel order is front-left, front-right, rear-left, rear-right.
- Match TinyPedal's direct `mTemperature[0/1/2]` reading. The LMU SDK defines
  those samples as physical left/centre/right, not inside/centre/outside; display
  all four wheels in that order and never mirror the left-side samples.
- All tyre samples and `mBrakeTemp` are converted from Kelvin to Celsius.
- The white strip uses LMU's `mGripFract`: its width is the live fraction of the
  contact patch that is sliding, anchored to the outside edge of each tyre. It is
  not a fourth temperature sample and does not change color.

## Presentation and invariants

- Each tyre keeps the SDK's left/centre/right order on screen. Every band shows
  its own rounded Celsius value. The
  heatmap follows TinyPedal's compound targets: wet/intermediate 50 C, soft 80 C,
  medium 90 C and hard 100 C. Its nine bands change at `-30/-20/-10/0` and
  `+10/+20/+30/+40 C` around that target, expressed with BlackRack's own palette.
  Damage + Tyres reuses this same compound-aware scale.
- Brake discs sit between the tyres, aligned by axle, with their numeric Celsius
  value and a 100 C stepped cold-to-hot scale matching TinyPedal's behavior.
- Round values before selecting a heatmap band, as TinyPedal does. Cold blue bands
  use light text; the remaining brighter bands use dark text for legibility.
- The panel uses the compact dark surface, neutral outline and 2 px lime accent.
  Puncture and detachment affect the tyre outline without hiding live readings.
- Cache DOM nodes through the static document and skip unchanged text, title and
  CSS-property writes in the 50 Hz renderer.
- Native composite mode has no animation, transition or backdrop filter.

## Verification focus

Test all twelve tread samples independently, direct `0/1/2` zone mapping,
Kelvin conversion, brake values and colors, per-wheel sliding-fraction strips,
puncture/detachment outlines, locale labels, native composite projection and the
`/tiretemps` OBS route.
