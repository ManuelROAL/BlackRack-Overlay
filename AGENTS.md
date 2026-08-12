# LMUOverlay agent guide

## Project

LMUOverlay is a desktop telemetry overlay for Le Mans Ultimate. The current
release line is `0.2.x` (`0.2.0` at the time this file was written). It uses
Tauri 2, Rust, TypeScript, Vite and plain HTML/CSS without a frontend framework.

The live LMU telemetry implementation currently works on Windows. A mock source
is used when the official LMU shared-memory SDK is unavailable at build time.
Linux UI support exists, but live telemetry under Proton is still pending.

## Read before changing code

Always read:

- `docs/PROJECT_CONTEXT.md`
- `docs/DECISIONS.md`
- `docs/TODO.md`

Then read only the task-specific document:

- Backend, data flow or windows: `docs/ARCHITECTURE.md`
- Telemetry, RaceControl, REST, fuel, flags or timing: `docs/TELEMETRY.md`
- UI or overlay behavior: `docs/OVERLAYS.md`
- Profiling or optimization: `docs/PERFORMANCE.md`

Treat the implementation and tests as the final source of truth when a document
has become stale. Update the relevant document whenever behavior or architecture
changes materially. Update AGENTS.md.

## Important commands

Run from the repository root on Windows. Prefer `npm.cmd` because PowerShell may
block `npm.ps1` through its execution policy.

```powershell
npm.cmd install
npm.cmd run dev
npm.cmd run tauri dev
npm.cmd run build
cargo fmt --manifest-path src-tauri\Cargo.toml
cargo test --manifest-path src-tauri\Cargo.toml --lib
npm.cmd run tauri build
```

- `npm.cmd run dev` serves browser previews on `http://localhost:1420`.
- `npm.cmd run tauri dev` runs the complete desktop application.
- `npm.cmd run build` validates TypeScript and builds all HTML entries.
- `cargo test ... --lib` is the normal Rust regression suite.
- `npm.cmd run tauri build` creates the Windows NSIS installer. Do not generate
  an installer unless the user explicitly requests a release/build artifact.

## Working rules

- Preserve unrelated user changes; the working tree may be dirty.
- Use UTF-8 for all text files.
- Keep the frontend framework-free unless the user requests a migration.
- Keep each overlay's CSS in its own file. Shared font and base rules belong in
  `src/fonts.css` and `src/styles.css` only when genuinely shared.
- Keep the control panel's top-level separation between overlays, general settings
  and integrations. Visibility stays immediately accessible; per-overlay monitor,
  transparency and reset controls remain behind the compact settings disclosure.
- Keep every per-monitor host both `transparent(true)` and explicitly configured
  with a transparent RGBA background; release WebView2 builds must not rely on
  the transparent flag alone.
- Keep general and per-overlay modes available independently for transparency and
  monitor assignment. General mode must not discard the saved individual values.
- Keep per-overlay reset actions scoped: configuration restores only that panel's
  defaults, while position restores its default geometry on its assigned monitor.
  Neither reset changes visibility or another overlay. Use the styled in-panel
  confirmation dialog, not `window.confirm`, so no localhost origin is exposed.
- Do not set `color-scheme: dark` on an embedded overlay document root. WebView2
  can paint the unused iframe canvas instead of preserving transparency when a
  panel is resized to a different aspect ratio.
- Keep overlays grouped as panels in one host WebView per monitor. Explicit
  deactivation removes the panel document; LMU-driven automatic visibility uses
  hide/show on the host so transient state changes do not rebuild the UI.
- Start every run in click-through game mode; edit mode is entered explicitly
  through the control panel or its global shortcut.
- Keep embedded overlay content inert. In edit mode pointer input is reserved for
  moving panels and using their resize handles; do not expose WebView controls,
  text selection, native dragging or context menus inside overlay hosts.
- Allow composite panels to cross monitor edges for deliberate cropping, while
  retaining a small visible strip so they remain recoverable in edit mode.
- Standings and Relative must override shared `.overlay-shell` surface styles with
  greater selector specificity. Production Vite CSS extraction loads shared CSS
  after entry CSS, unlike the apparent development cascade.
- Keep the per-driver category accent and gradient visually aligned between
  Standings and Relative; player and pit-state layers must remain distinguishable.
  Standings additionally groups each class in an outlined card with separated row
  surfaces and a filled angled category tab. Relative uses the same framed header
  and separated row cards inside one neutral outline because physical order must
  not be split into category groups.
- Keep Relative's physical time value as the dominant row datum: it uses a larger,
  heavier treatment and a slightly wider column than secondary values.
