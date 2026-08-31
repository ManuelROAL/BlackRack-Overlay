# Relative overlay

## Scope and files

- Entry: `relative.html`
- Renderer/style/settings: `src/relative.ts`, `src/relative.css`,
  `src/relative-settings.ts`
- Rust selection model: `src-tauri/src/telemetry/standings_models.rs`
- OBS route: `/relative`
- Cadence: enriched roster and renderer at 20 Hz in Smooth, 12.5 Hz in Balanced
  and approximately 8.3 Hz in Efficiency

Rust owns physical row selection, row roles and selected gaps. Its view model is
built only when the native panel or `/relative` route is active. TypeScript resolves
the prepared vehicle IDs and formats them; it must not repeat domain selection.

## Physical ordering

- Show the nearest configured cars physically ahead, the player and nearest cars
  physically behind, regardless of race position, class or completed lap count.
- Use TinyPedal-style circular timing based on estimated time into lap and player
  lap pace. Negative time is ahead and positive time is behind.
- Configure 1 to 10 rows per direction (default 4). Exclude garage cars before
  selection.
- A driver may occur once ahead and once behind at the lap wrap, but never more
  than once in either direction.
- The physical time column is mandatory and visually larger than secondary data,
  without extra width, a special background or heavier weight.

## At-a-glance events

- Keep physical time as the dominant value and layer events onto the existing row;
  do not add a second table or replace physical ordering with race order.
- During races, tint the right side blue when the player is a lap or more ahead
  of that car and red when that car is a lap or more ahead of the player. Do not
  infer lapping from lap counts in practice or qualifying. Preserve the official
  class accent on the left and the lime player treatment.
- Derive the lap relationship in Rust from completed laps plus continuous lap
  phase. Do not flash a false lapping event while only one car has crossed the
  timing line.
- Show compact, cumulative `OUT` and `PIT` chips in Signals. `PIT` is green while
  a stop is requested and orange once the car is in the pit lane. Damage is represented
  only by a restrained underline on the driver name, becoming red at the shared
  50% heavy-damage threshold. Keep the underline inside the name bounds so text
  scaling cannot clip it.
- For a red car physically behind, describe the state as about to lap the player;
  for a red car ahead, describe it as having lapped the player. Blue consistently
  means the player has lapped that car.

## Columns and header

- Position, driver and physical time are mandatory. Configurable columns include
  number, country, license, DR/SR and estimate, position change, lap, AVG 5,
  last/best, NRG, damage, per-driver track limits, pit, tyre and flags.
- Column visibility and order are independent from Standings. Signals remains
  fixed at the transparent far right. Relative has no column-label row or footer.
- Driver-name format is independent from Standings and supports full name,
  initial plus surname, name plus surname initial, surname only, name only and
  surname plus name initial.
- Keep full header visibility separate from air/track temperature, brake bias,
  player track-limit counter/threshold, game-time and local-clock toggles. Format
  both clocks with the selected locale and distinguish the game-time clock from
  the real-time globe icon.
- Apply the same asset, NRG, damage, tyre, pit, DR/SR and track-limit semantics as
  Standings, including the sidewall-style uniform-compound SVGs; do not create
  divergent formatting rules for shared cells.
- Apply Standings' completed-lap validity semantics to LAST, including the latched
  `mLapInvalidated`, negative official-time signals and reconstructed invalid
  durations when LMU omits the official time; that missing official result also
  confirms the reconstructed lap as invalid. Invalid LAST values are gray and
  take visual precedence over personal-best or session-fastest coloring.
- Reuse Standings' event-roster fallback for missing/`XX` nationality and badges.
- If a resolved country asset fails to load, replace it with the bundled `XX`
  marker; remove the image if that fallback also fails instead of leaving a
  broken-image glyph in the row.

## Visual and sizing rules

- Use one neutral outlined physical-order table with a framed header and separated
  row cards. Do not split it into class groups.
- Keep each row's official class accent and short gradient aligned with Standings.
  Use Hypercar red, LMP2 blue, LMP3 purple and LMGT3 green. Preserve distinct
  player and pit layers, including LMGT3 contrast.
- Derive design width from active columns with a compact 344 px minimum. New and
  reset configurations default to number, driver, physical time and Signals;
  advanced identity, timing and strategy columns remain configurable. Reserve
  23 px for every selected row so the last card is not clipped.
- Expand text-bearing column tracks together with the text-size design surface so
  enlarged labels and values cannot overlap neighboring columns. Keep the
  country-flag, driver-badge and 95 px driver-name tracks fixed; truncate driver
  names with an ellipsis when they exceed that space. Position/change and assigned
  number use limited expansion so large values remain separated without the full
  text-column margin. Text-bearing tracks retain their base width at 100% and
  below so fixed-minimum contents such as DR/SR badges cannot overlap neighbors.
- Cache DOM rows and update at the selected performance-profile cadence. Cycles
  coinciding with Standings reuse the same constructed roster.
- Give the transparent `.overlay-shell` override greater specificity than the
  shared surface rule for production Vite extraction.

## Verification focus

Test lap-wrap duplication, multiclass/lapped traffic, timing-line transitions,
garage exclusion, sign and ordering of gaps, event combinations, row limits,
independent column/header settings and fitted size. Confirm 20 Hz roster demand
and Standings reuse.

## Localization

Empty states, tooltips, lap relations, session header and accessibility text use
the bundled locale. Translations are resolved only when cached cells or headers
change, preserving the 20 Hz presentation path.
The OBS route follows the shared saved locale or a page-local `?lang=` override.
