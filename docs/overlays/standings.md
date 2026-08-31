# Standings overlay

## Scope and files

- Entry: `standings.html`
- Renderer/style/settings: `src/standings.ts`, `src/standings.css`,
  `src/standings-settings.ts`
- Rust view model: `src-tauri/src/telemetry/standings_models.rs`
- Timing/history/enrichment: `src-tauri/src/telemetry/lmu.rs`,
  `driver_ranks.rs`, `event_split.rs`, `lmu_rest.rs`
- OBS route: `/standings`
- Cadence: enriched roster and renderer at 10 Hz in Smooth, 6.25 Hz in Balanced
  and approximately 4.2 Hz in Efficiency

Rust owns class grouping, class order/counts, SOF and visible-row selection. Its
view model is built only for a visible native panel or connected `/standings` route.
The renderer resolves prepared vehicle IDs and owns presentation only; it must not
duplicate selection or timing semantics.

## Rows, classes and columns

- The table is multiclass and has no scrollbar. Player-class rows are configurable
  from 3 to 30 (default 10); other classes from 1 to 15 (default 3) and may be
  hidden.
- For the player's class, select the first three plus the closest cars around the
  player until the limit is reached. Other classes show their leading rows.
- Order classes by performance: Hypercar/GTP, LMP2, LMP3, LMGT3, then other.
- Preserve LMU-compatible category colors: Hypercar red, LMP2 blue, LMP3 purple
  and LMGT3 green.
- Normalize LMU's abbreviated `Hyper` class label to the `HYPERCAR` heading and
  Hypercar red tone, consistently with longer Hypercar names and GTP aliases.
- Columns are position, assigned number, manufacturer, badge, driver, DR/SR,
  GAP, INT, best, last, AVG 5, NRG, damage, track-limit steps, tyre and
  signals. Position and driver are mandatory; signals stays fixed at the far
  right. Other columns are independently visible and reorderable. Pit data is an
  independently visible status integrated into the driver cell rather than a
  separate reorderable column.
- Reordering preserves the category-header boundary: identity columns without a
  label remain inside the colored category heading, while labeled data columns
  remain outside it. Signals stays fixed at the far right.
- Use the session-assigned number. Driver-name presentation is independently
  configurable as full name, initial plus surname, name plus surname initial,
  surname only, name only or surname plus name initial. Prefer bundled
  manufacturer, nationality, badge and tyre assets.
- A uniform tyre set uses the bundled sidewall-style compound SVG with a fixed
  color and letter glyph; mixed sets use four compact colored circles.

## Timing and history semantics

- GAP is relative to the class leader; INT is relative to the preceding car in
  class. Derive them from continuous shared-memory lap progress.
- Reconstruct a temporary same-lap zero gap from continuous progress. Show a lap
  difference only after a complete physical lap separates the cars.
- The category leader displays its current lap once in GAP and once in INT.
- Lap times use three decimals; GAP/INT use one decimal. Personal best is green
  and session fastest is purple.
- A completed invalid lap remains visible in gray, taking visual precedence over
  personal-best or session-fastest coloring. Latch the per-vehicle telemetry
  `mLapInvalidated` signal across the complete lap. Also treat a negative
  `mLastLapTime` between -900 and -20 seconds
  as official invalid confirmation and display its absolute duration. Ignore
  residual invalidation signals for two seconds after a stable lap
  boundary. If `mLastLapTime` is unavailable,
  reconstruct it from consecutive `mLapStartET` values only after the new lap has
  been active for more than one second and mark that reconstructed result invalid.
  Discard stable partial resets below 50%
  of official best or estimated pace only for valid laps; retain every invalid
  reconstructed duration between 20 and 900 seconds so LAST never becomes empty.
- AVG 5 keeps the latest five completed plausible times, rejects values below the
  official valid best and excludes laps slower than 120% of the best plausible
  recent lap. Clean validity remains mandatory for consumption learning.
- `OUT` replaces last-lap time for the complete pit-out lap and retains its orange
  state rather than best-lap coloring.
