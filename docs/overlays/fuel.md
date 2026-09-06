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
Consumption learning remains shared, while the multi-scenario strategy model is
calculated only for a visible native panel or connected `/fuel` route.

## Resource and strategy semantics

- Hypercar and LMGT3 use virtual energy when reported; LMP2, LMP3 and other
  classes use fuel litres.
- Regulated classes calculate virtual energy and fuel in parallel. The active race
  plan uses whichever resource requires more stops.
- Consumption and base race distance belong to the player. Leader/last-stop lap
  adjustments are informational and never modify the resource requirement.
- Use exact fractional-lap projections. Never add an arbitrary safety lap or
  subtract a reserve from the race-wide requirement. The stint-target widget's
  separate 0.2-unit end-of-stint reserve follows TinyPedal semantics and does not
  change refills or total required fuel/energy.
- Respect an initial resource below 100% and track pit refills so lap consumption
  remains correct.
- Autonomy is remaining resource divided by the selected reference and may be
  fractional.
- The summary and PIT range warning read `resource_autonomy.range_laps`, shared
  with Dashboard; the auxiliary fuel card reads `resource_autonomy.fuel_laps`.
  Rust selects the first finite positive consumption from projected, clean average,
  last lap, stored profile reference and qualifying. These lightweight ranges are
  built independently of strategy demand or remaining race distance. With energy
  active, the summary uses the smaller fuel/energy range and requires both
  references. Missing values are null; a known empty resource is zero. Scenario
  rows retain their separate consumption references and resource-specific ranges.

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
- Invalid laps follow the shared latched `mLapInvalidated`, negative official
  time confirmation and a reconstructed duration when LMU returns the missing
  official sentinel; `mCountLapFlag` is not used for consumption validity.
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

- For timed races use `ceil(remaining_seconds / player_pace + lap_progress) -
  lap_progress` as the shared base remaining distance. For fixed-lap races use
  the official target minus completed laps and current progress; mixed sessions
  select the first finish criterion. Preserve progress across the finish line.
- Use the player's six-sample clean-lap EMA for projected crossings, matching
  TinyPedal rather than relying on LMU's noisier instantaneous estimated lap.
  Standings and Timing derive the player's total from this same base distance.
- Timed-race leader and final-stop effects are published separately as
  `session_extra_laps_estimated` for those headers. They never reduce or increase
  fuel/energy requirements. The final-stop hint uses the active resource's
  fractional remaining stops (strictly between 0.2 and 1.2), official concurrent
  service time and learned moving pitlane time; unavailable time references omit
  that component. The leader component uses the overall leader's finish time.
- These shared estimates do not depend on whether the Fuel panel is enabled.
- The serialized frame contains the active plan, parallel fuel plan where needed,
  and each reference scenario; the frontend must not recalculate them.

## Presentation

- The fuel settings disclosure offers independent visibility switches for current
  resource, autonomy, pit window, post-pit range, PIT warning, level bar, stint
  targets, each scenario row, each numeric scenario column, the auxiliary fuel
  card and its ratios. All are enabled by default, including when loading older
  settings or importing a configuration without visibility preferences.
- Hidden summary tracks and scenario columns collapse; the table disappears when
  it has no selected rows or numeric columns. The design height follows visible
  sections and rows, including the absence of the auxiliary card in fuel-only
  mode, while preserving the user's visual scale. With everything hidden a
  minimal 32 px surface remains recoverable in edit mode.
- Visibility uses the existing fuel settings key/event and is included in
  profiles, validated configuration export/import, scoped reset and OBS mirroring.
- Present current resource, autonomy, pit window/load and the clean-average,
  qualifying and last-lap scenarios as a compact endurance strategy tool.
- Use a `292 x 198` design surface. Migrate the former default `560 x 230` and
  intermediate `470 x 188`, `390 x 188`, `356 x 202` and `356 x 188` placements
  plus the former `252 x 188`, `252 x 216` and `292 x 216` defaults to the current
  size so deliberately resized user layouts keep their chosen scale.
- Let the scenario table define the panel width: its label column and three
  numeric columns fill the complete usable surface without empty side gutters.
- When text size expands the design surface, distribute the added width and
  only the required row height through the summary, stint-target and scenario tracks; do
  not scale the complete height proportionally or leave added width unused.
- Preserve the established track proportions and a tight 2 px horizontal gap
  between stint-target and scenario columns; distribute only text-size expansion across
  those tracks.
- Center each scenario value horizontally under its numeric column heading while
  keeping the scenario names left-aligned.
- Color the average scenario cyan as the prominent baseline, qualifying as an
  aggressive high-consumption reference and the last lap as the freshest live
  sample.
