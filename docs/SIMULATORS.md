# Simulators

The app reads one simulator at a time through a single contract. Everything that
knows a simulator exists lives under `src-tauri/src/telemetry/sim/<id>/`; every
other module — the frame, the 50 Hz loop, the domain models and the whole
frontend — is simulator agnostic and must stay that way.

Le Mans Ultimate is the only simulator implemented. This document is the contract
a second one has to satisfy.

## The contract

`src-tauri/src/telemetry/sim/mod.rs` owns four things:

| Item | Purpose |
| --- | --- |
| `TelemetrySource` | `descriptor()` plus `next_frame(demand)`, called from the loop thread |
| `SourceDescriptor` | id, display name, capabilities, and two optional function pointers |
| `SourceCapabilities` | what the simulator can *ever* report |
| `detect(app_data)` | tries each simulator in order and falls back to the mock |

The descriptor carries function pointers rather than source methods for the two
things the app asks for outside the loop thread:

- `official_geometry` — the track outline, requested by the `get_track_map_geometry`
  command and by the browser-source server.
- `dependency` — whether the simulator's plugin, SDK or service is installed,
  requested by `get_simulator_status` for the control panel.

A source that has neither leaves both `None`; `track_geometry.rs` then falls back
to the outline learned from laps, and the panel simply does not report a
dependency.

## Capabilities are not availability

Two different questions share the frame and must not be confused:

- **Capability** — can this simulator *ever* report it? Fixed per simulator,
  declared in the descriptor, delivered as `frame.capabilities`. The control
  panel uses it to retire an overlay that would never have data.
- **Availability** — is it there *in this session*? Already carried by the frame
  as `virtual_energy_active`, `rest_weather_available` and the `-1` sentinels.
  The renderers use it to hide a value that is missing right now.

Only four overlays are gated by capability, because each exists for one of them:
`damage` (`damage_detail`), `forecast` (`weather_forecast`), `liftcoast`
(`lift_and_coast`) and `pitstop` (`pit_service_estimate`). Every other overlay
degrades inside its own renderer.

## Adding a simulator

1. Create `src-tauri/src/telemetry/sim/<id>/mod.rs` with a `DESCRIPTOR` and a
   `try_new(app_data) -> Option<Box<dyn TelemetrySource>>` probe returning `None`
   when the simulator cannot be read in this build or on this machine.
2. Implement `TelemetrySource` for it. Fill the frame with the semantics the
   agnostic modules already expect; leave what the simulator does not report at
   the sentinel values the frame documents, and turn the matching capability off.
3. Declare the capability set honestly. A capability that is on but never filled
   is worse than one that is off: the panel will offer an overlay that stays empty.
4. Add the module to `sim/mod.rs` and the probe to the `detect` chain, ahead of
   the mock. Order is priority.
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
  configuration document are not namespaced per simulator, so switching would
  carry one simulator's layout into another. The migration is a per-simulator
  prefix on the `localStorage` keys in `src/main.ts` plus a `simulator` field in
  the configuration document with a `schemaVersion` bump; do it when the second
  simulator lands, not before.
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
