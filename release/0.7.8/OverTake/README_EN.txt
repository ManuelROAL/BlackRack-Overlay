BLACKRACK OVERLAY 0.7.8 - Windows x64
====================================
Free, configurable telemetry overlays for Le Mans Ultimate.
No SimHub, Node.js, Rust or Tauri installation is required.

INSTALLATION AND FIRST USE
1. Extract this ZIP and run BlackRack Overlay_0.7.8_x64-setup.exe.
2. Open the application. Select your language and overlay monitor in General.
3. In Overlays, enable the panels you need and customize their settings.
4. Enter Edit mode in General > Application to move and resize the panels.
5. Return to Game mode before driving so mouse input passes through.
6. Start Le Mans Ultimate and head out on track.

The installer includes the Microsoft WebView2 bootstrapper. Internet access
may be needed if WebView2 is not already installed.
Do not copy any BlackRack DLL into the game folder: telemetry uses LMU's own
Plugins/LMU_SharedMemoryMapPlugin64.dll.

WHAT'S NEW IN 0.7.8
- Dashboard: no eight-field limit, temporary car-adjustment notices, optional
  headlights and wipers, a hybrid-battery SOC icon, capability-aware fields,
  configurable pit warnings and stable three-digit speed sizing.
- Timing and Track Map: dynamic lap history and shared qualifying sector colours;
  the map also learns pit geometry and protects learned data during saving.
- Tyres and Damage: compounds from the mounted axle, game-matching surface
  colours and a real detached-rear-wing warning.
- Forecast: the single NOW column is centred.
- Fuel / Virtual Energy: independent visibility controls, completed-lap
  references, fuel and energy margins, fractional autonomy, parallel-resource
  planning, leader/class finish estimates and learned or calibrated pit travel.
- Visibility controls in Trailing + Pedal, Relative and Tyres now apply reliably,
  with new options enabled by default for existing installations.
- Standings and Relative: independent pit-information visibility and placement,
  plus shared player-distance and final-stop corrections.
- Stint History: hybrid SOC and regenerated energy, with correct fuel accounting
  when refuelling during a pit-stop lap. Lift & Coast is compact and supports
  active-only visibility saved in profiles and configuration backups.
- Spectator mode hides strategy-only panels while retaining preferences and no
  longer inherits the local car's garage state. Onboarding, diagnostics and
  Spanish Dashboard copy are improved.
- Flags, session reset, LMU restart, OBS/browser-source
  handling and optional REST data are more resilient to stale or slow data.

TIPS
- Open the built-in Guide for explanations of each overlay.
- Overlays may hide automatically in the garage or outside the game.
- Default shortcuts: Ctrl+Shift+O switches game/edit mode; Ctrl+Shift+M shows
  the control panel. Customize them in General > Application.
- Create your own profiles in General and assign them to driving, spectator
  or team mode, or to practice, qualifying and race sessions.
- Export your configuration in General > Backup before upgrading.
- Smooth, Balanced and Efficiency adjust the visual update cadence.
- The interface is available in English and Spanish.

OBS
Enable the local browser source in Integrations, then add the provided URLs
as Browser Sources in OBS on the same computer.

SUPPORT
Use Integrations > Support > Copy diagnostics. Include the summary with your
report on https://discord.gg/VdT6Wncxup, together with the steps to reproduce
the issue and a screenshot if useful. The summary excludes logs, personal
paths and credentials.

Data availability depends on the simulator and session. Some estimates need
completed laps to learn consumption or pace. Optional online information may
be unavailable without affecting the main shared-memory telemetry.

PACKAGE CONTENTS
Windows installer, English and Spanish guides, release highlights, OverTake
BBCode text and SHA256SUMS.txt with SHA-256 checksums for these files.

Optional project support: https://ko-fi.com/blackrack
Acknowledgments to https://www.twitch.tv/rastaracing
