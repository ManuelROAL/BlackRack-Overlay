# Strategy analysis logging

## Purpose and ownership

Strategy logging is a user-facing, opt-in CSV export for comparing complete laps
and building external race-strategy simulations. It is independent of the JSONL
diagnostic/performance logger. Rust owns aggregation and file output in
`src-tauri/src/telemetry/strategy_log.rs`; the control panel only enables it and
shows the active filename inside a collapsed, expandable Strategy section.

The authoritative machine/AI interpretation contract is
[`STRATEGY_CSV_AI_GUIDE.md`](STRATEGY_CSV_AI_GUIDE.md).

Files are written under `%APPDATA%\BlackRack Overlay\strategy-logs` as UTF-8 CSV
with a semicolon delimiter for direct spreadsheet use. While enabled, the logger
opens and rotates one file per detected practice, qualifying or race session.
Track/vehicle identity changes and backwards session time also start a new file.
Disabling it flushes and closes the active file. The preference persists, but
strategy files remain excluded from configuration import/export.

## Recording boundary

- Sample the already-calculated 50 Hz player frame in memory and write exactly
  one row after a completed lap boundary.
- Skip the first partial lap after enabling, connecting or changing session.
- Keep invalid, non-green and pit laps, explicitly marked, because they still
  explain real resource balance and stint history.
- Flush every completed row so a game or application failure loses at most the
  current incomplete lap.
- Reset partial aggregation on disconnect, player inactivity, track/vehicle
  identity change, session-type change or backwards session time.
- Keep a temporary disconnect in the same file when session identity and elapsed
  time still agree, but skip the interrupted partial lap.

## CSV fields

Each row includes stable English column names and:

- identity: timestamp, circuit, vehicle, session type, lap and stint;
- lap context: official lap time, validity, all-green state and pit-lap marker;
- fuel and virtual energy: start, reconstructed end, consumption and additions;
- tyres: change detection, compound per wheel, start/end remaining tread, wear
  per wheel, average temperature and maximum temperature;
- conditions: average ambient/track temperature, rain, wetness and grip;
- aggregate damage at lap start and end.

Wheel order is FL, FR, RL, RR. Numeric values use invariant decimal points so
the file remains machine-readable across application locales.

## Verification

Run Rust tests and the frontend build. In LMU, enable recording before a complete
lap and verify normal, invalid, pit/refill and tyre-change laps. Confirm consumed
fuel/energy includes additions, tyre replacement cannot produce negative wear,
and a reconnect does not emit a partial row.
