# Session Info overlay

## Scope and files

- Entry: `sessioninfo.html`
- Renderer/style and settings: `src/sessioninfo.ts`, `src/sessioninfo.css`,
  `src/sessioninfo-settings.ts`
- Settings are stored under `blackrack-overlay.sessioninfo.v1` and synchronized
  through `sessioninfo://settings`.
- OBS route: `/sessioninfo`
- Native cadence: the secondary overlay cadence (25 Hz Smooth, 12.5 Hz Balanced,
  about 8 Hz Efficiency). OBS receives the shared browser-source frame cadence.

## Fields and sources

Each field is independently optional: session type, clock, track name, time
remaining, laps remaining, current lap and estimated total laps, track and air
temperature, weather and track limits. These use existing `TelemetryFrame`
values. Session and lap values come from the active telemetry source. The clock
uses game time by default; `Use System Clock` switches to local time and only
runs a one-second update while the clock field is visible. Temperatures and
current weather are shown only when the source reports them available. Remaining
laps are rounded up from the telemetry estimate and marked with `~`; unknown
values are rendered as unavailable, not as zero. Track-limit values match
Standings: current points over the points required for a penalty, with the same
warning and critical thresholds.

## Layout

The native seed geometry is 520 × 54 px. The line expands to fit the selected
fields; settings also allow a column layout, and the renderer reports its actual
dimensions to the composite host. Keep each field optional in both layouts, and
respect global temperature units, text size and background transparency
preferences. The same saved field and layout settings apply to the native overlay
and OBS route.

## Cadence and demand

The native renderer updates only when the overlay is active, at the configured
secondary cadence. These values are session-level or slowly changing, so they do
not need the fast driving cadence. When active, the overlay requests the existing
REST weather snapshot for temperature and condition fields, but does not request
standings or additional model construction. The OBS browser source uses the
established shared frame delivery cadence and route-specific client demand.
