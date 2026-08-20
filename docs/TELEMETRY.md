# Shared telemetry and integrations

This document describes infrastructure shared by several overlays. Detailed
calculation, selection and presentation semantics live in the relevant file under
`docs/overlays/`.

## Source precedence

1. Official LMU shared memory (`LMU_Data`) is authoritative for real-time and
   safety-critical telemetry.
2. Fresh LMU local REST data supplements fields that are absent or more reliable
   there, including assigned number, qualification, pit estimates and wearables.
3. RaceControl/RaceOS adds optional DR/SR, profile and online-event metadata.
4. The mock source supports builds without the Windows SDK.

Every optional source must tolerate missing or stale data. Losing REST or RaceOS
may remove enrichment but must not stop overlays or shared-memory telemetry.
Network and REST work runs outside `next_frame()`.

## Scheduling and payloads

The source snapshot runs at 50 Hz. Rust schedules each consumer according to its
documented cadence in `docs/overlays/README.md`:

- Base overlays, including Delta and Timing compacto, share one serialized frame
  per cycle. Distance interpolation, reference selection and the player's
  official scoring-sector transitions are calculated in Rust before delivery.
- Standings and Relative share an enriched roster batch when due; coincident
  cycles reuse the constructed roster.
- Track Map receives a stripped coordinate-only batch and never requests enriched
  standings.

Native delivery uses filtered `telemetry://batch` events. Each monitor host
forwards only locally mounted named targets and projects the frame through the
reused allowlist in `src/composite.ts` before same-origin `postMessage`. Do not
restore per-overlay native listeners or direct cross-realm object events.

## Identity and session state

- Cache static vehicle identity but refresh it on driver swaps and clear
  session-scoped state at session boundaries.
- Shared-memory vehicle `mID` and REST `slotID` are different namespaces. Match
  cross-source vehicles by normalized driver identity; accept a numeric slot
  fallback only when identity agrees.
- Do not replace fresh shared-memory timing with REST history. History is a
  low-frequency late-start recovery source only where an overlay document
  explicitly permits it.
- Preserve unavailable values as unavailable rather than converting them to real
  zeroes.
- At the timing line, suppress a stale near-finish scoring distance while the
  telemetry lap counter and new-lap timer have already advanced. Consumers must
  not observe that one-frame source disagreement as a backwards lap jump.
- The player's authoritative lap distance comes from scoring. Between its lower
  cadence updates, advance that distance with telemetry speed and the telemetry
  lap clock, then reconcile small scoring corrections progressively. This keeps
  reconstructed 50 Hz deltas continuous without allowing integration drift.
- Compact Timing uses LMU's official current-sector partials; it does not derive
  sector duration from the asynchronously updated scoring-sector transition.
- Delta's session-best mode consumes telemetry's native `mDeltaBest`; custom
  overall, optimal, stint and last-lap modes retain reconstructed traces.
- Persistent lap references use track, vehicle and rounded track length as their
  identity. They are loaded asynchronously and merged with any newer in-memory
  result rather than replacing it.

## Local REST endpoints

- `/rest/watch/sessionInfo`: 1 Hz for the official configured session `maxTime`.
- `/rest/watch/standings`: 1 Hz for assigned number, qualification and
  supplementary pit/finish fields.
- `/rest/watch/standings/history`: 0.2 Hz for permitted late-start recovery.
- `/rest/watch/trackmap`: fetch once per circuit for static type-0/type-1 geometry.
- `/rest/strategy/pitstop-estimate`: 1 Hz for authoritative service estimates.
- `/rest/garage/getPlayerGarageData`: 0.2 Hz for the active steering-wheel range
  in `VM_STEER_LOCK`; shared memory can report the nominal vehicle range instead.
- `/rest/garage/UIScreen/RepairAndRefuel`: 1 Hz for aero and per-wheel suspension
  wearables.
- `/rest/profile/getAuthSessionTicket`: only when RaceOS authentication needs a
  new token.

`/rest/strategy/usage` remains an unadopted per-lap history candidate. It does not
replace shared-memory state or provide a current-energy fallback.

Observed endpoint details are stored under `postman/`:

- `ENDPOINT-AUDIT-2026-08-12.md`
- `OBSERVED-RESPONSE-FIELDS-2026-08-12.md`
- `RACE-ENDPOINT-CAPTURE-2026-08-13.md`

These are observed client endpoints, not guaranteed public APIs. Parse responses
selectively and fail gracefully.

## RaceOS authentication and event split

Authentication flow:

1. Request LMU's local short-lived session ticket.
2. POST it to `https://raceos.gg/authenticate`.
3. Cache the returned access token in memory for five minutes.

Never log or persist tickets, tokens or complete responses containing participant
or server connection data. Do not add the historical Nakama key or any private
server key.

Resolve the online-event split through `POST /api/v1/event/overview` with the
`game`, `eventType` and `eventId` JSON object accepted by RaceOS. Prefer the
authenticated user's `split`, then use the direct read-only `event/my-split`
route, then LMU `lmu.cs.registeredEvents` local storage. Stop only when current
split and total count are known. A lightweight trace check may trigger a new
request only after the event ID changes.

Profile refreshes run asynchronously. During practice, request new identities as
they appear. During qualifying/race, an empty, failed or partial startup response
must leave unresolved identities retryable; do not mark the roster refresh
complete until a successful non-empty result covers it.

For registered events, load the complete `event/my-split` roster once per event
and retain only its non-sensitive driver profile enrichment. Its nationality and
badge may fill empty or `XX` data from `POST /players`, but never replace a valid
profile value. Server details and the full response remain neither logged nor
persisted.

## Logging and safety

- Optional JSONL analysis may record derived diagnostic fields and performance
  samples, never authentication material.
- Keep bounded/non-blocking delivery for browser clients and skip serialization
  when no client exists.
- Add focused Rust tests for shared telemetry semantics, especially session reset,
  identity changes, lap transitions, pits, gaps, flags and consumption learning.
