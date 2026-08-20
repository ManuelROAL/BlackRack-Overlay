# Delta and lap records

## Purpose

Delta provides an iRacing-style signed time comparison while also establishing
the Rust-owned lap and stint record used by future overlays. Faster is negative
and green; slower is positive and red. The bar grows right for gains and left for
losses, with its configured range clamped visually while the numeric value remains
unclamped.

The presentation contains only a transparent horizontal timing rail and the
signed delta centred immediately below it. The zero marker remains fixed in the
centre and the coloured bar grows away from it. Mode, mini-sector and reference
details remain configuration/backend concerns and are not rendered.

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

Overall references are keyed by normalized track name, vehicle name and rounded
track length. Session references reset when the shared session identity changes
or elapsed session time moves backwards. Stint references use LMU's player stint
counter. A reference becomes available after the first reconstructable eligible
lap in its scope.

## Sampling and calculation

- A lap trace starts only when the overlay observes the car near the timing line.
- Samples are retained at approximately 5 metre distance intervals and linearly
  interpolated at the player's current lap distance.
- Optimal references split the circuit into roughly 250 metre mini-sectors,
  bounded to 12-40 sectors, and retain the fastest trace for each sector.
- The rendered value uses a 100 ms exponential response to reduce shared-memory
  jitter without adding a long perceptible delay.
- At a completed lap the final result freezes for 3 seconds.

A completed trace is accepted only with at least ten samples, an official lap
time between 20 and 900 seconds, and a reconstructed final sample within 0.5
seconds of LMU's official last-lap value. Only green, non-pit, non-formation laps
can improve best or optimal references. Invalid, pit and formation laps may still
be written to history when their trace is reconstructable. A backwards distance
jump greater than 200 metres invalidates the candidate lap.

LMU's telemetry lap counter and scoring result can cross the timing line on
different source updates. The completed trace therefore remains pending until
the scoring completed-lap counter advances and supplies that lap's official
time. Signed official times are normalized to their duration while the observed
validity state continues to determine the history category and reference
eligibility.

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
