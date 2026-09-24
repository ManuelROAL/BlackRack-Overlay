# Weather Forecast overlay

## Scope and files

- Entry: `forecast.html`
- Renderer/style: `src/forecast.ts`, `src/forecast.css`
- Icon mapping: `src/weather-icons.ts`, icons under `src/assets/lmu-icons/weather/`
- REST source: `src-tauri/src/telemetry/sim/lmu/rest.rs` (weather thread on
  `/rest/sessions/weather`, selecting the active session)
- OBS route: `/forecast`
- Cadence: 2 Hz (500 ms); REST forecast polled at 1 Hz and accepted while fresh

## Data semantics

- The forecast strip comes from LMU's own weather session forecast
  (`/rest/sessions/weather`, selecting PRACTICE, QUALIFY or RACE by the
  shared-memory session type). It contains five fixed slots: START, 25%, 50%,
  75% and FINISH.
- Rust serializes one node per slot with `sky`, `sky_label` (the REST
  `stringValue`), `temperature_c`, `rain_chance_percent`, `humidity_percent`
  and the rounded `minutes_from_now` for future nodes. Percentages are clamped
  to 0..100.
- Rust computes the current slot and next forecast slot from session progress.
  The panel always shows the live shared-memory condition as `NOW`, followed by
  future forecast nodes labelled `+XM`; already passed nodes are hidden. With
  no reliable session length, future nodes remain visible and their relative
  time is shown as unavailable.
- `weather_forecast.available` becomes false when the REST value is missing or
  stale (accepted up to five seconds), or before the first refresh of a new
  session.

## Presentation

- Keep the panel as a horizontal strip with a slim heading and one column per
  visible slot: relative time, condition icon, air temperature and rain
  probability. The first column is live and labelled `NOW`.
- Use a 366 × 112 px base surface with all five forecast slots visible.
- Column count is dynamic (5 down to 1 future nodes plus NOW); the overlay reports its design width
  so the host follows the content while preserving the user's visual scale.
- Centre the strip so a lone `NOW` column sits in the middle of the panel's
  minimum width instead of hugging its left edge.
- Expand each visible forecast column with the text-size surface so translated
  labels remain isolated from adjacent slots.
- Forecast `sky` maps directly to the game's condition SVG (0..10) via
  `src/weather-icons.ts`. The live `NOW` icon combines shared-memory cloud
  coverage with rain intensity using LMU's 0..10 condition scale.
- Keep the shared 2 px translucent lime accent along the shell's left edge.

## Verification focus

Forecast and current air temperatures follow the global Celsius/Fahrenheit
display preference; the REST values remain Celsius.

Test forecast availability transitions, session switch (PRACTICE/QUALIFY/RACE),
missing/stale REST, sky/icon selection, NOW labelling and the dynamic-width
reporting with the frontend build.

## Localization

Static labels, slot names, `NOW`, document title and accessibility text use the
bundled locale; temperatures and percentages remain format-only. Its OBS route
follows the shared saved locale or a page-local `?lang=` override.
