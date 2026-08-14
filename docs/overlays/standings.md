# Standings overlay

## Scope and files

- Entry: `standings.html`
- Renderer/style/settings: `src/standings.ts`, `src/standings.css`,
  `src/standings-settings.ts`
- Rust view model: `src-tauri/src/telemetry/standings_models.rs`
- Timing/history/enrichment: `src-tauri/src/telemetry/lmu.rs`,
  `driver_ranks.rs`, `event_split.rs`, `lmu_rest.rs`
- OBS route: `/standings`
- Cadence: enriched roster and renderer at 10 Hz

Rust owns class grouping, class order/counts, SOF and visible-row selection. The
renderer resolves prepared vehicle IDs and owns presentation only; it must not
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
  GAP, INT, best, last, AVG 5, NRG, damage, track-limit steps, pit, tyre and
  signals. Position and driver are mandatory; signals stays fixed at the far
  right. Other columns are independently visible and reorderable.
- Use the session-assigned number and shorten the name to first name plus one
  surname. Prefer bundled manufacturer, nationality, badge and tyre assets.
- A uniform tyre set uses the bundled compound SVG; mixed sets use four compact
  colored circles.

## Timing and history semantics

- GAP is relative to the class leader; INT is relative to the preceding car in
  class. Derive them from continuous shared-memory lap progress.
- Reconstruct a temporary same-lap zero gap from continuous progress. Show a lap
  difference only after a complete physical lap separates the cars.
- The category leader displays its current lap once in GAP and once in INT.
- Lap times use three decimals; GAP/INT use one decimal. Personal best is green
  and session fastest is purple.
- A completed invalid lap remains visible in gray. If `mLastLapTime` is zero,
  reconstruct it from consecutive `mLapStartET` values only after the new lap has
  been active for more than one second. Discard stable partial resets below 50%
  of official best or estimated pace; keep plausible invalid slow laps.
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
  grid; derive class starting position from REST `qualification`. Never invent a
  duration for history `lapTime = -1`.
- RaceControl supplies DR, progress, ELO when available, SR, nationality and badge.
  In qualifying/race, complete the one-per-roster refresh only after a successful
  non-empty result; failed, empty and partial results remain retryable.
- In registered events, the one-time `event/my-split` roster supplements missing
  or `XX` nationality and missing badges without replacing valid `/players` data.
- The DR estimate is same-class and event-parameter aware. Convert it back from
  the three-times internal rating scale before serializing visible progress.
- SOF uses resolved continuous DR and reports partial coverage. Do not calculate or
  display SOF in practice.

## Header and states

- Keep complete header visibility separate from each header datum. The header may
  show session, split, remaining/total time, current/estimated laps, air/track
  temperature, brake bias, track-limit steps/threshold and local time.
- Remaining time is the live countdown and displays `00:00` once exhausted. The
  compact total duration (`h m`) uses the official, fixed `maxTime` from local
  `/rest/watch/sessionInfo`; it is latched once per session and never reconstructed
  from elapsed plus remaining clocks.
- Category headers show current/initial counts and DNF/DQ difference outside
  practice. Recover initial counts from history when opened late.
- The pit column normally shows completed stops. From pit entry through the end of
  the out lap, show elapsed pit-cycle seconds. Do not invent elapsed time when the
  app starts during a partial stop. A pit request is green when no timer is shown.
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
- Player tint is translucent lime with lime edge markers over the category layer;
  pit state remains distinguishable. Stop the player background before signals.
- Compact NRG, damage, track-limit and pit cells are neutral until a meaningful
  warning or active state applies.
- Cache row/header nodes and replace only changed cells/signatures.
- Grow the design from its 450 px height minimum for configured rows. Scale the
  complete table without clipping.
- The entry-specific transparent `.overlay-shell` override must have greater
  specificity than the shared rule because production CSS extraction reverses the
  apparent development cascade.

## Verification focus

Test lap transitions, invalid laps, late startup, driver swaps, pits, GAP/INT,
class selection/counts, track-limit availability, DR/SR retry and session reset.
Confirm 10 Hz requested cycles and reuse with Relative rather than a 50 Hz roster.