- The TinyPedal-style remaining-lap display estimate is separate from the
  leader-aware finish-line crossing count used by fuel strategy.
- Estimated total laps in the header follow Dox's multiclass projection: the
  overall leader determines when the checkered flag begins, while the player's
  class leader supplies the completed laps, lap phase and pace used to project
  that category's maximum lap count.

## Identity, roster and enrichment

- Shared-memory `mID` and REST `slotID` are different namespaces. Match REST
  standings/history by normalized driver identity; accept a numeric fallback only
  when identity also agrees.
- Sample standings history at low frequency only for late-start initial roster
  size and previous valid AVG 5 samples. Its `totalLaps = 0` order is not the race
  grid; derive class starting position from REST `qualification` only when every
  current car in that class has a positive value, converting the complete overall
  order to class positions. Otherwise use the complete shared-memory starting
  order for that class; never mix partial REST and shared-memory positions. Never
  invent a duration for history `lapTime = -1`.
- RaceControl supplies DR, progress, ELO when available, SR, nationality and badge.
  In qualifying/race, complete the one-per-roster refresh only after a successful
  non-empty result; failed, empty and partial results remain retryable. A driver
  swap extends that roster and must trigger a profile request for the new active
  driver without discarding the profiles already cached for the team.
- In registered events, the one-time `event/my-split` roster supplements missing
  or `XX` nationality and missing badges without replacing valid `/players` data.
- The DR estimate is same-class and event-parameter aware. Compare drivers on
  the three-times internal rating scale, then normalize the visible gain with
  `multiplier * K / (2 * rated opponents)`.
- Freeze each driver's latest pre-race visual score for the race estimate. A
  driver first discovered during the race is frozen when their rank becomes
  available, so asynchronous profile refreshes cannot rewrite earlier inputs.
- Its dedicated diagnostic switch writes the player's current DR when available
  or changed during practice and qualifying, then the initial player estimate and
  each meaningful change during a race, to a unique per-run file under
  `dr-estimate-logs/`.
  Practice and qualifying samples contain no position or calculation fields;
  all resolved player samples retain raw continuous ELO to diagnose rank-boundary
  resets. Unchanged samples are omitted and floating-point values are compared
  after rounding to 0.001. Exactly derivable duplicate values are not serialized.
  That JSONL contains only the estimate inputs, coverage, calculation and event
  settings; it remains separate from general analysis telemetry and keeps the
  10 Hz standings model active while enabled. It accumulates all sessions and
  enable/disable cycles in one app run; later launches create a distinct file and
  retain the previous logs.
- Version 2 samples identify the app and formula, event, split and race sequence,
  and include each anonymous rated opponent's positions, score, expected result
  and head-to-head outcomes. At the checkered flag, write the final prediction;
  after a fresh RaceControl profile refresh in a later session, write a settlement
  with actual gain, prediction error and source. Prefer continuous raw ELO for
  settlement and fall back to the continuous visual score only when raw ELO is
  unavailable or masked as zero.
- During the live race, DR head-to-head results follow the responsive class order
  from shared memory. After the checkered flag, prefer LMU REST `position` only
  when `serverScored` is true and every car in that class has a positive scored
  position; otherwise retain the complete shared-memory order rather than mixing
  sources. This lets post-race penalties and classification changes correct the
  final estimate without adding REST lag to the live race. Latch each complete
  scored class order until the session changes, while accepting later corrections.
- SOF uses resolved continuous DR and reports partial coverage. Do not calculate or
  display SOF in practice.

## Header and states

- Keep complete header visibility separate from each header datum. The header may
  show session, split, remaining/total time, current/estimated laps, air/track
  temperature, brake bias, track-limit steps/threshold and local time.
- Game time and local time are independently configurable. Use the shared-memory
  time of day for game time, format both clocks with the selected locale and keep
  the session stopwatch, game-time clock and real-time globe icons distinct.
- Remaining time is the live countdown and displays `00:00` once exhausted. The
  compact total duration (`h m`) uses the official, fixed `maxTime` from local
  `/rest/watch/sessionInfo`; it is latched once per session and never reconstructed
  from elapsed plus remaining clocks.
