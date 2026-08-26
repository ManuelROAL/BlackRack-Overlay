BlackRack Overlay 0.7.1 - Windows x64
=====================================

Independent telemetry overlay application for Le Mans Ultimate.
SimHub is not required.

Installation
------------
1. Run "BlackRack Overlay_0.7.1_x64-setup.exe".
2. Start Le Mans Ultimate in windowed or borderless-windowed mode.
3. Open BlackRack Overlay and enable the panels you want to use.

Main features
-------------
- 14 configurable overlays: Delta, Compact Timing, Standings, Relative,
  Fuel / Virtual Energy, Trailing + Pedal, Damage + Tyres, Detailed Damage,
  Pit-stop Estimate, Track Map, Flags, Safe Rejoin, Weather Forecast and
  Current Conditions.
- Multiclass standings and nearby-car information enriched with DR/SR data when
  RaceControl makes it available.
- Persistent lap, stint, fuel and Virtual Energy references.
- One transparent overlay host designed to reduce renderer overhead.
- Click-through game mode and a proportional edit mode.
- Optional localhost-only browser sources for OBS.
- Spanish and English interface.
- Import and export of a single versioned configuration file.

Version 0.7.1 highlights
------------------------
- Improved class-header alignment, car counts and helmet visibility in Standings.
- Refined suspension glyph proportions in Damage + Tyres.

Requirements
------------
- 64-bit Windows 10 or Windows 11.
- Le Mans Ultimate installed through Steam.
- LMU's official shared-memory plugin at:
  Le Mans Ultimate\Plugins\LMU_SharedMemoryMapPlugin64.dll
- Microsoft Edge WebView2 Runtime (the installer includes its bootstrapper).

No Node.js, Rust, Tauri or SimHub installation is required. Web assets, icons,
flags, badges and fonts are embedded in the application.

Controls and diagnostics
------------------------
- Ctrl+Shift+O toggles edit/game mode by default.
- Ctrl+Shift+M shows and focuses the control panel by default.
- Startup diagnostics are written to:
  %APPDATA%\BlackRack Overlay\startup.log

Privacy and network access
--------------------------
Shared memory is the primary telemetry source. Optional localhost LMU and
RaceControl requests enrich some fields and fail safely when unavailable.
Authentication tickets and access tokens are never logged or persisted.
The optional OBS server listens only on 127.0.0.1 and is disabled by default.

SmartScreen notice
------------------
This installer is not digitally signed. Windows SmartScreen may display a
warning the first time it is run. Verify the SHA-256 value in SHA256SUMS.txt if
you want to confirm the downloaded file.

Support the project
-------------------
BlackRack Overlay is free. Optional support: https://ko-fi.com/blackrack