- Keep the shared 2 px translucent lime accent along the shell's left edge.
- In the native composite, resource bars and critical-state emphasis update
  directly at the bounded overlay cadence; CSS easing/pulsing remains available
  only to standalone and OBS pages so it cannot force monitor-rate presentation.
- Keep the summary focused on current resource, lap autonomy, pit window and PIT
  status. Do not show the redundant projected remaining/total race laps or
  autonomy minutes. Reserve the summary's first column for an enlarged current
  value and represent the active resource with its energy or fuel icon only in
  the scenario header instead of spelling out `NRG` or `FUEL`.
  Use the lime energy bar for regulated cars and the orange fuel bar for cars
  without virtual energy.
- Replace the former race-plan strip with three current-stint targets for reaching
  the next integer runnable range plus one, two or three laps. Following
  TinyPedal, Rust starts from current resource plus the amount already consumed
  this lap, subtracts a 0.2-unit reserve, rounds autonomy to one decimal before
  flooring, and divides the remaining resource across each target lap count.
  Once a complete pit passage has been learned and more stops remain, bias the
  available resource from the finish line towards the learned pit-entry position.
- When qualifying pace/consumption and LMU's current pit-time estimate are
  available, estimate the net remaining-race time for each stint target. Compare
  stops avoided and the shorter total resource service with the linearly
  estimated lap-time cost of the extra saving. Scale LMU's active fuel/energy
  service time by the average load per remaining stop, preserve the official
  total's fixed residual, and preserve the longest parallel service as the
  minimum stop duration. A requested driver change sets
  a 26-second parallel-service floor; it is never added on top of a longer
  refuel. Add the median moving pitlane time learned by Track Map once for every
  stop the target actually removes. If a target removes a stop but no complete
  pit passage has been learned yet, keep its time and color neutral rather than
  underestimating the gain;
  color a positive result green, a negative result red and a near-zero result
  amber. Keep the target neutral and omit the time when those references are not
  available; never invent a pace cost in the frontend.
- Keep the target's time calculation as color semantics only; the compact caption
  shows only the extra range (`+1`, `+2` or `+3`) instead of diagnostic deltas or
  repeated seconds.
- Parse the absolute fuel or virtual-energy load selected in LMU's official pit
  menu and show its post-pit autonomy using the projected valid consumption.
  Keep it unavailable when the REST value or consumption reference is unavailable.
- Post-pit range uses the same Rust range model and planned references as current
  autonomy. With virtual energy active, both configured absolute loads are required
  and the limiting resource determines laps and minutes. The selected active-resource
  load remains separate from this combined range.
- Omit diagnostic context (confidence, pit-cycle delta, qualifying gain and
  pit-service estimate) from the driving overlay; those values are not needed for
  the immediate stop/save decision.
- In energy mode, give the auxiliary fuel resource its own card below the energy
  scenarios. Emphasize current litres without a redundant resource label, show
  fractional autonomy, capacity and a proportional level bar, then show only the
  official assigned fuel ratio, the
  clean-lap average ratio and the last-lap ratio in one readable detail row.
- Label scenario-wide replenishment as `TOTAL +`; show the total resource required
  by adding the current level to the calculated refill, without subtracting any
  calculated surplus. It is not necessarily the next pit load. The summary's
  `CARGA` value remains the next-stop load.
- Let the user switch the scenario value column between `TOTAL +` and `REFUEL`.
  `REFUEL` shows the calculated load for each scenario's next stop. When LMU has
  an active player pit request, calculate that load for the end of the current
  requested lap instead of the latest lap in the normal pit window. Persist and
  mirror this choice to OBS; `TOTAL +` remains the default.
- When exactly one stop remains and the qualifying scenario can also finish with
  one stop, use the larger of the active and qualifying next-fill calculations
  and label it `CARGA Q`. This follows the conservative final-stint behavior of
  planning for full-power running without adding an arbitrary reserve.
- Keep `TOTAL +` in the scenario table, but omit `Δ QUALY` and the redundant
  `ESTIMADO` row. The visible rows are `PROMEDIO`, `QUALY` and `ÚLTIMA`.
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
- Grow the PIT indicator's box with the text design surface so its label remains
  centered and unclipped throughout the supported 75–200% text range.

## Verification focus

Test session transitions, qualifying carryover, fractional progress, sub-100%
starts, refills, formation/neutralization/pit exclusion, multi-stop pit profiles
and the limiting-resource choice. Run Rust tests and the frontend build.
Verify matching Dashboard/Fuel range with Fuel enabled and disabled, zero versus
unknown consumption, either resource limiting and missing parallel post-pit loads.

## Localization

Static and dynamic strategy labels use the bundled locale. Decimal formatting and
lap abbreviations follow it, while strategy calculations and resource IDs remain
unchanged and outside the renderer. Its OBS route follows the shared saved locale
or a page-local `?lang=` override.