- Category headers show current/initial counts and DNF/DQ difference outside
  practice. Recover initial counts from history when opened late.
- Pit status shares the driver cell and stays empty until a stop is observed.
  While the car is in pit lane, show only its elapsed pit-cycle timer. Once the
  stop is confirmed, retain `L<lap>`, the final compact duration and `P<count>`
  for the latest stop. The completed-stop count comes from shared-memory
  `mNumPitstops`; REST `pitstops` must not override it because it can overcount in
  team races. Accept a delayed counter increase before the next finish-line
  crossing, and do not invent elapsed time or a stop lap when the app starts
  during a partial stop.
- Opponent track-limit steps come from the matched all-vehicle telemetry slot.
  Unmatched means unavailable (`--`), not zero. Four raw SDK steps equal one game
  point. Warning thresholds are 60% and 80% of the session penalty threshold.
- NRG compares all-driver virtual energy only for regulated Hypercar/LMGT3.
  Never substitute the player's fuel percentage; other classes display `--`.
- Opponent damage is shared-memory integrity inverted: dents plus 50 points for a
  detached body part or 100 for a detached wheel, clamped to 100%.
- Signals may show yellow cause, pit, garage, stop/go and supported penalties.
  Empty signals have no persistent background. Blue, fastest-lap and time-penalty
  badges do not belong in this column.

## Visual and performance rules

- Keep the general session header visually anchored with the same restrained
  2 px lime left edge used by the Trailing + Pedal overlay. Category headers
  retain their own class-colored edge.

- Group each class in an outlined card with an angled filled category tab and
  separated row surfaces. Keep category accents/gradients aligned with Relative.
- In each category tab, render the helmet in black, enlarge the car count and
  center the class name, helmet and count from their real flex boxes. Reserve a
  compact shared class-name track so every helmet/count pair aligns in one
  vertical column without leaving excess space after the longest class name.
- Player tint is translucent lime with lime edge markers over the category layer;
  pit state remains distinguishable. Stop the player background before signals.
- Compact NRG, damage and track-limit cells are neutral until a meaningful
  warning applies. The integrated pit lap uses lime, while its active timer uses
  the same amber/orange semantic family as PIT and OUT.
- Cache row/header nodes and replace only changed cells/signatures. The general
  performance profile changes roster request and renderer cadence together.
- Derive the design height from the rendered header, class sections, rows,
  margins and padding instead of fixed row estimates, retaining only the shared
  72 px empty-state minimum. Scale the complete table without clipping so the
  edit border follows the rendered content throughout the 75–200% text range.
- Expand text-bearing column tracks together with the text-size design surface so
  enlarged labels and values cannot overlap neighboring columns. Manufacturer
  logos, driver badges, tyres and header SVG icons grow at half the rate of text
  (150% at the 200% text setting), together with their tracks where applicable.
  Keep the 195 px driver track at 100% and expand it at half the text-growth
  rate so the integrated pit summary remains anchored at its right edge;
  truncate driver names with an ellipsis. Position/change and
  assigned number use limited expansion, including the nested position/change
  tracks, so large values remain separated without the full text-column margin.
  Text-bearing tracks retain their base width at 100% and
  below so fixed-minimum contents such as DR/SR badges cannot overlap neighbors.
- Keep GAP, INT, lap-time, NRG and damage tracks at their compact measured
  minimums; retain a small internal gutter and let text scaling grow those tracks.
- The entry-specific transparent `.overlay-shell` override must have greater
  specificity than the shared rule because production CSS extraction reverses the
  apparent development cascade.

## Verification focus

Test lap transitions, invalid laps, late startup, driver swaps, pits, GAP/INT,
class selection/counts, track-limit availability, DR/SR retry and session reset.
Confirm 10 Hz requested cycles and reuse with Relative rather than a 50 Hz roster.

## Localization

Session names, empty states, counts, tooltips, table header and accessibility text
use the bundled locale. Cached rows translate only when their visible cell or
header changes; domain IDs and roster calculations remain stable.
The OBS route follows the shared saved locale or a page-local `?lang=` override.
