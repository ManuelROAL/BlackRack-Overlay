# Flags overlay

## Scope and files

- Entry: `flags.html`
- Renderer/style: `src/flags.ts`, `src/flags.css`
- Backend inference: `src-tauri/src/telemetry/lmu.rs`
- OBS route: `/flags`
- Cadence: 50 Hz while active; 250 ms while inactive

## Priority and display

- Checkered always has highest priority and shows no distance or car details.
- Green contains no car information.
- Yellow and blue show distance, class position and category. Yellow includes an
  ahead/behind arrow.
- Blue uses the player's official flag state and nearest plausible faster/lapping
  car behind.

## Yellow semantics

The consumed official shared-memory subset does not expose exact yellow
causation, so describe the result honestly as a stabilized inference.

- Standings may always flag the likely slow-car culprit using the preventive
  below-8-m/s rule, independent of player proximity.
- This dedicated overlay additionally requires an LMU sector yellow and a nearby
  relevant candidate: up to 500 m ahead, otherwise up to 50 m behind.
- Stabilize candidate evidence for about one second to avoid treating normal
  braking as an incident.
- Exclude cars in pitlane.

## Verification focus

Test priority changes, sector-yellow gating, front/rear thresholds, stabilization,
pit exclusion, blue selection and responsiveness at active cadence.
