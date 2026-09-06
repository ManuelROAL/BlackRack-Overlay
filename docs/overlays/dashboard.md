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
- The two warnings add nothing to the strip. They blink something already on
  screen — the gear ring, or the shell's border and background when the strip is
  configured without the gear, so a warning never depends on which readouts the
  driver chose. Blinking colour rather than adding a badge also means the strip
  cannot change width while one is engaged.
- The pit warning target is selectable: gear only (default), or the whole
  overlay, lighting both the gear and shell. With the gear hidden, either option
  falls back to the shell. Lift & Coast keeps its existing gear/shell behavior.
  `pitWarningTarget` is validated and persisted with dashboard preferences,
  including profiles and OBS; older settings default to `gear`.
- Each keeps the colour of the overlay that owns it, so the cue reads the same
  wherever it appears: amber for the pit limiter, and Lift & Coast's own purple
  (`#ad52ff`) for its cue, which follows `lift_and_coast_progress > 0` exactly
  as that overlay's segments do. The limiter wins when both are engaged; they
  only overlap on the way into the pits, where the limiter is the one with a
  penalty attached.
- That blink is driven from the telemetry loop, reading the clock on each frame
  and flipping a data attribute every 450 ms. It is never a CSS animation: the
  shared `composite-embed` safeguard disables those precisely so the native host
  repaints from data instead of at the monitor refresh rate, which is also why
  Rejoin's pulse only runs on its standalone page. Reading the clock rather than
  counting frames keeps the rate identical across performance profiles.

## Visible fields

Every readout, the gear and the rev lights included, is one entry in a flat list
the control panel shows alphabetically by its Spanish label. Any number of
fields may be enabled, including all of them. The strip grows with its content.

- The pit limiter and Lift & Coast light existing elements instead of adding
  readouts.
- The three traction-control trims are separate fields: level (`mTC`), slip
  target (`mTCSlip`) and cut (`mTCCut`). Each is gated on its own maximum.
- `normalizeDashboardSettings` preserves every selected known field when loading
  settings, profiles or imported configuration, validates booleans and supplies
  defaults for missing fields.
- Defaults remain gear, rev lights, speed, class position, fuel, last lap, air
  and track temperature, plus both warnings. Car-specific fields are opt-in.

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
- Range reads Rust's shared `resource_autonomy.range_laps`, exactly like Fuel's
  summary. It uses projected consumption with the same fallback order as strategy,
  rather than the clean average. With energy active, the smaller fuel/energy range
  wins; both references must exist. Unknown is `--`, while an empty resource with
  a valid consumption reference is `0.0`. This model runs even with Fuel disabled.
- Delta and lap times come from `delta_model` and `timing_model`. Both are built
  on demand, so the source loop treats an active Dashboard as a request for them
  exactly like the Delta and Timing overlays.

## Presentation

- Changes to brake bias, engine map, TC level/slip/cut and ABS show a centered
  label and new value over the existing strip for three seconds, including when
  that readout is disabled. Repeated changes restart the duration; the latest
  change replaces the previous notice (simultaneous changes follow field order,
  with brake bias taking precedence). Values are compared at their displayed
  precision so sub-display brake-bias noise does not trigger a notice.
- Initial telemetry, inactive players, a changed source/track/vehicle name/session
  type and newly available electronics establish a baseline without a notice.
  Identity detection is limited to those published fields; cars with identical
  names cannot be distinguished here. Preview values never seed live notices.
- Notices preserve layout and transparency, hide underlying text for readability,
  and expire on telemetry updates without animations or an independent timer.
  Native field projection includes the same context used by the OBS renderer.

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

## Verification focus

Switch the pit warning between gear and whole-overlay targets while active.
Confirm both clear when it stops, hidden gear falls back to the shell, and the
choice survives reload, profiles and export/import.

Change BB repeatedly and change each available electronics trim, including with
its readout disabled: confirm the new value overlays the strip for three seconds
after the last change, without resizing. Check initial/reconnected telemetry,
unavailable systems and context changes do not generate false notices. Check
compact gear-only layouts, text scaling, transparency and OBS as well.

Check a Hypercar and an LMP2 in the same session: the battery, the engine map
and ABS must appear for one and be absent for the other, and the strip must
shorten accordingly. Confirm both warnings start and stop with their own
state — the limiter entering and leaving the pits, the purple cue with Lift &
Coast's own segments — that they fall back to the shell with the gear turned
off, that the limiter wins while both are engaged, and that each traction-control
trim appears only on a car that publishes a maximum for it. Enable more than
eight fields, including all fields, and confirm every toggle stays available and
the selection survives reload and export/import. Confirm
the delta and lap times fill in with the Delta and Timing overlays closed, the
range with and without virtual energy, the OBS route and a configuration
export/import round trip.

## Localization

Field labels, the waiting copy, the document title and
accessibility text use the bundled locale; values remain format-only. Its OBS
route follows the shared saved locale or a page-local `?lang=` override.
