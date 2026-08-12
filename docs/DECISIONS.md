# Product and engineering decisions

This file records decisions that are easy to lose when only reading individual
source files. New work should preserve them unless the user explicitly changes a
requirement.

## Product

- Primary platform is Windows. Linux remains a desired target, with live Proton
  telemetry still to be validated.
- Overlays remain independently selectable and configurable, but Tauri groups
  them into one transparent host WebView per monitor to avoid one Chromium
  renderer per overlay.
- Information density and legibility while driving take priority over decoration.
- All overlay content stays visible when resized; resizing scales rather than
  clipping or scrolling.
- Game mode is click-through. Escape is not intercepted to hide overlays because
  doing so interfered with LMU's pause menu and controls.
- Every application start begins in game mode; edit mode remains an explicit,
  temporary choice through the control-panel button or global shortcut.
- Overlay positions, sizes, monitor assignments and user choices persist.
- Monitor edges may intentionally crop an overlay so the user can expose only the
  desired rows or columns. Keep at least a small draggable strip on-screen so an
  off-edge panel remains recoverable in edit mode.
- Roboto Condensed, matching LMU's HUD typography, is bundled and used without
  requiring OS installation.
- Background transparency and monitor assignment can each use a general value or
  per-overlay values. Switching to a general value must preserve the individual
  choices so they can be restored later.
- Damage panels prioritize TinyPedal-like density: Damage + Tyres keeps all eight
  chassis locations distinct and gives every wheel a compact module for tyre,
  brake, suspension and flat-spot state, while detailed damage is restricted to
  four numeric percentage categories.
- Track Map prefers LMU's static official type-0 centerline and type-1 pitlane,
  loaded once and kept outside the 20 Hz telemetry frame. A complete valid player
  lap remains the local fallback, followed by circular progress. The player keeps
  its class-position label and is identified by a permanently visible disc plus
  a pulsing lime halo; it is never replaced with a `P` label.
- Telemetry cadence follows the type of information: direct driving inputs,
  tyres, fuel and active flags use 50 Hz; Relative, pit estimate and active
  Rejoin plus detailed damage use 20 Hz; Standings uses 10 Hz. Slow work must not
  be promoted to the 50 Hz path merely because a panel shares the same host.

## Standings

- Multiclass layout groups rows by category with official-game-like category
  colors (Hypercar red, not the old blue treatment).
- Default rows are 10 for the player class and 3 for other classes, now user
  configurable. Other classes can be hidden.
- The player-class selection preserves the top three and a window around the user.
- Column visibility is configurable and the background must adapt without seams or
  leftover width from hidden columns.
- Row data favors session-assigned number, first name plus one surname, image-based
  country/manufacturer, profile badge, DR/SR progress and estimated DR change.
- GAP/INT lap differences require a complete physical lap.
- Best timing uses green for personal best and purple for session fastest.
- Category headers show SOF and current/initial car counts.
- Pit column switches from stop count to entry+stop+out-lap elapsed time while the
  pit cycle is active.

## Fuel and endurance strategy

- The calculation belongs to the player, except total race laps are leader-driven.
- Virtual energy is the resource for regulated Hypercar/LMGT3; fuel is the fallback
  for other classes.
- Hypercar/LMGT3 strategy evaluates virtual energy and fuel in parallel and uses
  the resource requiring more pit stops.
- Never add an arbitrary extra lap. Use exact fractional-lap projections without
  subtracting a configured reserve.
- Qualifying consumption is the maximum plausible target and survives into race.
- Formation and pit use still affect actual balance, but do not contaminate clean
  average consumption.
- Learn pit-in/pit-out separately and include them in multi-stop strategy.
- Initial resource may be below 100% and must be respected.
- Pit-stop duration uses LMU's local REST `total` as authoritative. The service
  rows are informational and are never summed because services may overlap.

## Flags and rejoin

- Checkered is always highest priority.
- Standings always shows a likely yellow-causing car; dedicated flags only warns
  when the incident is relevant to the player and LMU confirms the sector yellow.
- Yellow includes front/rear direction.
- Rejoin follows TinyPedal-style traffic semantics: it is armed while the player
  is in the pit lane, below 8 m/s or has all four wheels on grass, dirt or gravel,
  and remains armed for ten seconds after recovery or pit exit.
- Rejoin only becomes visible when on-track traffic is approaching from behind
  within a 15-second physical time gap.
- Exact yellow causation is not exposed by the selected official shared-memory
  fields, so the implementation is a stabilized inference and should be described
  honestly.

## External services and third-party references

- Use LMU's own local auth ticket; do not embed a long-lived Nakama/server key.
- Cache resolved split for the session and stop requesting it once known.
- RaceOS client endpoints are treated as optional and potentially unstable.
- Resolve the online-event split through RaceOS `POST /api/v1/event/overview`
  with `game`, `eventType` and `eventId`. Prefer the authenticated user's
  `split` object, use the complete split roster only as supporting data and keep
  LMU local storage as fallback.
- TinyPedal is a functional/performance reference only. No GPL code is copied.
- Flat-spot wear uses an original implementation of TinyPedal's published semantics:
  accumulate actual tread loss only during braking lockups and reset after a tyre change.
- Detailed damage remains split by authoritative source: REST aero/suspension and
  shared-memory body severities; one category never substitutes for another.
- Standings damage is opponent-safe shared-memory integrity inverted to a percentage:
  dents plus 50% for detached bodywork or 100% for a detached wheel.
- Dox and Go Fast are design/behavior references only; no proprietary dependency
  is required or distributed.

## Distribution and diagnostics

- Users should install the application normally; they should not copy a custom DLL
  to LMU.
- Startup shortcut conflicts must be non-fatal and visible/configurable in the UI.
- `startup.log` is overwritten each run and is the first diagnostic for an app that
  closes immediately on another machine.
- Checksums accompany releases for integrity verification but are not required to
  run the installer.
- OBS browser source is optional and off by default to avoid idle resource use.
