# Detailed tyre temperatures overlay

## Scope and files

- Entry: `tiretemps.html`
- Renderer/style: `src/tiretemps.ts`, `src/tiretemps.css`
- Shared-memory conversion: `src-tauri/src/telemetry/lmu_bridge.cpp`, `src-tauri/src/telemetry/lmu.rs`
- OBS route: `/tiretemps`
- Cadence: 50 Hz base telemetry

This panel complements Damage + Tyres with the three live tread temperatures for
every wheel and the four brake-disc temperatures. Damage and wear remain in their
own overlays.

## Sources and semantics

- Wheel order is front-left, front-right, rear-left, rear-right.
- The SDK exposes `mTemperature[3]` as physical left/centre/right samples, not
  semantic inside/centre/outside samples. The C++ bridge normalizes them to
  inside/centre/outside: left-side wheels use indices `2/1/0`, right-side wheels
  use `0/1/2`.
- All tyre samples and `mBrakeTemp` are converted from Kelvin to Celsius.
- The white strip on each tyre is a static representation of the asphalt contact
  patch. It is not a fourth temperature sample and does not change color.

## Presentation and invariants

- Each tyre keeps its inside band nearest the car centre and its outside band
  nearest the panel edge. Every band shows its own rounded Celsius value and uses
  the shared cold/working/hot color progression.
- Brake discs sit between the tyres, aligned by axle, with their numeric Celsius
  value and the existing brake-temperature color scale.
- The panel uses the compact dark surface, neutral outline and 2 px lime accent.
  Puncture and detachment affect the tyre outline without hiding live readings.
- Cache DOM nodes through the static document and skip unchanged text, title and
  CSS-property writes in the 50 Hz renderer.
- Native composite mode has no animation, transition or backdrop filter.

## Verification focus

Test all twelve tread samples independently, left/right semantic normalization,
Kelvin conversion, brake values and colors, puncture/detachment outlines, locale
labels, native composite projection and the `/tiretemps` OBS route.
