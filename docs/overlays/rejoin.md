# Rejoin overlay

## Scope and files

- Entry: `rejoin.html`
- Renderer/style: `src/rejoin.ts`, `src/rejoin.css`
- Backend model: `src-tauri/src/telemetry/lmu.rs`
- OBS route: `/rejoin`
- Cadence: 20 Hz while active; 250 ms while inactive

## Arming and candidate semantics

- Arm while the player is in pitlane, below 8 m/s, or has all four wheels on
  grass, dirt or gravel.
- Keep the state armed for ten seconds after pit exit or recovery.
- Show only for an active on-track car physically behind within a 15-second
  relative time gap.
- Prefer the earliest positive arrival time among closing cars. If none is
  closing, use the smallest physical time gap.
- Instantaneous closing speed supplies arrival time. A non-closing car is always
  safe; otherwise distance and arrival time choose safe, caution or danger.

## Presentation

- Display distance, arrival time, position and category.
- Preserve distinct safe, caution and danger colors in the compact alert surface.

## Verification focus

Test pit entry/exit, slow/off-surface arming, ten-second hold, 15-second boundary,
closing versus non-closing cars, candidate preference and automatic visibility.

## Localization

Warning reasons, safety states, document title and accessibility text use the
bundled locale. Distances and arrival times use its ordinary number formatting.
