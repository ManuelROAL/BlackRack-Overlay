# Lift & Coast overlay

## Scope and files

- Entry: `liftcoast.html`
- Renderer/style: `src/liftcoast.ts`, `src/liftcoast.css`
- OBS route: `/liftcoast`
- Cadence: 50 Hz native input

## Telemetry semantics

- Use LMU's official `TelemInfoV01::mLiftAndCoastProgress` byte from shared
  memory. Do not infer the cue from pedals, lap distance, fuel use or learned
  braking points.
- Zero switches every lamp off. Values 1–5 illuminate that many segments from
  left to right. Until a live LMU capture documents values above five, treat
  them as an active fully-lit cue rather than inventing a percentage scale.
- The panel mirrors LMU's lamp state only. It does not create an additional
  braking flash when the official value returns to zero.

## Layout and behavior

- The native panel is disabled in spectator mode; its saved visibility is retained
  for game and team modes.

- Show only one row of five purple segments, without a visible label. The
  design surface is 190 × 32 px, with 2 px outer padding and a minimal frame.
- The panel remains mounted while inactive so its configured position is stable;
  inactive lamps are deliberately dim.
- Preserve proportional resize, general/per-overlay transparency and text size,
  click-through game mode, composite embedding and the `/liftcoast` OBS route.
- Do not add continuous animation, blur or transitions. Repaint only when the
  telemetry value changes.

## Verification focus

- Confirm 0 and 1–5 against the mock preview and changed-value rendering.
- In LMU, compare the overlay with the cockpit lamps using Target Laps `+1` and
  `+2`; record any value above five before changing the mapping.
- Check native composite placement, reset, import/export and `/liftcoast` OBS.
