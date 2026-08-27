# Fuel and virtual-energy overlay

## Scope and files

- Entry: `fuel.html`
- Renderer/style: `src/fuel.ts`, `src/fuel.css`
- Rust strategy: `src-tauri/src/telemetry/fuel_strategy.rs`
- Persistent learning: `src-tauri/src/telemetry/consumption_profile.rs`
- OBS route: `/fuel`
- Cadence: 50 Hz base telemetry

All stop, target-consumption, pit-window, refill and parallel-resource strategy
math belongs in Rust. `fuel.ts` only formats serialized plans.

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
- Invalid laps follow the shared latched `mLapInvalidated` plus negative official
  time confirmation; `mCountLapFlag` is not used for consumption validity.
- Learn pit-in and pit-out use separately per car/circuit and include them in
  multi-stop strategy. Do not learn garage exit as a race pit-out lap.
- Automatic target consumption follows TinyPedal semantics and cannot exceed the
  qualifying reference. Reaching the cap means full-power running until the stop.
- Show the theoretical one-stop reduction, its target consumption and the
  required saving up to a 15% saving threshold, so the driver has a useful
  reference even before achieving that pace. Above that threshold the target is
  too disproportionate to present as actionable guidance. Do not gate this
  guidance on previously observed clean-lap consumption. A parallel fuel/energy
  resource may still impose a stop-count floor that the target cannot cross, and
  a pit stop already in progress cannot be removed.

## Race distance

- For timed races, project leader crossings and then player crossings. For
  fixed-lap races use the official target. Preserve physical progress across the
  finish-line transition.
- Keep the Standings visual remaining-lap estimate separate from this leader-aware
  strategy count.
- The serialized frame contains the active plan, parallel fuel plan where needed,
  and each reference scenario; the frontend must not recalculate them.

## Presentation

- Present current resource, autonomy, pit window/load and the clean-average,
  qualifying and last-lap scenarios as a compact endurance strategy tool.
- Use a `292 x 198` design surface. Migrate the former default `560 x 230` and
  intermediate `470 x 188`, `390 x 188`, `356 x 202` and `356 x 188` placements
  plus the former `252 x 188`, `252 x 216` and `292 x 216` defaults to the current
  size so deliberately resized user layouts keep their chosen scale.
- Let the scenario table define the panel width: its label column and four
  numeric columns fill the complete usable surface without empty side gutters.
- When text size expands the design surface, distribute the added width and
  only the required row height through the summary, plan and scenario tracks; do
  not scale the complete height proportionally or leave added width unused.
- Preserve the established track proportions and a tight 2 px horizontal gap
  between plan and scenario columns; distribute only text-size expansion across
  those tracks.
- Center each scenario value horizontally under its numeric column heading while
  keeping the scenario names left-aligned.
- Color the average scenario cyan as the prominent baseline, qualifying as an
  aggressive high-consumption reference and the last lap as the freshest live
  sample.
- Keep the shared 2 px translucent lime accent along the shell's left edge.
- Keep the summary focused on current resource, lap autonomy, pit window and PIT
  status. Do not show the redundant projected remaining/total race laps or
  autonomy minutes. Reserve the summary's first column for an enlarged current
  value and represent the active resource with its energy or fuel icon only in
  the scenario header instead of spelling out `NRG` or `FUEL`.
  Use the lime energy bar for regulated cars and the orange fuel bar for cars
  without virtual energy.
- Keep the race plan focused on stops, target consumption, required saving and
  next-stop load. Do not show the presentation-only live stint delta.
- Omit diagnostic context (confidence, pit-cycle delta, qualifying gain and
  pit-service estimate) from the driving overlay; those values are not needed for
  the immediate stop/save decision.
- In energy mode, give the auxiliary fuel resource its own card below the energy
  scenarios. Emphasize current litres without a redundant resource label, show
  fractional autonomy, capacity and a proportional level bar, then show only the
  official assigned fuel ratio, the
  clean-lap average ratio and the last-lap ratio in one readable detail row.
- Label scenario-wide replenishment as `TOTAL +`; it is the sum still required
  over the remaining race, not necessarily the next pit load. The summary's
  `CARGA` value remains the next-stop load.
- When exactly one stop remains and the qualifying scenario can also finish with
  one stop, use the larger of the active and qualifying next-fill calculations
  and label it `CARGA Q`. This follows the conservative final-stint behavior of
  planning for full-power running without adding an arbitrary reserve.
- Keep `TOTAL +` in the scenario table, but omit `Δ QUALY` and the redundant
  `ESTIMADO` row. The visible rows are `PROMEDIO`, `QUALY` and `ÚLTIMA`. Each row
  also shows the projected active-resource balance at the end of the current
  planned stint, or at the race finish when it comes first. Positive values are
  surplus and negative values are a deficit; use percent for NRG and litres for
  fuel-only cars.
- Hybrid cars show energy scenarios plus a compact fuel card while the global pit
  plan accounts for both resources.
- Cars reporting both virtual energy and fuel show the fuel ratio selected in
  LMU's official pit menu plus clean-average and last-lap ratios. Each observed
  ratio is fuel consumption divided by virtual-energy consumption; keep it
  unavailable until both matching consumption values exist, and do not show the
  ratios in fuel-only mode.
- The PIT indicator is neutral above three laps of autonomy and changes as the
  stop approaches. Show a distinct full-power state when qualifying caps target
  consumption. Label the target `MANTÉN` when an active pit stop, the parallel
  resource or an over-15% saving requirement prevents reducing the current stop
  count.

## Verification focus

Test session transitions, qualifying carryover, fractional progress, sub-100%
starts, refills, formation/neutralization/pit exclusion, multi-stop pit profiles
and the limiting-resource choice. Run Rust tests and the frontend build.

## Localization

Static and dynamic strategy labels use the bundled locale. Decimal formatting and
lap abbreviations follow it, while strategy calculations and resource IDs remain
unchanged and outside the renderer. Its OBS route follows the shared saved locale
or a page-local `?lang=` override.
