# Shared telemetry and integrations

This document describes infrastructure shared by several overlays. Detailed
calculation, selection and presentation semantics live in the relevant file under
`docs/overlays/`. The contract a simulator implements, and what it takes to add
one, live in `docs/SIMULATORS.md`.

## Source precedence

Within Le Mans Ultimate, the only simulator implemented today:

1. Official LMU shared memory (`LMU_Data`) is authoritative for real-time and
   safety-critical telemetry.
2. Fresh LMU local REST data supplements fields that are absent or more reliable
   there, including assigned number, qualification, pit estimates and wearables.
3. RaceControl/RaceOS adds optional DR/SR, profile and online-event metadata.
4. The mock source supports builds without the Windows SDK. It is also the
   standing second implementation of the source contract, so the seam stays
   honest without a second simulator.

Every optional source must tolerate missing or stale data. Losing REST or RaceOS
may remove enrichment but must not stop overlays or shared-memory telemetry.
Network and REST work runs outside `next_frame()`.

Lift & Coast presentation consumes the official shared-memory
`mLiftAndCoastProgress` byte directly. It must not reconstruct the cue from track
position, pedal behavior or learned braking points.

## Scheduling and payloads

The source snapshot runs at 50 Hz. Rust schedules each consumer according to its
documented cadence in `docs/overlays/README.md`:

- Overlay-specific view models and strategy/warning calculations run only while
  their native panel or matching OBS route has demand. Lap traces, consumption
  learning and the shared player/session state continue at source cadence so an
  overlay can be enabled without losing completed-lap history.

- Base overlays, including Delta and Timing, share one serialized frame
  per cycle. Distance interpolation, reference selection and the player's
  official scoring-sector transitions are calculated in Rust before delivery.
- Standings and Relative share an enriched roster batch when due; coincident
  cycles reuse the constructed roster.
- Track Map receives a stripped coordinate-only batch and never requests enriched
  standings.
- Browser-source SSE clients declare their overlay route. Only Standings and
  Relative routes request the enriched roster; legacy clients without a route
  retain full demand for compatibility.

Native delivery uses filtered `telemetry://batch` events. Each monitor host
forwards only locally mounted named targets and projects the frame through the
reused allowlist in `src/composite.ts` before same-origin `postMessage`. Do not
restore per-overlay native listeners or direct cross-realm object events.

## Identity and session state

- Cache static vehicle identity but refresh it on driver swaps and clear
  session-scoped state at session boundaries.
- In spectator mode, if LMU no longer exposes a locally controlled vehicle, use
  the fresh `focus`/`hasFocus` identity from local REST standings and match its
  normalized driver name to shared-memory scoring before selecting the telemetry
  vehicle. Never equate REST `slotID` with shared-memory `mID` without that
  identity match.
- Team mode is separate from spectator mode. Validate the local REST `teamInfo`
  roster against `sessionInfo.playerName`, then match its drivers, team or vehicle
  identity to shared-memory scoring. This keeps the registered team car selected
  regardless of the active spectator camera. The two modes are mutually exclusive.
- Shared-memory vehicle `mID` and REST `slotID` are different namespaces. Match
  cross-source vehicles by normalized driver identity; accept a numeric slot
  fallback only when identity agrees.
- The in-game time of day comes from shared memory's `mTimeOfDay` seconds since
  midnight; presentation formats it as a locale-aware clock without applying the
  computer's timezone.
- Do not replace fresh shared-memory timing with REST history. History is a
  low-frequency late-start recovery source only where an overlay document
  explicitly permits it.
- Preserve unavailable values as unavailable rather than converting them to real
  zeroes.
- Derive lap validity from telemetry's per-vehicle `mLapInvalidated` signal,
  latched for the complete lap. A negative official `mLastLapTime` confirms an
  invalid completed lap. LMU can publish the sentinel `-1` instead of the invalid
  duration; when consecutive `mLapStartET` values reconstruct that completed lap,
  treat the missing official result as invalid. Do not use scoring's `mCountLapFlag`: it also represents
  laps that are uncounted or not timed and is not a track-limit validity signal.
- At the timing line, suppress a stale near-finish scoring distance while the
  telemetry lap counter and new-lap timer have already advanced. Consumers must
  not observe that one-frame source disagreement as a backwards lap jump.
- The player's authoritative lap distance comes from scoring. Between its lower
  cadence updates, advance that distance with telemetry speed and the telemetry
  lap clock, then reconcile small scoring corrections progressively. This keeps
  reconstructed 50 Hz deltas continuous without allowing integration drift.
- Timing uses LMU's official current-sector partials; it does not derive
  sector duration from the asynchronously updated scoring-sector transition.
- Delta's session-best mode consumes telemetry's native `mDeltaBest`; custom
  overall, optimal, stint and last-lap modes retain reconstructed traces.
- Persistent lap references use track, generic vehicle model and rounded track
  length as their identity; learned consumption uses track and that same generic
  model. Neither fragments history by team/livery entry name. Lap references also
  check and migrate the current livery-specific legacy key. They are loaded
  asynchronously and merged with any newer in-memory result rather than replacing
  it.

## Local REST endpoints

The REST workers have independent demand gates. Standings/history poll only for
Standings, Relative, spectator identity or DR logging; the bundled supplement
polls only for consumers of session, strategy, steering, damage or pit data; and
weather polls only for Forecast or Conditions. Disconnecting LMU disables all three.

The supplement worker carries a second condition: the player must have a
vehicle, which shared memory reports as `player_active`. Being connected is not
enough, because the plugin reports that from the main menu, where no car exists
and every `/rest/garage/` endpoint is being asked about one that is not there.
That is not a tidiness rule — see the warning below.