- Keep Standings and Relative header visibility separate from their individual
  header-data toggles so users can preserve a compact bar with only the values
  they need.
- Keep the TinyPedal-style Standings remaining-lap display estimate separate from
  the leader-aware crossing count used by fuel and energy strategy.
- Keep both damage panels compact. Detailed damage reports percentages only,
  including tyre wear; localized body, suspension, brake and tyre SVGs belong to
  Damage + Tyres, alongside each wheel's temperature, tread, flat spot and compound.
- In Detailed Damage, tyre wear uses separate thresholds from component damage:
  normal below 30%, warning from 30%, heavy from 50% and critical from 75%.
- Keep Detailed Damage's label/value columns narrow and fixed; the percentage
  column reserves only enough width for `100%`. Its square compact width and lower
  host minimum are intentional—do not leave padding after the values.
- Keep `src/telemetry-types.ts` synchronized with serialized Rust frame fields.
- Keep Track Map on its lightweight 20 Hz coordinate roster. Do not make it
  request enriched Standings data. Load official type-0/type-1 REST geometry only
  once per circuit and outside telemetry frames. Preserve the circular fallback
  and require a complete valid non-pit lap before persisting fallback geometry.
- Track Map vehicle coordinates prefer matched per-vehicle telemetry over the
  slower scoring position. Preserve compositor-managed transform transitions;
  do not add a permanent JavaScript animation loop, per-marker SVG filters or a
  higher backend emission rate to hide visual stepping.
- Track Map identifies the player with a pulsing lime circle around the normal
  class-position marker; do not replace the label with `P`.
- Track Map identifies only the overall race leader with a static gold star;
  class leaders receive no separate mark. Keep the star distinct from the
  player's animated lime halo.
- Keep the OBS browser-source route catalog synchronized with every overlay entry,
  and mirror configurable overlay preferences needed by browser pages.
- Keep Damage + Tyres split into eight independent chassis zones and four wheel
  modules; suspension damage must use its matching per-wheel REST value rather
  than a nearby body-damage severity.
- Preserve the availability distinction for opponent track-limit steps: an
  unmatched vehicle telemetry slot is unavailable, not a real zero.
- Preserve TinyPedal's stabilization for invalid opponent lap times: reconstruct
  `LAST` from consecutive `mLapStartET` values only after the new lap has been
  active for more than one second, so asynchronous scoring/telemetry transitions
  cannot record a transient short lap. Discard stable partial-reset deltas below
  50% of the driver's official best or estimated pace; plausible invalid laps,
  including slow ones, must remain visible in gray.
- Keep Track Map's `P` separate from the player marker. Its prediction combines
  the authoritative REST service total with the median learned moving time through
  the pitlane; validate full passages against official type-1 geometry when it is
  available. Do not count stationary service time twice or invent a circuit-wide
  fallback before a complete pit passage has been observed.
- Do not use the player's fuel percentage as a Standings `NRG` fallback; that
  column is a comparable all-driver virtual-energy reading only.
- Prefer cached DOM nodes and update only changed text/classes in hot render paths.
- Do not move heavy work into the 50 Hz telemetry loop without measurements.
- Add focused tests for telemetry semantics, especially lap transitions, pits,
  gaps, flags, session resets and consumption learning.
- Never log authentication tickets, access tokens or other credentials.
- Do not add private server keys. RaceControl authentication must continue to use
  LMU's local, short-lived session ticket.
- RaceOS endpoints used here are observed client endpoints, not a guaranteed public
  API. Fail gracefully and keep shared-memory telemetry functional without them.
- Resolve the online-event split through RaceOS `POST /api/v1/event/overview`
  with the Dox-compatible `game`/`eventType`/`eventId` request. Keep LMU local
  storage as a fallback and prefer the authenticated user's `split` object over
  entries in the complete `splits` roster.
- TinyPedal may be used as behavioral reference, but do not copy GPL source into
  this project. Dox/Go Fast are visual or behavioral references only.
- The bundled manufacturer logos, country flags, badges and Roboto Condensed font
  must remain included in production builds and browser-source pages.

## Verification expectations

For Rust/backend changes:

1. `cargo fmt --manifest-path src-tauri\Cargo.toml`
2. `cargo test --manifest-path src-tauri\Cargo.toml --lib`
3. `npm.cmd run build` if serialized fields or frontend behavior changed

For frontend-only changes, run `npm.cmd run build`. For performance work, also
collect a comparable race/replay sample as described in `docs/PERFORMANCE.md`;
successful compilation alone does not prove an optimization.
