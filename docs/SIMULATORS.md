# Simulators

The app reads one simulator at a time through a single contract. Everything that
knows a simulator exists lives under `src-tauri/src/telemetry/sim/<id>/`; every
other module — the frame, the 50 Hz loop, the domain models and the whole
frontend — is simulator agnostic and must stay that way.

Le Mans Ultimate is the only shipped simulator. When its shared-memory SDK is
not available, the mock source keeps the control panel and overlays usable.

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

`detect` returns a source that keeps looking. It builds LMU when its `available`
probe answers yes, falls back to the mock, and then on every cycle where the
frame comes back disconnected — or where the mock is active, since it is always
"connected" — it re-probes at most once every two seconds and adopts LMU when it
becomes available.

That is what makes launching the app before the game work. A connected source is
never displaced, and when nothing is available the current source is kept, so a
waiting simulator keeps reporting itself instead of falling back to the mock.

An `available` probe must be cheap and must answer about *this machine, right
now*. LMU answers whether the build has the SDK, because its bridge cannot be
asked more cheaply than that.

Every automatic path — the first pick, the two-second re-probe and the return to
auto — draws from `available_candidate`, the single function that answers which
simulator selection adopts right now. Keeping that decision centralized makes
future additions follow the same picker and preference rules.

Each genuine switch is written to the session diagnostics log as
`telemetry source <previous> -> <next>`, with `none` as the first previous. Only
transitions appear, because the selection paths re-adopt the source they already
had. It is the one record of which simulator the overlays were actually reading,
which is what a report of "the overlays went empty" needs to be answerable.

`try_new` is a different question from `available` and must not assume it was
already checked: it answers whether this build can ever construct the source at
all (wrong OS, no SDK), not whether the simulator is running. LMU's `try_new`
succeeds on a capable build; the source it returns handles "not running yet"
itself, per frame, the same way it handles a mid-session disconnect. That gap is
what lets a pinned preference construct a source before the simulator has even
started, so it reports its waiting state instead of a fabricated mock frame.

## Pinning a preference

The control panel's header can carry a compact select next to the connection
pill, with Auto plus one option per `CANDIDATES` entry, labelled from
`sim::options()` — never a literal name in TypeScript or the catalogs, the same
rule visible copy already follows. With only LMU shipped, the picker is hidden.
`set_simulator_preference` (`sim::set_preference`) stores the choice as an
index into `CANDIDATES`, with `CANDIDATES.len()` standing for "auto", and bumps
a generation counter. `SelectedSource::next_frame` compares that counter every
cycle and reacts on the one cycle it changes: a pin builds the chosen
candidate's own source regardless of `available`, and returning to Auto lets
the normal priority order pick. This keeps a future pinned source from being
silently overridden by auto-reselection while it is temporarily closed.

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
  configuration document will need simulator namespacing if another simulator
  is added. The migration is a per-simulator prefix on the `localStorage` keys
  in `src/main.ts` plus a `simulator` field in the configuration document with a
  `schemaVersion` bump.
- **Learned data is keyed by track and car, not by simulator.** The delta
  records, the learned track outline and the consumption profiles share one
  store. A future simulator could collide with LMU on a track name and mix the
  learned entries.
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
