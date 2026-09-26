# Trailing + Pedal overlay

## Scope and files

- Entry: `driving.html`
- Renderer/style/settings: `src/driving.ts`, `src/driving.css`,
  `src/driving-settings.ts`
- OBS route: `/driving`
- Cadence: 50 Hz input sampling, canvas repaint up to 25 Hz

## Telemetry semantics

- Use the player's unfiltered throttle, brake and clutch for pedal traces.
- LMU's `mUnfilteredClutch` is exposed as the shared `TelemetryFrame.clutch`
  value. It represents driver input, not clutch slip or the simulator's final
  engagement state.
- LMU's explicit `mTCActive` and `mABSActive` fields are authoritative. Do not
  infer intervention from filtered pedals, which can reflect other controls.
- Plot TC at the throttle trace height and ABS at the brake trace height for each
  individual 50 Hz sample.
- Normalize signed `FFBTorque` to `[-1, 1]` for the bipolar FFB bar.
- Steering angle matches the physical and rendered in-game wheel: multiply
  `mUnfilteredSteering` by half the active `VM_STEER_LOCK` range from
  `/rest/garage/getPlayerGarageData`. LMU shared memory can expose the car's
  nominal range instead of the active controller range; use its physical, then
  visual range only while the REST value is unavailable. Do not use
  `mFilteredSteering`, which can include vehicle steering processing.

## Layout and configuration

- Keep one continuous compact dark surface with a restrained dark gradient,
  rounded neutral shell, thin internal separators and a 2 px lime edge accent.
  The dial, pedal meters and history trace remain visually related areas, not
  independent cards; avoid decorative blur or extra animation.
- Use a 468 × 120 px base surface before optional blocks compact its width.
- Left-to-right composition is: a crisp circular dial combining the rotating
  steering-centre arc, prominent central gear, speed and the FFB bar below it;
  clutch/brake/throttle meters; and the wide five-second trace in a quiet,
  subtly framed surface. Pedal meters use clear value and abbreviated label
  hierarchy above clean segmented tracks. Keep green for throttle, coral for
  brake and cyan for clutch; TC remains blue and ABS yellow.
- The settings section assigns the graph, current-input meters and driving-data
  dial independently to the left, center or right. Every location is exclusive:
  choosing an occupied location swaps the two sections. Defaults are driving
  data left, inputs center and graph right. Older profiles with the graph at the
  left retain their historical graph → dial → inputs order.
- An optional 12-segment RPM strip spans the top of the panel. Based on the
  established TinyPedal behavior rather than copied source, it illuminates
  progressively from 84% to 96% of `max_rpm`, symmetrically from both outer
  edges toward the centre. Each side advances from two green through two yellow
  to two red segments, placing the four red segments in the centre. At 96% all
  segments flash cyan; at 99.99% they flash magenta. It is enabled by default.
  The thresholds, the symmetric fill and the colour band of each position live
  in `src/rpm-leds.ts`, shared with Dashboard's strip: two panels disagreeing
  about when to shift is worse than either being slightly wrong. Change the
  reading there, not here.
- Represent the in-game steering angle with the rotating white centre arc; do
  not add a separate numeric angle readout.
- Keep the trace free of a title and color legend. The established pedal-line and
  TC/ABS marker colors identify its contents while preserving graph space.
- Render TC intervention markers in electric blue and ABS intervention markers in
  vivid yellow so both alerts remain distinct from the pedal traces.
- The complete graph is optional without losing its selected pedal traces. Graph
  pedals and current-input pedals are independently selectable. Steering, FFB,
  speed, gear and the RPM strip are independently selectable too.
- Abbreviated labels above the current-input pedal bars can be hidden
  independently; the pedal bars and numeric values remain visible. Labels are
  shown by default.
- Compact panel width when a complete block is hidden while preserving current
  visual scale. Mirror every option in OBS browser-source preferences.
- Keep gear, speed and the FFB readout vertically separated throughout the
  supported 75–200% text-size range without enlarging the compact dial surface.

## Performance invariants

- Preserve the complete five-second history and all 50 Hz intervention samples.
  A 25 Hz canvas repaint reduces presentation cost without reducing sampling.
- Do not add CSS transitions to pedal, FFB or steering values. They keep WebView2
  composing between telemetry frames.
- Cache hot DOM/canvas references and skip unchanged writes.

## Display units

The dial speed follows the global km/h or mph preference, including its unit
label. The frame stays in km/h.

## Verification focus

Check rapid steering/FFB/pedal/clutch changes, the RPM strip through its color thresholds
and limiter state, TC/ABS single-sample markers, five-second history, all visibility
combinations, compact sizing and OBS preference parity.

## Localization

Static labels, tooltips and accessibility text use the bundled locale. Pedal IDs,
units and telemetry sampling remain unchanged. Its OBS route follows the shared
saved locale or a page-local `?lang=` override.
