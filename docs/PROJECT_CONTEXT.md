# Project context

## Product goal

LMUOverlay provides compact, readable, configurable overlays for Le Mans
Ultimate, with special attention to multiclass endurance racing and virtual
energy management. The application should remain useful while driving, avoid
capturing mouse input in game mode and consume as few resources as practical.

The design is original. TinyPedal, Dox and Go Fast have been used to compare
behavior, information density and performance, but this project does not depend
on their runtime or source code.

## Current feature set

- Tauri control panel that selects independent overlay panels hosted together in
  one transparent WebView per monitor.
- Dashboard for core driving telemetry.
- iRacing-style delta bar with overall, session, stint and last-lap references,
  backed by persistent lap/stint records.
- Configurable multiclass standings.
- Endurance fuel/virtual-energy calculator.
- Yellow, blue and checkered flag overlay.
- Safe rejoin and pit-exit warning.
- Optional localhost browser source for OBS.
- Configurable global shortcuts and click-through game mode.
- Background transparency selectable as one general value or per overlay.
- Monitor assignment selectable globally or per overlay, preserving individual
  choices when temporarily using one common monitor.
- Optional telemetry/performance analysis logging.
- Compact tyre/damage schematic plus an independent detailed damage overlay.
- Compact pit-stop estimate with LMU's official total and service breakdown.
- Circuit map with live multiclass vehicle positions and a learned per-track path.
- Startup diagnostics for failures on other computers.

## Current data sources

1. Official LMU shared memory (`LMU_Data`) through the C++ bridge. This is the
   primary real-time source and must remain authoritative for critical telemetry.
2. LMU local REST service at `127.0.0.1:6397`, used for fields that are more
   reliable or only exposed there (standings supplement, weather and strategy).
3. RaceControl/RaceOS client endpoints, authenticated with LMU's local session
   ticket, for DR/SR profiles and the online-event split.
4. Mock telemetry when the Windows SDK is unavailable at compile time.

All optional REST/network integrations must degrade gracefully. Losing them may
remove enrichment such as DR/SR or split data, but must not stop the overlays.

## Current release and distribution

- Version is defined in `package.json`, `src-tauri/Cargo.toml` and
  `src-tauri/tauri.conf.json`; keep all three synchronized.
- Windows distribution uses a current-user NSIS installer and embeds the WebView2
  bootstrapper.
- Web assets, icons, logos, flags, badges and Roboto Condensed are embedded in
  the executable through Tauri's `frontendDist`; the installer does not deploy a
  separate `web/` directory.
- Production JavaScript is minified with Terser using moderate top-level name
  mangling, no source maps and no comments. DevTools are disabled in the control
  panel and every dynamically created overlay host.
- Rust release builds use fat LTO, one code-generation unit and strip symbols
  from the final binaries. Debug and development profiles remain unchanged.
- Users do not need to copy a project DLL into LMU. The application expects LMU's
  own `Plugins/LMU_SharedMemoryMapPlugin64.dll` and reports whether it was found.
- Release artifacts and checksums live under `release/<version>/`.

## Current optimization state

The hot telemetry source runs at 50 Hz. Full standings construction has been
removed from every source cycle: it is requested at 10 Hz for Standings or a local
browser-source client, and at 20 Hz while the more time-sensitive Relative window
is visible.
Dashboard, Trailing + Pedal, tyres, fuel and active flags consume the 50 Hz raw
snapshot. Delta also calculates and renders from that 50 Hz snapshot. Rejoin,
Relative and detailed damage run at 20 Hz, while standings
history/identity state is maintained at 10 Hz.

The backend optimizations are compiled and covered by the Rust regression suite,
but still need a new real-race/replay performance capture to quantify the
reduction. See `docs/PERFORMANCE.md` and `docs/TODO.md`.

Tauri creates one transparent full-monitor host WebView per detected display.
Active overlays are mounted inside the assigned host and removed when disabled,
so renderer count scales with monitors instead of overlay count. Layout is
migrated from the former native-window geometry on first use.

## Repository map

```text
*.html                         Vite overlay, control and composite-host entries
src/                           TypeScript and CSS overlays/control panel
src/assets/                    Country flags, badges and manufacturer logos
src-tauri/src/lib.rs           Tauri windows, commands, shortcuts and lifecycle
src-tauri/src/browser_source.rs Local HTTP/SSE source for OBS
src-tauri/src/telemetry/       Telemetry, REST, ranks and calculations
src-tauri/src/telemetry/lmu_bridge.cpp Official shared-memory adapter
docs/overlays/                 Per-overlay behavior, telemetry and ownership
postman/                       Local LMU and RaceOS request collections
tools/performance/             LMUOverlay/TinyPedal comparison tooling
release/                       Generated release artifacts
```
