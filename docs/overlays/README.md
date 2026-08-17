# Overlay documentation index

Read `docs/OVERLAYS.md` for shared host and visual contracts, then only the file
for the overlay being changed. Each overlay file is the authoritative home for its
UI behavior, telemetry semantics, cadence and special invariants.

| Overlay | Document | Entry | OBS route | Normal cadence |
| --- | --- | --- | --- | --- |
| Dashboard | [dashboard.md](dashboard.md) | `dashboard.html` | `/dashboard` | 50 Hz |
| Delta / lap records | [delta.md](delta.md) | `delta.html` | `/delta` | 50 Hz |
| Timing compacto | [timing.md](timing.md) | `timing.html` | `/timing` | 50 Hz |
| Standings | [standings.md](standings.md) | `standings.html` | `/standings` | 10 Hz |
| Relative | [relative.md](relative.md) | `relative.html` | `/relative` | 20 Hz |
| Fuel / energy | [fuel.md](fuel.md) | `fuel.html` | `/fuel` | 50 Hz |
| Trailing + Pedal | [driving.md](driving.md) | `driving.html` | `/driving` | 50 Hz input, 25 Hz canvas |
| Damage + Tyres | [tires.md](tires.md) | `tires.html` | `/tires` | 50 Hz |
| Detailed Damage | [damage.md](damage.md) | `damage.html` | `/damage` | 20 Hz |
| Pit-stop estimate | [pitstop.md](pitstop.md) | `pitstop.html` | `/pitstop` | 20 Hz |
| Track Map | [trackmap.md](trackmap.md) | `trackmap.html` | `/trackmap` | approximately 30 Hz |
| Flags | [flags.md](flags.md) | `flags.html` | `/flags` | 50 Hz active, 4 Hz inactive |
| Rejoin | [rejoin.md](rejoin.md) | `rejoin.html` | `/rejoin` | 20 Hz active, 4 Hz inactive |

Cross-cutting ownership remains in:

- [Architecture](../ARCHITECTURE.md): runtime, hosts, persistence and build behavior.
- [Telemetry](../TELEMETRY.md): shared source precedence, identity and integrations.
- [Performance](../PERFORMANCE.md): measurement procedure and current target.
