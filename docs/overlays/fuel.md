# Fuel and virtual-energy overlay

## Scope and files

- Entry: `fuel.html`
- Renderer/style: `src/fuel.ts`, `src/fuel.css`
- Rust strategy: `src-tauri/src/telemetry/fuel_strategy.rs`
- Persistent learning: `src-tauri/src/telemetry/consumption_profile.rs`
- OBS route: `/fuel`
- Cadence: 50 Hz base telemetry

All stop, target-consumption, pit-window, refill and parallel-resource strategy
math belongs in Rust. `fuel.ts` formats serialized plans and may track only the
presentation-specific stint delta.

## Resource and strategy semantics

- Hypercar and LMGT3 use virtual energy when reported; LMP2, LMP3 and other
  classes use fuel litres.
- Regulated classes calculate virtual energy and fuel in parallel. The active race
  plan uses whichever resource requires more stops.
- Consumption belongs to the player. Only total race-lap projection follows the
  leader.
- Use exact fractional-lap projections. Never add an arbitrary safety lap or
  subtract a configurable reserve.
- Respect an initial resource below 100% and track pit refills so lap consumption
  remains correct.
- Autonomy is remaining resource divided by the selected reference and may be
  fractional.

## Consumption references and learning

- Show estimated, clean average, qualifying and last-lap scenarios. The qualifying
  reference is the consumption associated with the fastest valid official
  qualifying lap.
- Compare every scenario's fractional autonomy with qualifying in Rust. The UI
  presents that delta as potential laps gained or lost at the current resource
  level; it is not a claim that a full lap has already been banked.
- Average and last reset for a new session. Qualifying survives qualifying phases
  into the race and clears for a new practice/event.
- Formation, invalid, neutralized and pit laps affect real balance but do not
  contaminate clean average consumption.
- Learn pit-in and pit-out use separately per car/circuit and include them in
  multi-stop strategy. Do not learn garage exit as a race pit-out lap.
- Automatic target consumption follows TinyPedal semantics and cannot exceed the
  qualifying reference. Reaching the cap means full-power running until the stop.
- Only propose removing a stop when the required consumption is supported by at
  least three recent clean laps. Use the lower quartile of up to twelve clean
  laps so one anomalously low lap cannot define an unattainable target. Without
  enough evidence, or when the required target is below that bound, retain the
  current stop count and request no additional saving. A parallel fuel/energy
  resource may also impose a stop-count floor that the target cannot cross.

## Race distance

- For timed races, project leader crossings and then player crossings. For
  fixed-lap races use the official target. Preserve physical progress across the
  finish-line transition.
- Keep the Standings visual remaining-lap estimate separate from this leader-aware
  strategy count.
- The serialized frame contains the active plan, parallel fuel plan where needed,
  and each reference scenario; the frontend must not recalculate them.

## Presentation

- Present current resource, race projection, pit window/load and all scenarios as
  a compact endurance strategy tool.
- Label scenario-wide replenishment as `TOTAL +`; it is the sum still required
  over the remaining race, not necessarily the next pit load. The summary's
  `CARGA` value remains the next-stop load.
- When exactly one stop remains and the qualifying scenario can also finish with
  one stop, use the larger of the active and qualifying next-fill calculations
  and label it `CARGA Q`. This follows the conservative final-stint behavior of
  planning for full-power running without adding an arbitrary reserve.
- Replace the formerly exact-but-usually-zero scenario `FINAL` column with
  `Δ QUALY`, since there is no authoritative selected pit-menu load from which to
  predict a meaningful user-specific finish reserve.
- Hybrid cars show energy scenarios plus a compact fuel card while the global pit
  plan accounts for both resources.
- The PIT indicator is neutral above three laps of autonomy and changes as the
  stop approaches. Show a distinct full-power state when qualifying caps target
  consumption. Label the target `MANTÉN` when clean-lap evidence rejects the
  attempted stop reduction and the current stop count is retained.

## Verification focus

Test session transitions, qualifying carryover, fractional progress, sub-100%
starts, refills, formation/neutralization/pit exclusion, multi-stop pit profiles
and the limiting-resource choice. Run Rust tests and the frontend build.
