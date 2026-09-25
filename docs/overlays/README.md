# Overlay documentation index

Read `docs/OVERLAYS.md` for shared host and visual contracts, then only the file
for the overlay being changed. Each overlay file is the authoritative home for its
UI behavior, telemetry semantics, cadence and special invariants.

| Overlay | Document | Entry | OBS route | Normal cadence |
| --- | --- | --- | --- | --- |
| Delta / lap records | [delta.md](delta.md) | `delta.html` | `/delta` | 50 Hz |
| Timing | [timing.md](timing.md) | `timing.html` | `/timing` | 50 Hz |
| Stint history | [stinthistory.md](stinthistory.md) | `stinthistory.html` | `/stinthistory` | 4 Hz |
| Standings | [standings.md](standings.md) | `standings.html` | `/standings` | 10 Hz |
| Relative | [relative.md](relative.md) | `relative.html` | `/relative` | 20 Hz |
| Fuel / energy | [fuel.md](fuel.md) | `fuel.html` | `/fuel` | 50 Hz |
| Trailing + Pedal | [driving.md](driving.md) | `driving.html` | `/driving` | 50 Hz input, 25 Hz canvas |
| Lift & Coast | [liftcoast.md](liftcoast.md) | `liftcoast.html` | `/liftcoast` | 50 Hz |
| Damage + Tyres | [tires.md](tires.md) | `tires.html` | `/tires` | 50 Hz |
| Detailed Damage | [damage.md](damage.md) | `damage.html` | `/damage` | 20 Hz |
| Pit-stop estimate | [pitstop.md](pitstop.md) | `pitstop.html` | `/pitstop` | 20 Hz |
| Track Map | [trackmap.md](trackmap.md) | `trackmap.html` | `/trackmap` | approximately 30 Hz |
| Flags | [flags.md](flags.md) | `flags.html` | `/flags` | 50 Hz active, 4 Hz inactive |
| Rejoin | [rejoin.md](rejoin.md) | `rejoin.html` | `/rejoin` | 20 Hz active, 4 Hz inactive |
| Weather Forecast | [forecast.md](forecast.md) | `forecast.html` | `/forecast` | 2 Hz |
| Current conditions | [conditions.md](conditions.md) | `conditions.html` | `/conditions` | 20/12.5/~8 Hz by profile |
| Dashboard | [dashboard.md](dashboard.md) | `dashboard.html` | `/dashboard` | 50 Hz |
| Session Info | [sessioninfo.md](sessioninfo.md) | `sessioninfo.html` | `/sessioninfo` | secondary cadence (25/12.5/~8 Hz) |
| In-game chat | [chat.md](chat.md) | `chat.html` | `/chat` | 0.5 Hz while requested |

`src/overlay-appearance.ts`'s `OverlayId` is the roster every other surface has to
match. `npm run check:overlays` reads each of them back and reports drift: this
index and the per-overlay documents, the HTML entries and renderers, the control
panel and OBS route lists, the Vite `input` map, `OVERLAY_LABELS` in `lib.rs`,
`BROWSER_OVERLAYS` in `browser_source.rs`, `overlayIds` plus `telemetryFields`
in `src/composite.ts` and `overlayIds` in `src/composite-layout.ts`. It also
checks that every `<id>://settings` event an overlay listens for is on the
composite host's forwarding list. Adding or renaming an overlay means updating
all of them.

Cross-cutting ownership remains in:

- [Architecture](../ARCHITECTURE.md): runtime, hosts, persistence and build behavior.
- [Telemetry](../TELEMETRY.md): shared source precedence, identity and integrations.
- [Performance](../PERFORMANCE.md): measurement procedure and current target.
