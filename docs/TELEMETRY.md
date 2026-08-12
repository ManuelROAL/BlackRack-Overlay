# Telemetry and calculations

## Source precedence

Shared memory is the primary real-time source. Fresh local REST fields supplement
fields that are not reliably available in the shared-memory subset, such as the
assigned car number, pit status, weather and strategy estimates. GAP/INT and fuel
or virtual-energy consumption are calculated locally from the shared-memory stream
and driver history using TinyPedal-style semantics. RaceControl adds profile/event
metadata only. Every fallback must tolerate missing or stale data.

Vehicle identities are cached because names, vehicle model and class rarely
change. They must still update on driver swaps and reset at session boundaries.

## Track map

The official shared-memory roster supplies every active vehicle's world X/Z
coordinates. The bridge prefers the matched per-vehicle telemetry position over
the slower scoring position and normalizes LMU's lateral axis to `world_y = -mPos.z`
and emits a small map-only roster at 20 Hz while Track Map is requested. This path
does not resolve ranks, REST identities or timing history.

Track Map requests LMU's static `/rest/watch/trackmap` geometry once per circuit.
Type `0` supplies the ordered main centerline and type `1` the open pitlane. The
backend filters out the other point families and caches successful responses by
normalized circuit key; the geometry is transferred separately from the 20 Hz
vehicle roster. After the player moves a few metres outside the pits, the
frontend aligns direction and lap-distance origin against the shared-memory world
position. If REST is unavailable or the response fails validation, the frontend
still samples the player's world position every three metres and stores a path
only after a complete valid non-pit lap covers at least 88% of the official
length. The circular projection remains the last fallback.

The pit-out prediction reuses LMU's authoritative REST service total. Pitlane
traversal is learned independently per circuit from complete pit passages by any
visible vehicle: only intervals with meaningful lap-distance movement count, so
stationary service time is excluded. When official type `1` geometry is
available, a sample is accepted only if it progresses from one pitlane endpoint
to the other; activating the overlay during a partial passage cannot seed the
estimate. The last seven valid observations are stored locally and their median
is used. The prediction remains hidden until traversal time and a valid player
lap pace are both available.

## Driver inputs and assists

The 50 Hz shared-memory snapshot uses the player's unfiltered throttle and brake
for the pedal traces. LMU's explicit `mTCActive` and `mABSActive` fields are the
authoritative intervention signals; do not infer them from filtered pedal values,
which may also reflect the pit limiter, rev limiter or other vehicle controls.
Trailing + Pedal plots TC at the throttle-trace height and ABS at the brake-trace
height for each sampled intervention.

Force Feedback uses LMU's signed `FFBTorque` value, normalized to `[-1, 1]` for
the bipolar overlay bar. Steering angle uses `mUnfilteredSteering` multiplied by
half the physical steering-wheel range; the visual range is used as fallback when
the physical range is unavailable.

## Damage and tyres

The player tyre frame exposes all four wheels in LMU order (front-left,
front-right, rear-left, rear-right). Temperature uses LMU's
`mTireCarcassTemperature`, matching the value shown by the native tyre HUD, and is
converted from Kelvin to Celsius; brake temperature uses the wheel's Celsius
`mBrakeTemp` value. LMU's observed `mWear` semantics are remaining
tread (`1.0` new, `0.0` exhausted), so the percentage is `mWear * 100` rather than
its inverse. Flat-spot percentage is an estimate accumulated from tread loss while
braking with a per-wheel slip ratio below `-0.3`; it resets on a tyre change in pits
or a new session, following TinyPedal's documented behavior without copying its code.
Compound type, flat and detached states come directly from each
`TelemWheelV01`. Vehicle damage uses the eight ordered `mDentSeverity` locations,
plus the detached-part signal and the existing aggregate damage percentage.
The Damage + Tyres frame exposes the four `wearables.suspension` values separately,
including a 100% override only for the corresponding detached wheel. The detailed
damage overlay keeps the categories independent: aerodynamic damage
comes from `wearables.body.aero` and suspension damage from the maximum value in
`wearables.suspension` at `/rest/garage/UIScreen/RepairAndRefuel`; bodywork damage
continues to use `sum(mDentSeverity) / 16`. The REST values are optional and shown
as unavailable when stale.

Standings cannot obtain opponent aero or suspension wear from the player-only REST
endpoint. Its damage column therefore shows the inverse of shared-memory vehicle
integrity: `sum(mDentSeverity) / 16`, plus 50 percentage points for a detached body
part and 100 for a detached wheel, clamped to 100%. This preserves a comparable
opponent value without applying the player's private REST data to other cars.

## Pit-stop estimate

`/rest/strategy/pitstop-estimate` supplies the predicted service duration. Its
`total` field is authoritative and must be displayed directly: fuel or virtual
energy, tyres, repairs, driver swap and penalties may overlap, so adding their
individual durations produces an incorrect result. Repairs combine damage, brakes
and brake ducts; penalties remain a separate optional row. The resource row uses
`ve` for virtual-energy cars and `fuel` otherwise. All values become unavailable
when the REST estimate is missing or stale.

