# Dashboard overlay

## Scope and files

- Entry: `dashboard.html`
- Renderer/style: `src/dashboard.ts`, `src/dashboard.css`
- Shared types/runtime: `src/telemetry-types.ts`, `src/runtime-events.ts`
- OBS route: `/dashboard`
- Cadence: 50 Hz base telemetry

The Dashboard presents the player's core driving telemetry: speed, gear, RPM,
pedals, lap timing/delta and current resource information. It consumes the raw
base frame and must not request an enriched Standings roster.

## Invariants

- Keep the high-frequency renderer lightweight: cache hot DOM references and skip
  unchanged text, class and inline-style writes.
- Do not add CSS transitions to values updated at 50 Hz. They keep WebView2's
  compositor active between telemetry frames.
- Use the shared compact visual language, but preserve a hierarchy suitable for
  immediate driving inputs rather than copying table-specific decoration.
- Any field added to the renderer must be mirrored in `src/telemetry-types.ts` and
  the Dashboard projection allowlist in `src/composite.ts`.
- Keep the browser-source page and preferences synchronized when behavior becomes
  configurable.

## Verification focus

- Check rapid gear, RPM, pedal and delta updates for stale or flickering values.
- Confirm the overlay receives the base batch at 50 Hz without triggering full
  standings construction.
- Run `npm.cmd run build`; run Rust tests too when serialized fields change.
