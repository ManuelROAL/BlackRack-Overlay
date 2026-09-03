# Simulators

The app reads one simulator at a time through a single contract. Everything that
knows a simulator exists lives under `src-tauri/src/telemetry/sim/<id>/`; every
other module — the frame, the 50 Hz loop, the domain models and the whole
frontend — is simulator agnostic and must stay that way.

Le Mans Ultimate is complete. iRacing is being built out one overlay at a time
and today feeds Driving; what it does and does not fill is recorded below.

## The contract

`src-tauri/src/telemetry/sim/mod.rs` owns five things:

| Item | Purpose |
| --- | --- |
| `TelemetrySource` | `descriptor()` plus `next_frame(demand)`, called from the loop thread |
| `SourceDescriptor` | id, display name, capabilities, and two optional function pointers |
| `SourceCapabilities` | what the simulator can *ever* report |
| `CANDIDATES` | the simulators, in priority order, each with an `available` probe and a `try_new` |
| `detect(app_data)` | builds the source that follows whichever candidate is running |

The descriptor carries function pointers rather than source methods for the two
things the app asks for outside the loop thread:

- `official_geometry` — the track outline, requested by the `get_track_map_geometry`
  command and by the browser-source server.
- `dependency` — whether the simulator's plugin, SDK or service is installed,
  requested by `get_simulator_status` for the control panel.

A source that has neither leaves both `None`; `track_geometry.rs` then falls back
to the outline learned from laps, and the panel simply does not report a
dependency.

## Choosing the simulator

`detect` returns a source that keeps looking. It builds the first candidate whose
`available` probe answers yes, falling back to the mock, and then on every cycle
where the frame comes back disconnected — or where the mock is active, since it
is always "connected" — it re-probes at most once every two seconds and swaps in
the first available candidate whose id differs from the current one.

That is what makes launching the app before the game work, and what lets one
simulator be closed and another opened without a restart. A connected source is
never displaced, and when nothing is available the current source is kept, so a
waiting simulator keeps reporting itself instead of falling back to the mock.

An `available` probe must be cheap and must answer about *this machine, right
now*: iRacing opens its memory mapping, which only exists while it runs; LMU
answers whether the build has the SDK, because its bridge cannot be asked more
cheaply than that. Consequently LMU is the last candidate: a build that has its
SDK is always "available", so anything below it would never be reached.

`try_new` is a different question from `available` and must not assume it was
already checked: it answers whether this build can ever construct the source at
all (wrong OS, no SDK), not whether the simulator is running. Both LMU's and
iRacing's `try_new` succeed unconditionally on a capable build; the source they
return handles "not running yet" itself, per frame, the same way it handles a
mid-session disconnect. That gap is what lets a pinned preference (below)
construct a source before its simulator has even started, so it reports
"waiting for iRacing" instead of a fabricated mock frame.

## Pinning a preference

The control panel's header carries a compact select, next to the connection
pill, with Auto plus one option per `CANDIDATES` entry, labelled from
`sim::options()` — never a literal name in TypeScript or the catalogs, the same
rule visible copy already follows.
`set_simulator_preference` (`sim::set_preference`) stores the choice as an
index into `CANDIDATES`, with `CANDIDATES.len()` standing for "auto", and bumps
a generation counter. `SelectedSource::next_frame` compares that counter every
cycle and reacts on the one cycle it changes: a pin builds the chosen
candidate's own source regardless of `available`, and returning to Auto lets
the normal priority order pick. This forced switch is what keeps a pin from
being silently overridden by auto-reselecting the moment the pinned simulator
happens to be closed — a pinned choice reports "waiting", not a different
simulator's data.

The preference itself is frontend state, following the same pattern as the
performance profile: `src/simulator-settings.ts` persists it to `localStorage`
and the control panel resends it to the backend on load. It resets to Auto on
every cold start of the Rust process, same as the profile resets to Smooth,
until the frontend's startup call lands a cycle or two later.

## Capabilities are not availability

Two different questions share the frame and must not be confused:

- **Capability** — can this simulator *ever* report it? Fixed per simulator,
  declared in the descriptor, delivered as `frame.capabilities`. The control
  panel uses it to retire an overlay that would never have data.
- **Availability** — is it there *in this session*? Already carried by the frame
  as `virtual_energy_active`, `rest_weather_available` and the `-1` sentinels.
  The renderers use it to hide a value that is missing right now.

Only five overlays are gated by capability, because each exists for one of them:
`damage` (`damage_detail`), `forecast` (`weather_forecast`), `liftcoast`
(`lift_and_coast`), `pitstop` (`pit_service_estimate`) and `dashboard`
(`car_electronics`). Every other overlay degrades inside its own renderer.

## Adding a simulator

1. Create `src-tauri/src/telemetry/sim/<id>/mod.rs` with a `DESCRIPTOR`, an
   `available() -> bool` probe and a
   `try_new(app_data) -> Option<Box<dyn TelemetrySource>>` returning `None` when
   the simulator cannot be read in this build or on this machine.
2. Implement `TelemetrySource` for it. Fill the frame with the semantics the
   agnostic modules already expect; leave what the simulator does not report at
   the sentinel values the frame documents, and turn the matching capability off.
3. Declare the capability set honestly. A capability that is on but never filled
   is worse than one that is off: the panel will offer an overlay that stays empty.
   A simulator that is landing one overlay at a time turns each capability on in
   the change that fills the fields behind it, not before.
4. Add the module to `sim/mod.rs` and an entry to `CANDIDATES`. Order is
   priority, and a candidate whose `available` cannot fail goes last.
