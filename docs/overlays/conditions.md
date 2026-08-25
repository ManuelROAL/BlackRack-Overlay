# Current conditions overlay

## Scope and files

- Entry: `conditions.html`
- Renderer/style: `src/conditions.ts`, `src/conditions.css`
- Icon mapping: `src/weather-icons.ts`, icons under `src/assets/lmu-icons/weather/`
- REST source: `src-tauri/src/telemetry/lmu_rest.rs` (forecast humidity fallback
  from `/rest/sessions/weather`)
- OBS route: `/conditions`
- Cadence: 2 Hz (500 ms)

## Data semantics

- Live values come from shared memory and are authoritative: ambient and track
  temperature, rain percentage, track wetness, `mTrackGripLevel` (official track
  grip) and `mWind` (speed and bearing computed in Rust).
- The condition icon combines live shared-memory `mCloudCoverage` and `mRaining`
  into the game's 0..10 weather scale. The current forecast node's `sky` is used
  only as fallback when live weather values are unavailable.
- Track surface state (`dry`/`damp`/`wet`/`saturated`) is derived in Rust from
  the track wetness thresholds 15/40/70; the numeric wetness percentage is shown
  separately.
- `mTrackGripLevel` is converted from LMU's categorical scale to
  25/50/75/90 percent for levels 1/2/3/4; zero and unknown levels remain
  unavailable. Current humidity comes from the humidity of the forecast node
  covering the present moment; it degrades to unavailable when the forecast is
  missing or stale. Wind and grip scales are provisional until confirmed by a
  live capture.

## Presentation

- Keep the panel as one compact horizontal row: condition icon plus surface
  state, then cells for air temperature, track temperature, wind (speed and
  direction arrow), humidity, rain, grip and wetness.
- Cells share the restrained visual language and a thin divider between them;
  the state is color-coded by surface condition.
- Keep the shared 2 px translucent lime accent along the shell's left edge.

## Verification focus

Test shared-memory wind/grip availability, state thresholds, humidity fallback
and missing/stale forecast with the frontend build.

## Localization

Labels, states, document title and accessibility text use the bundled locale;
values remain format-only. Its OBS route follows the shared saved locale or a
page-local `?lang=` override.