## Fuel and virtual energy

- Hypercar and LMGT3 use virtual energy when the SDK reports that resource.
- LMP2, LMP3 and other non-regulated classes use fuel litres.
- Virtual-energy classes still calculate fuel in parallel; the race strategy uses
  whichever resource requires more pit stops.
- All driver consumption calculations refer to the local player. Only total race
  lap estimation follows the leader.
- Autonomy is remaining resource divided by the selected consumption reference
  and may contain decimals.
- Calculations use fractional lap distance and do not add a synthetic safety lap.
- The race may start below 100% energy; the actual initial resource is the basis.
- Resource added in the pits is tracked so lap consumption remains correct.

Three references are shown:

1. Average: clean race-lap average.
2. Qualifying: consumption associated with the fastest valid official qualifying
   lap; it survives into the race.
3. Last: consumption of the latest completed lap.

Average and last reset on a new session. Qualifying survives qualifying phases
into the race, but is cleared on a new practice/event. Formation laps, invalid
laps, neutralized laps and pit laps do not contaminate the clean average. Their
actual resource use still affects current balance and strategy.

Pit-in and pit-out consumption are learned separately and persisted per vehicle
and track. Multi-stop strategy adjusts required resource using those profiles.
Garage exit must not be learned as a race pit-out lap.

The automatic target-consumption calculation follows TinyPedal semantics and does
not subtract a configured reserve. It must never exceed the qualifying consumption
reference when that reference is available; reaching that cap means the driver can
run flat out until the stop.

## Remaining and total race laps

For timed races, project the leader's remaining crossings and then determine how
many finish-line crossings remain for the player. For fixed-lap races, use the
official target. Preserve physical progress around the finish-line transition:
do not display a whole lap difference until a complete physical lap separates
the cars.

Standings also receives a separate TinyPedal-style visual estimate. Fixed-lap
races subtract the player's physical lap progress from the official remaining
lap count. Timed races divide remaining time by the player's stabilized lap pace,
round up to the finish-line crossing after the timer expires and subtract current
lap progress. Combined laps-and-time sessions use whichever finish criterion is
projected to occur first. The pace starts from best, last or LMU estimated lap
time and then follows valid non-pit laps with a six-sample exponential average,
immediately accepting a faster lap and limiting a single slower-lap increase to
five seconds. This display estimate is intentionally independent from the
leader-aware crossings used by fuel and energy strategy.

## Standings timing

- `GAP` is calculated relative to the class leader from continuous shared-memory
  lap progress; `INT` is calculated relative to the preceding car in class.
- If LMU temporarily reports zero seconds on the same lap, reconstruct seconds
  from continuous lap progress rather than showing a false `0.0`.
- Only show `±1 V` after a complete physical lap, not merely because one car has
  crossed the finish line first.
- Lap times are displayed to three decimals. Gap/interval currently use one
  decimal in the overlay.
- Personal best is green; session/race fastest lap is purple.
- A completed invalid lap remains visible in gray. When LMU reports zero in
  `mLastLapTime`, its time is reconstructed from consecutive telemetry
  `mLapStartET` values. As in TinyPedal, a changed lap start is accepted only
  after the new lap has been active for more than one second, avoiding a mixed
  scoring/telemetry sample at the finish-line transition. LMU can also publish a
  stable partial-lap reset after a garage or vehicle restart; reconstructed times
  below 50% of the official best or estimated pace are discarded, while plausible
  invalid laps, including unusually slow ones, remain visible.
- `AVG 5` follows TinyPedal rather than excluding every invalid lap: it keeps
  the latest five completed times, rejects any time below the official valid
  best and excludes laps slower than 120% of the best plausible recent lap.
  Consequently, a plausible invalid lap can contribute to the average. Clean
  validity remains mandatory for consumption and strategy learning.
- `OUT` replaces last-lap time during the complete pit-out lap.

Relative time is independent of race or class position. It follows TinyPedal's
circular timing semantics using LMU's estimated time into lap and the player's
estimated lap time. Every opponent has a possible occurrence ahead and another
behind; the nearest configured rows are selected from both lists. Negative values
are ahead and positive values are behind. Completed lap count is ignored so lapped
and multiclass traffic is still ordered by actual track proximity in time. Cars
reported in the garage are excluded from both relative lists.

Relative's track-limit counter comes directly from the player's
`mTrackLimitsSteps` and the session's `mTrackLimitsStepsPerPenalty`; it is not a
car-position or outstanding-penalty count. LMU exposes four internal steps per
displayed game point, so Standings and Relative divide both values by four: one
raw step is shown as `0.25` and a raw penalty threshold of 16 as 4 points.

Each Standings vehicle is also matched by slot ID against LMU's all-vehicle
telemetry array. Its optional `track_limits_steps` value comes directly from that
vehicle's `mTrackLimitsSteps`; a missing telemetry match is serialized as
unavailable rather than being reported as zero. The overlay applies the same
four-steps-per-point display conversion to every driver.