5. Reuse the domain: `delta_records`, `fuel_strategy`, `standings_models`,
   `track_map_model`, `track_geometry`, `consumption_profile` and `strategy_log`
   are shared and must not gain a simulator-specific branch.
6. If the simulator has an SDK that has to be found at build time, follow the LMU
   pattern in `build.rs`: one detection function, one `rustc-check-cfg`, one
   `rustc-cfg`, and gate only the module that needs the symbols.
7. Update this document and `docs/ARCHITECTURE.md`.

## iRacing

Still experimental: it is marked `experimental: true` in `CANDIDATES`, so only
a build with `experimental-simulators` offers it. See *Shipping an unfinished
simulator* below, and clear the flag once the overlays below are filled.

The simulator publishes a memory-mapped file that only exists while it runs:
a header, a table describing every telemetry variable, a small ring of value
buffers and a YAML session string. Nothing is needed at build time, so
`sim/iracing/` compiles on every Windows build and `available()` is simply
whether that mapping opens.

- `irsdk.rs` maps the file and reads it by documented byte offsets rather than a
  `#[repr(C)]` mirror, bounds every read against the region size, and adopts a
  value buffer only when its tick did not advance during the copy.
- `yaml.rs` parses the restricted dialect the session string uses — block maps,
  block sequences of maps and plain scalars — without a dependency.
- `session.rs` reparses at most once a second and only when the generation
  changes, because the string is large and is republished as results change.
- `foreground.rs` answers whether the simulator owns the foreground window,
  which the telemetry does not report and the overlay host needs.

What it fills today is the session and car state the host uses to decide what to
show, plus the Driving values: speed, gear, RPM against the published redline,
throttle, brake, ABS, steering angle and wheel torque. Traction control has no
published state, so its indicator stays off rather than being inferred.

Everything else in the frame is still at its documented sentinel and every
capability is off, which is what keeps the control panel from offering an
overlay that would stay empty. Landing an area means filling its fields, turning
its capability on in the same change, and recording it here. The known shape of
the remaining work:

| Area | Source in the simulator |
| --- | --- |
| Standings, Relative, Track Map | the `CarIdx*` arrays joined to the roster in the session string; no world coordinates for other cars, so the outline has to be learned from the player's own position |
| Fuel | `FuelLevel` tracked across laps; there is no energy budget, so `virtual_energy_*` stays inactive |
| Tyres | the per-corner wear and carcass temperature variables |
| Conditions | `AirTemp`, `TrackTempCrew`, `Precipitation`, `TrackWetness`, `Skies`, wind |
| Damage, Forecast, Lift and coast, Pit stop | not published; these four stay capability-gated off permanently |

## Shipping an unfinished simulator

A simulator is rarely useful the day its source compiles: it fills the frame
one overlay at a time, and until it is done a build that offers it shows empty
overlays and a picker entry nobody should choose. `Candidate::experimental`
marks such a simulator, and `shipped()` decides whether this build lets the
user reach it:

- **`npm run tauri:dev`** enables the `experimental-simulators` feature, so
  every candidate appears in the picker and in automatic selection.
- **`npm run tauri dev`** and **`npm run tauri build`** leave the feature off.
  An experimental candidate is then absent from the picker, never chosen
  automatically, and refused by `set_simulator_preference` — so a preference
  pinned in a development build cannot resurrect it. The control panel drops
  the picker entirely while a single simulator is offered.

The filter is deliberately in `shipped()` rather than on the `CANDIDATES`
declaration: the module stays compiled and its tests keep running in every
configuration, so a simulator waiting to be finished cannot rot. `cargo test
--lib` covers the same 250 tests with and without the feature.

Clear the flag when the simulator fills the overlays its capabilities claim.

## The guardrail

`npm run check:overlays` (`tools/validate-overlays.mjs`) reads the simulator
roster out of `sim/mod.rs` and then fails when:

- a Rust file outside `telemetry/sim/` mentions `sim::<id>`, or hard-codes a
  simulator's display name;
- visible copy — the catalogs, or text between tags in the HTML entries —
  names a simulator instead of taking `{simulator}` as a parameter.

Asset paths are not copy, so `src/assets/lmu-icons/` is untouched: those icons
are lifted from LMU's own interface and the name records where they came from.

## What is not prepared yet

- **Settings are global.** Overlay preferences, profiles and the exported
  configuration document are not namespaced per simulator, so switching now
  carries one simulator's layout into the other. The migration is a
  per-simulator prefix on the `localStorage` keys in `src/main.ts` plus a
  `simulator` field in the configuration document with a `schemaVersion` bump.
  A second simulator exists, so this is owed rather than hypothetical.
- **Learned data is keyed by track and car, not by simulator.** The delta
  records, the learned track outline and the consumption profiles share one
  store. Two simulators are unlikely to agree on a track name, so today they
  simply learn separate entries; a collision would mix them.
- **Renderers do not read capabilities.** Only the control-panel catalog does.
  An overlay that should show fewer columns for a given simulator still has to
  learn that itself.
- **One persisted value still says `lmu`.** The timing overlay's sector
  reference mode is stored and sent to Rust as `"lmu"`, meaning "the game's own
  delta". Its label is already neutral; renaming the stored value would need a
  settings migration and is not worth one on its own.

## Verifying the seam

The mock source is the standing proof that the contract works with more than one
implementation. Build against it on a machine that has the game by pointing the
override at a directory without the SDK header:

```bash
LMU_SHARED_MEMORY_SDK=C:/nonexistent cargo test --manifest-path src-tauri/Cargo.toml --lib
```

An explicit `LMU_SHARED_MEMORY_SDK` is authoritative in `build.rs`, so this
selects the mock instead of falling through to the drive scan.
