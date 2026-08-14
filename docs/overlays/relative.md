# Relative overlay

## Scope and files

- Entry: `relative.html`
- Renderer/style/settings: `src/relative.ts`, `src/relative.css`,
  `src/relative-settings.ts`
- Rust selection model: `src-tauri/src/telemetry/standings_models.rs`
- OBS route: `/relative`
- Cadence: enriched roster and renderer at 20 Hz

Rust owns physical row selection, row roles and selected gaps. TypeScript resolves
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

## Columns and header

- Position, driver and physical time are mandatory. Configurable columns include
  number, country, license, DR/SR and estimate, position change, lap, AVG 5,
  last/best, NRG, damage, per-driver track limits, pit, tyre and flags.
- Column visibility and order are independent from Standings. Signals remains
  fixed at the transparent far right. Relative has no column-label row or footer.
- Keep full header visibility separate from air/track temperature, brake bias,
  player track-limit counter/threshold and local clock toggles.
- Apply the same asset, NRG, damage, tyre, pit, DR/SR and track-limit semantics as
  Standings; do not create divergent formatting rules for shared cells.
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
- Derive design width from active columns with a compact 760 px minimum. Reserve
  23 px for every selected row so the last card is not clipped.
- Cache DOM rows and update at 20 Hz. Cycles coinciding with Standings reuse the
  same constructed roster.
- Give the transparent `.overlay-shell` override greater specificity than the
  shared surface rule for production Vite extraction.

## Verification focus

Test lap-wrap duplication, multiclass/lapped traffic, garage exclusion, sign and
ordering of gaps, row limits, independent column/header settings and fitted size.
Confirm 20 Hz roster demand and Standings reuse.
