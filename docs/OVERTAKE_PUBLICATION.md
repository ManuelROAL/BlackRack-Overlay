# OverTake publication

This document owns the copy and checklist for publishing BlackRack Overlay on
OverTake. Keep the version, included overlays and requirements synchronized with
the implementation and `release/<version>/README.txt`.

Reference structure: the doX LMU Overlays Collection listing. BlackRack's copy is
original and emphasizes that it is an independent application rather than a
SimHub package.

## Resource fields

- Title: `BlackRack Overlay`
- Version: `0.7.0`
- Category: `Le Mans Ultimate`
- Tag line: `Lightweight, configurable LMU telemetry overlays — no SimHub required.`
- Tags: `le mans ultimate`, `lmu`, `overlay`, `standings`, `relative`,
  `delta bar`, `fuel calculator`, `virtual energy`, `track map`, `obs`
- Download: `release/0.7.0/BlackRack Overlay_0.7.0_x64-setup.exe`
- Support URL: `https://ko-fi.com/blackrack`

## Overview copy

BlackRack Overlay is a free, independent telemetry overlay application for **Le
Mans Ultimate**, focused on multiclass endurance racing, clear information while
driving and low renderer overhead. **SimHub is not required.**

It reads LMU's official shared memory on Windows and uses optional local LMU and
RaceControl data only to enrich fields such as weather, strategy and DR/SR. If an
optional service is unavailable, the overlays keep running.

### Main overlays included

- Standings — configurable multiclass order, gaps, intervals, lap data, tyres,
  Virtual Energy, pit state, flags, penalties and optional DR/SR.
- Relative — nearby traffic with class position, intervals, tyres, lap data and
  driver information.
- Delta — live comparison against overall, session, stint or last-lap references,
  with persistent records.
- Compact Timing — current/last/best lap times, three-sector feedback and
  recent laps.
- Fuel / Virtual Energy — endurance strategy, consumption, autonomy, pit demand
  and achievable saving references.
- Trailing + Pedal — live inputs and driving traces.
- Damage + Tyres — compact tyre, brake and damage status.
- Detailed Damage — expanded vehicle and corner damage information.
- Pit-stop Estimate — LMU's official service total and repair breakdown.
- Track Map — learned circuit path with live multiclass vehicle positions.
- Flags — yellow, blue and checkered flag notifications.
- Safe Rejoin — approaching-car and pit-exit warnings.
- Weather Forecast — upcoming conditions using LMU's official weather icons.
- Current Conditions — temperatures, wind, humidity, rain, grip and track state.

### Key features

- One transparent host WebView for all enabled panels.
- Click-through game mode and proportional edit mode.
- Independent visibility, position, scale and transparency settings.
- Configurable global shortcuts.
- Spanish and English interface.
- Optional localhost-only OBS browser sources.
- Versioned configuration import/export.
- Bundled fonts, flags, badges and manufacturer logos; no web assets are required
  while driving.

### Installation

1. Download and run `BlackRack Overlay_0.7.0_x64-setup.exe`.
2. Start Le Mans Ultimate in windowed or borderless-windowed mode.
3. Open BlackRack Overlay and enable the panels you want to use.

No Node.js, Rust, Tauri or SimHub installation is required. LMU's own official
shared-memory plugin must be present in
`Le Mans Ultimate\Plugins\LMU_SharedMemoryMapPlugin64.dll`; users do not need to
copy a BlackRack DLL into the game.

### Requirements and notes

- 64-bit Windows 10 or Windows 11.
- Le Mans Ultimate installed through Steam.
- Microsoft Edge WebView2 Runtime; its bootstrapper is included.
- Some values depend on telemetry exposed by the current LMU version.
- The installer is not digitally signed, so Windows SmartScreen may warn on the
  first run. A SHA-256 checksum is included for verification.
- Live telemetry under Proton/Linux is not yet supported.

### Privacy and connectivity

The primary live source is LMU shared memory. Optional requests to LMU's local
service and RaceControl provide enrichment and degrade gracefully. Authentication
tickets and access tokens are never logged or persisted. The optional OBS server
is disabled by default and listens only on `127.0.0.1`.

BlackRack Overlay is free. If you find it useful, optional support is available at
https://ko-fi.com/blackrack.

## Version update copy

### 0.7.0 — Weather and telemetry refinements

- Added Weather Forecast and Current Conditions overlays with official LMU icons.
- Added tyre compound and pit-state telemetry refinements for Standings, Relative
  and Damage + Tyres.
- Improved Relative hot-path performance and compact Timing sector references.
- Moved WebView2 data to `%LOCALAPPDATA%\BlackRackOverlay`.
- Preserved the optional Ko-fi choice on the NSIS finish page.

## Decisions required before submission

Choose and insert the resource's usage terms. Do not inherit the doX listing's
license automatically. Typical options are:

1. Free for personal use; redistribution, reuploading, resale and modified
   distributions require prior permission.
2. A named open-source license, which requires adding the corresponding license
   file and ensuring bundled third-party assets are compatible.

Prepare at least one real in-game hero image before publishing. Recommended set:

1. Hero image: multiclass race with Standings, Relative, Delta and Track Map.
2. Strategy image: Fuel / Virtual Energy plus Pit-stop Estimate.
3. Safety and car-status image: Flags, Safe Rejoin and Damage + Tyres.
4. Control-panel image showing configuration, English language and edit mode.

## Final checklist

- [x] Version synchronized at `0.7.0` in npm, Cargo and Tauri.
- [x] Frontend production build and localization checks pass.
- [x] Rust library tests pass with the official LMU SDK detected.
- [x] Production NSIS installer generated under the BlackRack name.
- [x] SHA-256 generated and matched to the release installer.
- [ ] Comparable moving race/replay performance capture completed.
- [ ] Clean-machine installation and functional smoke test completed.
- [ ] Real in-game publication images selected.
- [ ] Usage terms selected and added to the listing/package.
- [ ] OverTake account signed in and resource form reviewed.
- [ ] Final resource submission explicitly confirmed.