- `/rest/watch/sessionInfo`: 1 Hz for the official configured session `maxTime`.
- `/rest/sessions/weather`: 1 Hz for the three five-slot session forecasts;
  BlackRack selects PRACTICE, QUALIFY or RACE according to shared-memory session
  type and consumes sky, temperature, rain chance and humidity.
- `/rest/watch/standings`: 1 Hz for assigned number, qualification and
  supplementary pit/finish fields.
- `/rest/watch/standings/history`: 0.2 Hz for permitted late-start recovery.
- `/rest/watch/trackmap`: fetch once per circuit for static type-0/type-1 geometry.
- `/rest/strategy/pitstop-estimate`: 1 Hz for authoritative service estimates.
- `/rest/garage/getPlayerGarageData`: 0.2 Hz for the active steering-wheel range
  in `VM_STEER_LOCK`; shared memory can report the nominal vehicle range instead.
- `/rest/garage/UIScreen/TireManagement`: every 30 s for
  `optimalCompoundConditions`, the compounds the player's car carries and their
  optimal temperatures. The list order is what `mCompoundType` indexes, so it
  resolves both the compound letter and the temperature scale for a car that
  does not carry the full soft/medium/hard/wet ladder — an LMP2 running Medium
  and Wet indexes them 0 and 1, which the fixed ladder reads as Soft and Medium.
  It describes the player's car only; rivals keep the generic ladder.
- `/rest/garage/UIScreen/RepairAndRefuel`: 1 Hz for aero wearables, per-wheel
  suspension damage, assigned fuel ratio and the absolute fuel/virtual-energy
  load selected in the official pit menu. The latest successful wearable
  response remains latched across transient request failures until disconnect or
  session change; strategy menu values expire independently when the response
  becomes stale.

**`/rest/garage/getVehicleCondition` must not be requested.** 0.7.6 polled it at
5 Hz because its ~259 B carry the same per-corner suspension as the ~11 KB
response above, and it closed the game. Two traces from one user show the same
sequence twice: LMU idle at the main menu, the overlay starts, and within a
second the supplement worker's first burst goes out. Its siblings answered a
404 in a millisecond each — the HTTP server was healthy — while
`getVehicleCondition` never answered at all, and the process was gone by the
next two requests. It is not a screen route that can be absent like
`UIScreen/*`; it is routed, and with no vehicle loaded it goes looking for one.
Suspension comes from the wearables instead, which is what 0.7.5 did.

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

## Request failure diagnostics

Every HTTP request the backend makes — the local REST endpoints above and the
RaceOS calls below — reports its outcome to the session diagnostics log through
`startup_log::record_request_failure` and `record_request_success`. Only
transitions are written, so a poll running several times a second cannot flood
the file:

- `request_failed endpoint=… error=…` the first time an endpoint stops
  answering. For local REST the error names the stage that broke — `request`
  (connection or timeout), `status` (HTTP error) or `decode` (the response no
  longer matches the parsed shape); RaceOS reports its own stable error code.
- `request_still_failing endpoint=… consecutive=N error=…` at most every 30 s
  while the outage lasts.
- `request_recovered endpoint=… after_failures=N` when it answers again.

Failure detail is redacted the same way panics and frontend errors are, because
the RaceOS requests carry bearer tokens and session tickets. `trackmap` keeps
its own `track_geometry_*` lines from before this mechanism existed.

A silent failure here is expensive: LMU dropped the whole `RepairAndRefuel`
payload on every poll for one unparsable field, and with the error discarded the
overlay simply showed an undamaged car.

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

An attempt that leaves the split unresolved for a known event ID doubles the
retry interval, from ten seconds up to five minutes; the first retry keeps the
normal interval, so one failed request costs nothing. A resolved split or a new
event ID resets it, and being outside an online event is not a failure. RaceOS
answering `500` for a stale event ID must never become a poll that runs at full
cadence for the rest of the session.

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
- The independent DR-estimate JSONL records the player's current DR when it
  becomes available or changes during practice and qualifying, plus changes to
  the derived estimate during a race. Practice and qualifying samples omit
  positions and calculations. Every resolved player sample includes RaceControl's
  raw continuous ELO so rank promotion resets cannot be mistaken for exact gains.
  Values are compared at the precision useful for diagnosis, so floating-point
  noise and unchanged periodic samples are omitted independently for each event
  type. Versioned estimate samples contain the anonymous per-opponent inputs
  needed to replay every pairwise calculation. A race sequence links those
  samples to its final prediction and to a later settlement after a fresh
  RaceControl refresh; that settlement records estimated gain, actual gain,
  signed error and absolute error.
  The raw ELO comes first from the authenticated RaceControl player profile and
  falls back to the authenticated registration in `event/overview` when present;
  roster profiles remain the source for rank, tier and visible progress.
  Preserve a returned zero as raw evidence: RaceControl currently masks ELO as
  zero in some practice contexts, while registered event overview may expose it.
  A zero raw ELO is not valid settlement evidence; use the refreshed continuous
  visual score as the fallback ground truth.
  Enabling it requests the 10 Hz standings
  model even when Standings is hidden; it never records telemetry frames,
  performance samples or complete third-party responses. All sessions in one
  application run append to its unique
  `dr-estimate-<timestamp>-<pid>.jsonl`; toggling the logger continues that file,
  and later application launches retain every earlier run.
- Keep bounded/non-blocking delivery for browser clients and skip serialization
  when no client exists.
- Add focused Rust tests for shared telemetry semantics, especially session reset,
  identity changes, lap transitions, pits, gaps, flags and consumption learning.
