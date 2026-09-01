# Current conditions overlay

## Scope and files

- Entry: `conditions.html`
- Renderer/style: `src/conditions.ts`, `src/conditions.css`
- Icon mapping: `src/weather-icons.ts`, icons under `src/assets/lmu-icons/weather/`
- REST source: `src-tauri/src/telemetry/lmu_rest.rs` (forecast humidity fallback
  from `/rest/sessions/weather`)
- OBS route: `/conditions`
- Cadence: 20 Hz Smooth, 12.5 Hz Balanced, about 8 Hz Efficiency; the faster
  delivery keeps the relative wind arrow responsive while weather values remain
  backed by their low-frequency sources.

## Data semantics

- Live temperatures, rain, track wetness and `mTrackGripLevel` (official track
  grip) come from shared memory. Wind follows doX's LMU behavior and uses the
  current official REST forecast node: `WNV_WINDSPEED` is metres per second and
  `WNV_WINDDIRECTION` is the eight-point meteorological compass index. Do not
  convert shared-memory `mWind` into a compass bearing: its components use
  track/world axes rather than a documented north reference.
- The cardinal label remains the absolute meteorological direction. The arrow is
  rotated relative to the selected vehicle from telemetry `mOri`, matching doX:
  it therefore moves as the car turns even while the forecast direction is fixed.
  Keep it as a prominent solid pointer rather than a small text glyph so its
  orientation remains readable at driving distance.
- The condition icon combines live shared-memory `mCloudCoverage` and `mRaining`
  into the game's 0..10 weather scale. The current forecast node's `sky` is used
  only as fallback when live weather values are unavailable.
- Track surface state (`dry`/`damp`/`wet`/`heavy`/`saturated`) is derived in Rust
  from average path wetness. Below 1% remains dry, matching TinyPedal's dry-road
  boundary; the remaining bands start at 1/15/40/70%. Numeric wetness remains
  visible separately.
- Beside the surface state, dry conditions show an estimated rubber percentage
  matching TinyPedal/doX behavior: practice starts at 25%, qualifying/race at
  50%, then the completed laps of every valid scored car increase coverage on a
  2,000-lap median curve. At 1% wetness or above, that position shows the
  official average path wetness instead. This estimate is not official LMU grip.
- `mTrackGripLevel` is converted from LMU's categorical scale to
  25/50/75/90 percent for levels 1/2/3/4; zero and unknown levels remain
  unavailable. Current humidity comes from the humidity of the forecast node
  covering the present moment; it degrades to unavailable when the forecast is
  missing or stale. The grip scale remains provisional until confirmed by a live
  capture.

## Presentation

- Keep the panel as two compact rows. The header shows the localized live weather
  condition, surface state with rubber/wetness percentage, official grip and rain
  (rain is omitted at zero).
  The data row shows air temperature, track temperature, wind (speed, absolute
  compass bearing and vehicle-relative arrow), humidity and numeric wetness.
- Every header value and data column is independently optional: condition/icon,
  surface state, official grip, rain, air temperature, track temperature, wind,
  humidity and wetness. The remaining data columns expand to use the available
  width; an empty header or data row is removed. These preferences also apply to
  the OBS route and configuration import/export.
- Cells share the restrained visual language and a thin divider between them;
  the state is color-coded by surface condition.
- Keep equal compact tracks except for Wind, which receives the minimum extra
  width needed for the arrow and speed when text size increases.
- Use a 390 × 90 px base surface with narrow status and grid gutters; text-size
  expansion remains responsible for adding only the space needed by larger text.
- Keep the shared 2 px translucent lime accent along the shell's left edge.

## Verification focus

Test REST wind units/direction, vehicle-relative arrow rotation, shared-memory
grip availability, state thresholds, humidity fallback and missing/stale forecast
with the frontend build.

## Localization

Labels, states, document title and accessibility text use the bundled locale;
values remain format-only. Its OBS route follows the shared saved locale or a
page-local `?lang=` override.
