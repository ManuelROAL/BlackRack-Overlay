# Delta and lap records

## Purpose

Delta provides an iRacing-style signed time comparison while also establishing
the Rust-owned lap and stint record used by future overlays. The numeric value is
green while ahead of the reference and red while behind it. Independently, the
bar is green when the driver is gaining time over the recent circuit segment,
red when losing time and grey when stable. The bar grows right for gains and left
for losses, with its configured range clamped visually while the numeric value
remains unclamped.

The presentation contains only a transparent horizontal timing rail and the
signed delta centred immediately below it. The visible numeral height matches
the eight-pixel coloured bar so the timing rail retains the primary visual
weight. The coloured bar grows away from the centre without a marker or gap
between the two halves. Mode, mini-sector and reference details remain
configuration/backend concerns and are not rendered.

## Files and ownership

- `delta.html`, `src/delta.ts`, `src/delta.css`: presentation.
- `src/delta-settings.ts`: mode and visual-range persistence.
- `src-tauri/src/telemetry/delta_records.rs`: lap reconstruction, reference
  selection, delta calculation, stint aggregation and storage.
- `src/composite.ts`, `src/composite-layout.ts`: shared-host projection and layout.
- `src-tauri/src/browser_source.rs`: `/delta` OBS route and mirrored preferences.

Rust owns all timing semantics. TypeScript only formats the supplied view model
and scales the bar. The normal native cadence is 50 Hz; the optional browser
source follows the existing shared SSE publication cadence.

## Modes

- Off.
- Overall best lap.
- Overall optimal lap: accumulated optimal mini-sectors.
- Overall optimal sectors: the same bank, resetting the displayed delta at each
  mini-sector boundary.
- Session best lap.
- Session optimal lap.
- Session optimal sectors.
- Stint best lap.
- Last eligible completed lap.

Session best uses LMU telemetry's native `mDeltaBest` while an official session
best exists. This matches the in-game delta's reference, sign and high-frequency
update directly, avoiding the sawtooth produced by interpolating against the
lower-cadence scoring distance. The other modes continue to use BlackRack's
reconstructed and persisted traces because LMU does not expose equivalent native
deltas for those references.

No live delta is exposed during an outlap that BlackRack did not observe from
the timing line. This also prevents LMU's stale pre-line lap-start timestamp from
appearing as a multi-minute current lap before the first timed crossing.

Overall references are keyed by normalized track name, vehicle name and rounded
track length. Session references reset when the shared session identity changes
or elapsed session time moves backwards. Stint references use LMU's player stint
counter. A reference becomes available after the first reconstructable eligible
lap in its scope.

## Sampling and calculation

- A lap trace starts only when the overlay observes the car near the timing line.
- Reconstructed modes advance scoring lap distance at telemetry cadence using
  the car speed and lap clock, then progressively reconcile each authoritative
  scoring update. This removes the roughly 5 Hz sawtooth without accumulating
  integration drift. Mid-lap starts and outlaps remain unavailable until the
  next observed timing-line crossing.
- Samples are retained at approximately 5 metre distance intervals and linearly
  interpolated at the player's current lap distance.
- Optimal references split the circuit into roughly 250 metre mini-sectors,
  bounded to 12-40 sectors, and retain the fastest trace for each sector.
- The rendered value uses a 100 ms exponential response to reduce shared-memory
  jitter without adding a long perceptible delay.
- Bar trend compares the delta at 20 metre distance intervals. A change below
  8 ms is neutral; larger decreases are improving and larger increases are
  worsening. The trend resets across laps, references, modes and reset-style
  mini-sector boundaries so discontinuities cannot produce a false colour.
- Bar position and width ease toward each 50 Hz telemetry target over 180 ms so
  shared-memory noise does not become visible lateral oscillation. This visual
  easing does not filter or delay the numeric delta. Neutral trend is white.
- An invalid live lap uses amber rather than presenting its segment trend as
  actionable feedback.
- At the timing line the Delta overlay resets immediately to zero and begins
  showing the new lap once the car has moved beyond the first sample. It never
  freezes the completed lap's final delta. Timing retains its separate
  result freeze.

A completed trace is accepted only with at least ten samples, an official lap
time between 20 and 900 seconds, and a reconstructed final sample within 0.5
seconds of LMU's official last-lap value. Only green, non-pit, non-formation laps
can improve best or optimal references. Invalid, pit and formation laps may still
be written to history when their trace is reconstructable. A backwards distance
jump greater than 200 metres invalidates the candidate lap.

LMU's telemetry lap counter and scoring result can cross the timing line on
different source updates. The completed trace therefore remains pending until
the scoring completed-lap counter advances and supplies that lap's official
time. Validity latches telemetry's `mLapInvalidated` over the complete lap; a
negative official time confirms the completed lap as invalid after normalization.
`mCountLapFlag` is not a validity input. This state determines the history
category and reference eligibility.

Immediately after the telemetry lap counter advances, scoring can briefly expose
the previous lap's near-finish distance again while the new lap timer is already
running. That stale distance is held at the timing line until the sources agree,
preventing a false backwards jump from invalidating the trace. Delta smoothing
also restarts at each telemetry lap boundary so the previous lap cannot create a
large transient on the new lap.

The compact timing sector slots reset when the telemetry lap counter advances.
The just-completed lap can repopulate them for the documented three-second
result freeze once its official scoring result arrives; after that freeze, each
slot is filled only by the corresponding crossing of the current lap. LMU's
sector sequence is `1` (S1), `2` (S2), `0` (S3).

S1 and the cumulative S1+S2 boundary come from LMU's official scoring partials
(`mCurSector1` and `mCurSector2`) rather than sampling the telemetry clock when
the scoring sector index changes. S2 is their difference and S3 is the official
lap time minus the cumulative S1+S2 value. The short residual invalid flag at the
timing line is ignored until scoring has synchronized with the new lap; any
invalid state observed after that synchronization remains latched.

## Persistence

`lap-records.sqlite3` lives in the application data directory. `rusqlite` embeds
SQLite in the executable, so the user does not install SQLite separately. The
database uses WAL mode and `synchronous=NORMAL` and contains:

- `delta_references`: serialized overall best/optimal trace for each identity.
- `sessions`: session boundaries and identity.
- `laps`: reconstructable completed laps, validity category and resource use.
- `stints`: aggregate lap count, best/total time and resource use.

The telemetry thread performs no database queries or writes. It updates an
in-memory model; a dedicated storage worker loads references and writes only at
lap, stint and session boundaries. Learned records and history are deliberately
excluded from configuration import/export and configuration reset.

## Focused verification

- `cargo test --manifest-path src-tauri\Cargo.toml --lib` covers interpolation,
  sector sizing, optimal-sector composition and SQLite reference round trips.
- `npm.cmd run build` covers the standalone, composite and OBS entries.
- Live LMU validation should cover a clean lap, invalid lap, pit passage, session
  reset, stint transition, vehicle/track change and application restart.

## Localization

The document title and accessibility text use the bundled locale. Delta values,
reference IDs and lap-time notation remain stable telemetry presentation. Its OBS
route follows the shared saved locale or a page-local `?lang=` override.
