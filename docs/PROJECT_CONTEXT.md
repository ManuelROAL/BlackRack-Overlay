# Project context

## Product goal

BlackRack Overlay provides compact, readable, configurable overlays for Le Mans
Ultimate, with special attention to multiclass endurance racing and virtual
energy management. The application should remain useful while driving, avoid
capturing mouse input in game mode and consume as few resources as practical.

The design is original. TinyPedal, Dox and Go Fast have been used to compare
behavior, information density and performance, but this project does not depend
on their runtime or source code.

## Current feature set

- Tauri control panel that selects independent overlay panels hosted together in
  one transparent WebView per monitor that has visible overlays.
- Signed delta bar with overall, session, stint and last-lap references,
  backed by persistent lap/stint records.
- Timing panel with selectable lap-time rows, three-sector feedback
  and recent laps.
- Compact session stint history with laps, duration, resource use, tyre compound,
  wear, pace delta and consistency.
- Configurable multiclass standings.
- Endurance fuel/virtual-energy calculator.
- Yellow, blue and checkered flag overlay.
- Safe rejoin and pit-exit warning.
- Optional localhost browser source for OBS.
- Configurable global shortcuts and click-through game mode.
- Compact Lift & Coast lamps driven by LMU's official shared-memory progress.
- Background transparency selectable as one general value or per overlay.
- Text size selectable from 75% to 200% as one general value or per overlay.
- Visible overlays assigned to the same monitor share one host; hosts are created
  on demand only for monitors that currently have visible overlays.
- Optional telemetry/performance analysis logging.
- Optional per-lap strategy CSV with consumption, tyre wear and track conditions.
- Named overlay configuration profiles, with one bound to each of game,
  spectator and team mode and applied when the mode is selected. In game mode
  practice, qualifying and race can each take their own profile, applied when the
  session changes.
- Spectator mode keeps overlays attached to the car currently watched between driving stints.
- Team mode keeps overlays attached to the player's registered team car regardless of the spectator camera.
- Compact tyre/damage schematic plus an independent detailed damage overlay.
- Compact pit-stop estimate with LMU's official total and service breakdown.
- Circuit map with live multiclass vehicle positions and a learned per-track path.
- Weather forecast with the game's condition icons and a current-conditions panel
  for temperatures, wind, humidity, rain, grip and track state.
- Dashboard strip with the gear, the rev lights and freely selected readouts chosen
  from a flat list: speed, revs, class position, lap, session time, delta,
  last/best/predicted lap, fuel, virtual energy, range, hybrid battery, engine
  map, traction control, ABS, brake bias, lights, wipers and air/track
  temperature.
- Startup diagnostics for failures on other computers.

## Current data sources

1. Official LMU shared memory (`LMU_Data`) through the C++ bridge. This is the
   primary real-time source and must remain authoritative for critical telemetry.
2. LMU local REST service at `127.0.0.1:6397`, used for fields that are more
   reliable or only exposed there (standings supplement, weather and strategy).
3. RaceControl/RaceOS client endpoints, authenticated with LMU's local session
   ticket, for DR/SR profiles and the online-event split.
4. Mock telemetry when no simulator is available.

All optional REST/network integrations must degrade gracefully. Losing them may
remove enrichment such as DR/SR or split data, but must not stop the overlays.

## Current release and distribution

- The product is published as **BlackRack Overlay**, with bundle identifier
  `com.blackrack.overlay`; backend configuration, learned data and diagnostics
  live under `%APPDATA%\BlackRack Overlay` on Windows. The WebView2 user data
  profile (`EBWebView`) lives under `%LOCALAPPDATA%\BlackRackOverlay`, set
  explicitly and independently of the bundle identifier.
- Version is defined in `package.json`, `src-tauri/Cargo.toml` and
  `src-tauri/tauri.conf.json`; keep all three synchronized.
- Windows distribution uses a current-user NSIS installer and embeds the WebView2
  bootstrapper.
- The control panel checks a fixed HTTPS update manifest at startup and at most
  every six hours while it is open. A new installer is offered explicitly; Rust
  downloads it, verifies the manifest's SHA-256, and uses a detached helper mode
  to close the app before launching NSIS with update/relaunch flags. Installations
  are serialized across app instances and helper failures are surfaced after the
  next start. The check is optional and never participates in telemetry startup.
- Web assets, icons, logos, flags, badges and Roboto Condensed are embedded in
  the executable through Tauri's `frontendDist`; the installer does not deploy a
  separate `web/` directory.
- Production JavaScript is minified with Terser using moderate top-level name
  mangling, no source maps and no comments. DevTools are disabled in the control
  panel and every dynamically created overlay host.
- Rust release builds use fat LTO, one code-generation unit and strip symbols
  from the final binaries. Debug and development profiles remain unchanged.
- The application expects LMU's own `Plugins/LMU_SharedMemoryMapPlugin64.dll`
  for telemetry.
- Release artifacts and checksums live under `release/<version>/`.

## Current optimization state

The hot telemetry source runs at 50 Hz. Full standings construction has been
removed from every source cycle: it is requested at 10 Hz for Standings or its
route-specific browser client, and at 20 Hz while the more time-sensitive Relative
window is visible. Unrelated OBS routes no longer request that roster.
The shared snapshot, lap reconstruction and consumption learning remain at 50 Hz.
Delta/Timing view models, fuel scenarios and Flags/Rejoin scans now run only with
their native panel or matching OBS route active. Relative and detailed damage run
at 20 Hz, while standings history/identity state is maintained at 10 Hz.
Local REST standings, supplement and weather workers also stop independently when
their overlay/observer/logging consumers are absent.

The backend optimizations are compiled and covered by the Rust regression suite,
but still need a new real-race/replay performance capture to quantify the
reduction. See `docs/PERFORMANCE.md` and `docs/TODO.md`.

General settings provide Smooth, Balanced and Efficiency performance profiles.
They reduce overlay delivery/rendering cadence while keeping the complete Track
Map roster, telemetry source and critical calculations intact.

Tauri creates one transparent composite host WebView per active monitor. Active
overlays assigned to that monitor are mounted inside it, while hosts with no
visible overlays are closed. Moving one overlay therefore does not create a
renderer per panel, and putting every overlay on one monitor still uses one
composite host. Layout is migrated from the former native-window geometry on
first use.

## Repository map

```text
*.html                         Vite overlay, control and composite-host entries
src/                           TypeScript and CSS overlays/control panel
src/assets/                    Country flags, badges and manufacturer logos
src-tauri/src/lib.rs           Tauri windows, commands, shortcuts and lifecycle
src-tauri/src/browser_source.rs Local HTTP/SSE source for OBS
src-tauri/src/telemetry/       Simulator-agnostic frame, loop and calculations
src-tauri/src/telemetry/sim/   One module per simulator; see docs/SIMULATORS.md
src-tauri/src/telemetry/sim/lmu/bridge.cpp Official shared-memory adapter
docs/overlays/                 Per-overlay behavior, telemetry and ownership
postman/                       Local LMU and RaceOS request collections
tools/performance/             BlackRack Overlay/TinyPedal comparison tooling
release/                       Generated release artifacts
```