The pit column normally shows completed stops. From pit entry through the end of
the out lap it shows elapsed pit-cycle time with a distinct background. An active
pit request has a green state when the timer is not being shown.

## DR/SR and SOF

RaceControl profile enrichment supplies DR, DR progress, underlying ELO when
available, SR, nationality and badge. Profiles are fetched asynchronously and
cached across session changes. During practice, new driver names are queried as
they appear. Qualifying and race sessions make at most one roster request and
reuse cached profiles immediately while refreshing them once for the new session;
this matters because DR changes after every rated result. The current DR gain/loss estimate models qualifying
and race head-to-head results against same-class opponents. It converts the visible
DR tier/progress to the internal scale and applies the event-specific `Base`, `K`,
`D` and `Log` settings returned by RaceOS when available, with observed defaults
otherwise. It is an estimate, not an official live value.

When analysis logging is enabled, races emit one `driver_rank_estimate_sample`
per second. The sample records the resolved event parameters, same-class profile
coverage, player positions, aggregate head-to-head terms and final estimated
gain so a discrepancy can be reproduced without logging authentication data.

Category SOF is calculated from resolved continuous DR values and indicates
partial coverage when not every profile is available.

Authentication flow:

1. GET LMU local `http://127.0.0.1:6397/rest/profile/getAuthSessionTicket`.
2. POST the short-lived ticket to `https://raceos.gg/authenticate`.
3. Cache the returned access token for five minutes in memory only.

Never persist or log tickets/tokens. Do not add the historical Nakama server key
or any key copied from another overlay.

The current event split and total split count are resolved from the latest online
event ID found in LMU's trace and RaceOS `event/overview` data. The resolver posts
the same `game`, `eventType` and `eventId` request shape observed in Dox and scans large
trace files backwards so an event join cannot be missed when it falls outside the
first or last trace chunk. LMU's `lmu.cs.registeredEvents` local-storage state is a
fallback when RaceOS is temporarily unavailable. Requests stop only after both the
current split and total count are known. A lightweight periodic trace check detects
when the user joins another event without restarting LMUOverlay; only an event-ID
change triggers a new RaceOS overview request.

## Flags

Checkered flag has highest priority. Blue is based on the player's flag state and
the nearest plausible faster/lapping car behind.

The official shared-memory subset does not expose the exact internal
`mYellowSeverity` causation signal. Yellow causation is reconstructed:

- Standings uses the TinyPedal-style preventive slow-car rule (below 8 m/s) and
  always shows the likely culprit, independent of the player's proximity.
- The dedicated flag overlay additionally requires an LMU sector yellow and only
  appears for a relevant nearby incident: up to 500 m ahead, otherwise up to
  50 m behind.
- Candidate evidence is stabilized for about one second to avoid marking normal
  braking as an incident.
- Cars in the pit lane are excluded.
- The overlay shows direction, distance, class position and category.

## Rejoin

Rejoin follows TinyPedal's default traffic thresholds. It is armed while the
player is in the pit lane, travels below 8 m/s, or has all four wheels on a
surface reported by LMU as grass, dirt or gravel. The armed state persists for
ten seconds after the player exits the pits or recovers.

The warning is visible only when an on-track, active car is physically behind
within a 15-second relative gap. Among those candidates, prefer the car with the
earliest positive arrival time; if none is closing, use the smallest relative
gap. Instantaneous closing speed supplies arrival time. A non-closing car is
always safe; otherwise distance and arrival time determine safe, caution or
danger.

## Local REST endpoints currently consumed

- `/rest/watch/standings` at 1 Hz, primarily for the room-assigned car number,
  qualification and supplementary pit/finish fields.
- `/rest/strategy/pitstop-estimate` at 1 Hz for service-time estimates that are
  not exposed by the shared-memory subset.
- `/rest/garage/UIScreen/RepairAndRefuel` at 1 Hz for aerodynamic and per-wheel
  suspension damage.
- `/rest/profile/getAuthSessionTicket` only when RaceControl authentication needs
  a new token.
- Weather, tire wear, GAP/INT, fuel/energy state and consumption are read or
  calculated locally from shared memory.

Collections and observed endpoints for manual inspection are under `postman/`.
These endpoints are part of LMU/RaceOS client behavior and may change without
notice; do not make the core overlay depend on uninterrupted availability.
The live endpoint and cost comparison from August 12, 2026 is recorded in
`postman/ENDPOINT-AUDIT-2026-08-12.md`. Static `/rest/watch/trackmap` geometry is
adopted as a fetch-once source for Track Map; `/rest/strategy/usage` remains an
unadopted per-lap history candidate. Neither replaces the shared-memory vehicle
path or provides a current-energy fallback.
Observed response fields and context-dependent gaps are preserved separately in
`postman/OBSERVED-RESPONSE-FIELDS-2026-08-12.md`; that file intentionally omits
driver values, account identifiers, tickets and access tokens.
