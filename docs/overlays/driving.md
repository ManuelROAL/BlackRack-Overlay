# Trailing + Pedal overlay

## Scope and files

- Entry: `driving.html`
- Renderer/style/settings: `src/driving.ts`, `src/driving.css`,
  `src/driving-settings.ts`
- OBS route: `/driving`
- Cadence: 50 Hz input sampling, canvas repaint up to 25 Hz

## Telemetry semantics

- Use the player's unfiltered throttle and brake for pedal traces.
- LMU's explicit `mTCActive` and `mABSActive` fields are authoritative. Do not
  infer intervention from filtered pedals, which can reflect other controls.
- Plot TC at the throttle trace height and ABS at the brake trace height for each
  individual 50 Hz sample.
- Normalize signed `FFBTorque` to `[-1, 1]` for the bipolar FFB bar.
- Steering angle is `mUnfilteredSteering` times half the physical wheel range;
  use visual range only when physical range is unavailable.

## Layout and configuration

- Keep one continuous compact dark surface with thin internal separators and a
  restrained lime edge accent, not independent cards.
- Left-to-right composition is: a circular dial combining the rotating
  steering-centre arc, central gear and speed below it; clutch/brake/throttle
  meters; a separate FFB bar; and the wide five-second trace.
- Show the physical steering angle in degrees.
- Graph pedals and current-input pedals are independently selectable. Steering,
  FFB, speed and gear are independently selectable too.
- Compact panel width when a complete block is hidden while preserving current
  visual scale. Mirror every option in OBS browser-source preferences.

## Performance invariants

- Preserve the complete five-second history and all 50 Hz intervention samples.
  A 25 Hz canvas repaint reduces presentation cost without reducing sampling.
- Do not add CSS transitions to pedal, FFB or steering values. They keep WebView2
  composing between telemetry frames.
- Cache hot DOM/canvas references and skip unchanged writes.

## Verification focus

Check rapid steering/FFB/pedal changes, TC/ABS single-sample markers, five-second
history, all visibility combinations, compact sizing and OBS preference parity.
