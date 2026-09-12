# Rivals overlay

## Scope

The Rivals overlay is a frontend artifact that reads the existing
`TelemetryFrame.standings` contract. It keeps the player row and displays the
nearest active cars physically ahead and behind in the current session.

## Selection and displayed data

- Cars in the garage are excluded. The three nearest cars ahead are selected
  from finite negative `relative_ahead_seconds` values, and the three nearest
  behind from finite positive `relative_behind_seconds` values.
- Rows show driver, physical gap, completed lap, last-lap/average pace, and
  compact pit, penalty, and damage signals. The session header identifies the
  current session and lap.
- The empty state is used until the current frame contains a player row.

The physical-nearest selection is intentionally based on relative timing, not
race position, class, or standings order. It is useful for traffic and
stint-style situational awareness in practice, qualifying, and races.

Pit signals use the available `in_pits`, `pit_stop_requested`, `pit_stops`,
`pit_stop_lap`, and `pit_stop_time_seconds` fields. `pit_stop_time_seconds` is
the observed pit duration; it is not an exact stationary box-service time.
`penalty_count` is a generic penalty counter, not an incident taxonomy.
Incidents are not available in this contract and are not inferred or displayed.

## Limitations

The overlay has no additional session history, class filtering, incident feed,
or exact pit-box timing. It depends on the current standings frame and may show
the waiting state during roster refreshes or while no player row is available.
