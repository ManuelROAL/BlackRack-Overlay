# Flags overlay

## Scope and files

- Entry: `flags.html`
- Renderer/style: `src/flags.ts`, `src/flags.css`
- Backend inference: `src-tauri/src/telemetry/sim/lmu/source/warnings.rs`
- OBS route: `/flags`
- Cadence: 50 Hz while active; 250 ms while inactive

Candidate scanning and warning inference run only while the native panel or the
`/flags` OBS route is active.

## Priority and display

- Checkered always has highest priority and shows no distance or car details.
- Green contains no car information.
- Yellow and blue show distance, class position and category. Yellow includes an
  ahead/behind arrow.
- Larger text expands only the height needed to keep distance and car details
  separated throughout the supported 75–200% range.
- Blue uses the player's official flag state and nearest plausible faster/lapping
  car behind. Cars in pitlane, in the garage or already finished are excluded.
## Yellow semantics

The consumed official shared-memory subset does not expose exact yellow
causation, so describe the result honestly as a speed-based inference that is
gated by LMU's official sector-yellow mask.

- Standings and trackmap may always flag the likely slow-car culprit using the
  below-8-m/s rule, independent of player proximity. As in TinyPedal, cars in
  pit lane count as candidates; only cars in the garage are excluded.
- This dedicated overlay additionally requires an LMU sector yellow and a nearby
  relevant candidate: up to 500 m ahead, otherwise up to 50 m behind.
- The nearest candidate ahead has priority over candidates behind, and distances
  use signed circular track position: positive is ahead and negative behind.

## Verification focus

Test priority changes, sector-yellow gating, front/rear thresholds, stabilization,
pit exclusion, blue selection, and responsiveness at active cadence.

## Localization

The document title and accessibility text use the bundled locale. Flag kinds,
category names and compact motorsport notation stay semantic.
Its OBS route follows the shared saved locale or a page-local `?lang=` override.
