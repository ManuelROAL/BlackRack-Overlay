# Pit-stop estimate overlay

## Scope and files

- Entry: `pitstop.html`
- Renderer/style: `src/pitstop.ts`, `src/pitstop.css`
- REST source: `src-tauri/src/telemetry/lmu_rest.rs`
- OBS route: `/pitstop`
- Cadence: 20 Hz; REST estimate polled at 1 Hz and accepted while fresh

## Data semantics

- `/rest/strategy/pitstop-estimate` supplies the prediction.
- Display LMU's `total` directly as authoritative. Fuel/energy, tyres, repairs,
  driver swap and penalties may overlap, so never sum service rows.
- Repairs combine damage, brakes and brake ducts. Penalties remain a separate
  optional row.
- Use virtual energy for the resource row when the player's active strategy uses
  it, otherwise fuel.
- All values become unavailable when the estimate is missing or stale.

## Presentation

- Keep the panel compact and show repairs, resource, tyres, driver swap and total.
- Add the penalty row only when non-zero.
- Keep the restrained lime edge accent on the left as the overlay's visual mark,
  matching the Trailing + Pedal treatment.
- Follow the shared visual hierarchy without implying that displayed rows add up
  to the total.

## Verification focus

Test concurrent services, zero/non-zero penalty, fuel/energy label switching,
missing/stale REST and direct use of the official total.

## Localization

Service labels, the resource switch, document title and accessibility text use
the bundled locale; timing values and estimate semantics remain unchanged.
