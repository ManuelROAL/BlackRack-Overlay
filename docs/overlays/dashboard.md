# Dashboard overlay

## Scope and files

- Entry: `dashboard.html`
- Renderer/style: `src/dashboard.ts`, `src/dashboard.css`
- Preferences: `src/dashboard-settings.ts`
  (`blackrack-overlay.dashboard.v1`, event `dashboard://settings`)
- Source fields: `src-tauri/src/telemetry/sim/lmu/bridge.cpp` and
  `src-tauri/src/telemetry/sim/lmu/source/frame.rs`
- Capability: `car_electronics`; the control-panel card is disabled for a source
  that does not publish it
- OBS route: `/dashboard`
- Cadence: the fast overlay cadence (50 Hz Smooth). The gear, the rev lights and
  the speed are the reason; everything else changes far more slowly but rides
  along on the same emission.

## Layout

One pill-shaped strip, not a cluster:

- The gear sits in a ring on the left. The ring carries the lime accent, because
  a pill has no straight edge for the shared left-edge bar.
- The rev lights are the same shift lights as Trailing + Pedal, through the
  shared `src/rpm-leds.ts`: twelve positions filling symmetrically from both
  ends between 84% and 96% of `max_rpm`, coloured by position — two green, two
  yellow, then the four reds in the middle — flashing cyan at the critical band
  and magenta over the rev limit. The segments stretch across the readout the
  way they do there too: a fixed-width cluster reads as a stray fragment in the
  corner once the strip is wide. The gear ring follows the same two thresholds.
- Everything else is a uniform label-over-value pair in a single row. Class
  position carries the accent colour, since it is the one value read as a
  standing rather than as a measurement.
- The pit limiter adds nothing to the strip. It blinks something already on
  screen — the gear ring in amber, or the shell's border and background when the
  strip is configured without the gear, so the warning never depends on which
  readouts the driver chose. Blinking colour rather than adding a badge also
  means the strip cannot change width while it is engaged.
- That blink is driven from the telemetry loop, reading the clock on each frame
  and flipping a data attribute every 450 ms. It is never a CSS animation: the
  shared `composite-embed` safeguard disables those precisely so the native host
  repaints from data instead of at the monitor refresh rate, which is also why
  Rejoin's pulse only runs on its standalone page. Reading the clock rather than
  counting frames keeps the rate identical across performance profiles.

## Visible fields and the cap

Every readout, the gear and the rev lights included, is one entry in a flat list
the control panel shows alphabetically by its Spanish label. **At most
`DASHBOARD_MAX_FIELDS` (8) may be on at once.** The strip is only readable while
it is short, so the cap is what protects it: the driver chooses what matters for
this car and this session instead of accumulating every field the game
publishes.

- A field marked `counts: false` has no readout of its own and takes no room on
  the strip, so it is not charged against the cap and the control panel never
  locks it when the cap is full. The pit limiter is the only one. Adding another
  means arguing that it is a warning rather than a readout.
- The three traction-control trims are separate fields, because a car can expose
  any combination of them: the level (`mTC`), the slip target (`mTCSlip`) and
  the cut (`mTCCut`). Each is gated on its own maximum, so a car without one
  simply never offers it.
- The control panel disables the remaining entries once the cap is reached and
  says so in the note under the list. Turning one off re-enables the rest.
- `normalizeDashboardSettings` also trims a configuration that asks for more —
  an imported document or a hand-edited one — keeping the first fields in list
  order rather than rejecting the whole configuration.
- The defaults are the eight that read the same in every car and every session:
  gear, rev lights, speed, class position, fuel, last lap, air and track
  temperature, plus the uncounted pit-limiter warning. Anything car-specific is
  opt-in.

## Data semantics

- Everything comes from the selected vehicle, so spectator and team mode read
  the followed car rather than the player's.
- The strip draws nothing until `player_active`; before that it shows the
  waiting copy rather than a row full of dashes.
- A field the car or the session cannot answer is dropped rather than drawn as a
  dash: on a strip this short an empty slot costs as much room as a real
  reading. That covers the engine map, traction control and ABS when the car has
  no such system, the battery without a hybrid, virtual energy when the session
  does not use it, and the delta and lap times before the models exist.
- `car_electronics_available` is derived from the published maxima rather than
  from the values: a neutral setting and an absent one are both zero, so only a
  non-zero maximum proves the car publishes the system. Each cell is then gated
  on its own maximum too — zero means absent, one means there is nothing to
  select, which is how an LMP2 reports ABS and the engine map.
- Battery charge reads `mSoC` when the car fills it and falls back to
  `mBatteryChargeFraction`. The two do not share a scale — the fraction is
  documented as 0..1 while the state of charge has been seen as a percentage —
  so the bridge normalises whichever one is present and clamps it to 0..100.
- Position prefers the class position, which is what a multiclass grid is
  actually raced on, and falls back to the overall place. Both are counted in
  the bridge, the class one over the scoring class name, which is filled for
  every car including those without per-vehicle telemetry.
- The range in laps is whichever budget runs out first: with virtual energy
  active that is usually the energy, not the tank. Consumption and range show
  `--` until they are learned, because zero is not a reading for either.
- Delta and lap times come from `delta_model` and `timing_model`. Both are built
  on demand, so the source loop treats an active Dashboard as a request for them
  exactly like the Delta and Timing overlays.

## Presentation

- The design surface follows the content on **both** axes through
  `fitOverlayToContentBox`, and the shell is `width: max-content`. Turning a
  field off therefore shortens the strip instead of leaving a gap: the eight
  defaults measure about 362 × 54, four fields about 179 × 54.
- Keep every readout the same size and weight. The strip earns its density from
  uniformity, so a value that wants to be special should get colour, not scale.
- The shell's small left padding is sized for the gear badge, whose circle
  already keeps clear of the pill's corner radius. A strip configured without
  the gear gets that clearance back through `data-gear="off"`, or the rev
  segments and the first label sit inside the curve.

## Invariants

- Never infer availability from a value alone. Map 0, TC 0 and 0% charge are all
  legitimate settings; the maxima and `hybrid_available` are the only signals.
- The overlay owns presentation only. Normalisation of the charge scale, the
  availability flags and the class-position count stay in Rust and the bridge.
- Do not let the field list grow without raising the cap deliberately. Adding an
  entry that pushes the strip past a glance is the failure mode this overlay
  exists to avoid.

## Verification focus

Check a Hypercar and an LMP2 in the same session: the battery, the engine map
and ABS must appear for one and be absent for the other, and the strip must
shorten accordingly. Confirm the limiter blink starts and stops entering and
leaving the pits, falls back to the shell with the gear turned off, never
counts against the cap and stays reachable in the control panel with the cap
full, and that each traction-control trim
appears only on a car that publishes a maximum for it. Confirm the cap disables
the remaining toggles at eight and releases them when one is turned off, that
the delta and lap times fill in with the Delta and Timing overlays closed, the
range with and without virtual energy, the OBS route and a configuration
export/import round trip.

## Localization

Field labels, the cap note, the waiting copy, the document title and
accessibility text use the bundled locale; values remain format-only. Its OBS
route follows the shared saved locale or a page-local `?lang=` override.
