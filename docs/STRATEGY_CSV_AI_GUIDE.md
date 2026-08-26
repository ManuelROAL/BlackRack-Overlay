# BlackRack strategy CSV: AI interpretation guide

## Purpose

This document is the semantic contract for interpreting BlackRack Overlay strategy
CSV files. Each file represents one detected LMU session and each data row
represents one completed player lap. Use the CSV header as the authoritative
schema and tolerate additional columns in future versions.

The data is intended for stint comparison, fuel/virtual-energy planning, tyre
degradation analysis and simple race-strategy simulation. It is not raw physics
telemetry and must not be interpreted as a high-frequency driving trace.

## File format and lifecycle

- Encoding: UTF-8 with BOM.
- Delimiter: semicolon (`;`).
- Decimal separator: dot (`.`), independent of application locale.
- Boolean values: lowercase `true` or `false`.
- Missing text: empty string.
- Missing non-finite number: empty cell.
- Naming pattern:
  `blackrack-strategy-<unix-ms>-<track>-s<session_type>.csv`.
- A new file is opened when the track, player vehicle or session type changes, or
  when session elapsed time moves backwards enough to indicate a restart.
- A short telemetry disconnect does not rotate the file when the session identity
  still agrees, but its interrupted lap is discarded.
- The first partial lap after enabling, connecting or rotating is not written.
- Rows are flushed at every lap boundary.

Do not assume row numbers equal lap numbers. Sort primarily by `timestamp_ms` and
use `lap` as the LMU-provided lap identifier within the file.

## Session classification

`session_type` is the LMU/rFactor numeric session identifier:

| Range | Broad meaning |
| --- | --- |
| `0..4` | Test or practice |
| `5..8` | Qualifying |
| `9` | Warm-up or intermediate session |
| `10..13` | Race |

Preserve the exact integer. The range is sufficient for broad strategy grouping;
do not invent a more specific label unless another trusted source supplies it.

## Wheel convention

All wheel suffixes use this fixed order:

| Suffix | Wheel |
| --- | --- |
| `fl` | Front left |
| `fr` | Front right |
| `rl` | Rear left |
| `rr` | Rear right |

Never reorder wheels by vehicle layout or driving side.

## Column dictionary

### Identity and lap context

| Column | Type / unit | Meaning and interpretation |
| --- | --- | --- |
| `timestamp_ms` | integer, Unix ms | Wall-clock time when the completed row was written at the lap boundary. Convert as UTC Unix epoch; it is not lap duration. |
| `track` | string | Circuit name reported by LMU. Compare normalized strings only within deliberate cross-file grouping. |
| `vehicle` | string | Player vehicle name reported by LMU. Do not merge different vehicles when fitting consumption or degradation. |
| `session_type` | integer | Numeric session identifier described above. |
| `lap` | integer | LMU lap identifier for the lap that was sampled. Treat it as an ordering aid, not a guaranteed one-based consecutive index. |
| `stint` | non-negative integer | Application/LMU player stint counter. A change usually indicates a new stint, but also inspect pit and tyre columns. |
| `lap_time_s` | seconds | Official completed-lap time. Values `<= 0` are unavailable and must not enter pace models. |
| `valid` | boolean | `true` only if the lap remained valid throughout all observed samples. |
| `green` | boolean | `true` only if every observed sample was in normal green running (`game_phase == 5`). Formation, caution or other phases make it `false`. |
| `pit_lap` | boolean | `true` if the player was observed in pits or garage at any point during the lap. Includes pit-in/out context and should normally be excluded from clean-lap pace models. |

### Fuel and virtual energy

| Column | Type / unit | Meaning and interpretation |
| --- | --- | --- |
| `fuel_start_l` | litres | Fuel measured at the start of the sampled lap. |
| `fuel_end_l` | litres | Reconstructed end balance: `fuel_start_l + fuel_added_l - fuel_used_l`. |
| `fuel_used_l` | litres/lap | Net fuel consumed by the completed lap, corrected for detected refuelling. A value `<= 0` means unavailable/rejected for analysis, not proven zero consumption. |
| `fuel_added_l` | litres | Fuel added during the lap, normally on a pit lap. |
| `energy_start_pct` | percentage points | Virtual-energy balance at lap start. Fuel-only cars normally report zero. |
| `energy_end_pct` | percentage points | Reconstructed end balance: `energy_start_pct + energy_added_pct - energy_used_pct`. |
| `energy_used_pct` | percentage points/lap | Virtual energy consumed, corrected for replenishment. A value `<= 0` means unavailable or inactive. |
| `energy_added_pct` | percentage points | Virtual energy added during the lap, normally during service. |

Fuel consumption is accepted by the source only in a plausible per-lap range;
virtual energy is likewise validated. Therefore zero must be treated as missing
for consumption fitting. Do not replace it with the mean unless the requested
analysis explicitly calls for imputation.

### Tyres

The following templates exist once per wheel suffix (`fl`, `fr`, `rl`, `rr`):

| Column template | Type / unit | Meaning and interpretation |
| --- | --- | --- |
| `compound_<wheel>` | string | Compound observed at the start of the lap. Empty means unavailable. |
| `tire_start_<wheel>_pct` | percent remaining | Remaining tread at lap start; 100 is new. |
| `tire_end_<wheel>_pct` | percent remaining | Last observed remaining tread before the lap boundary. After a mid-lap tyre change this may describe the replacement tyre. |
| `tire_wear_<wheel>_pct` | percentage points/lap | `start - minimum observed remaining`, clamped at zero. This avoids negative wear when tyres are replaced. Use this field, not `start - end`, for degradation. |
| `tire_temp_avg_<wheel>_c` | degrees Celsius | Sample-weighted average carcass temperature over the lap, approximately time-weighted at 50 Hz. |
| `tire_temp_max_<wheel>_c` | degrees Celsius | Maximum observed carcass temperature during the lap. |

`tire_change` is `true` when remaining tread rises by more than two percentage
points between samples. For such rows, do not use end tread as a continuation of
the old set. Split the stint and treat `tire_wear_*` as wear accumulated on the
pre-change set during that lap. The compound fields describe the lap start and may
not describe tyres fitted later in the same pit lap.

### Conditions and damage

| Column | Type / unit | Meaning and interpretation |
| --- | --- | --- |
| `ambient_avg_c` | degrees Celsius | Sample-weighted average ambient temperature. |
| `track_avg_c` | degrees Celsius | Sample-weighted average track temperature. |
| `rain_avg_pct` | percent | Average reported rain intensity/proportion. |
| `wetness_avg_pct` | percent | Average track wetness. Do not treat it as identical to rain. |
| `grip_avg_pct` | percent | Average player-relevant track grip estimate. |
| `damage_start_pct` | percent | Aggregate player damage at lap start. Zero can also represent unavailable enrichment. |
| `damage_end_pct` | percent | Aggregate player damage at lap end. Compare with start to identify a possible damage event. |

Conditions are sampled throughout the lap. They are better covariates than a
single boundary reading, but correlation with pace or wear does not establish
causation.

## Recommended analysis workflow

1. Parse with UTF-8 BOM handling and `;` delimiter.
2. Validate required columns before analysis; ignore unknown future columns.
3. Preserve every row in the source dataset.
4. Create a clean-lap subset with:
   `valid == true`, `green == true`, `pit_lap == false`,
   `tire_change == false`, and `lap_time_s > 0`.
5. For fuel models, additionally require `fuel_used_l > 0`; for regulated virtual
   energy models require `energy_used_pct > 0`.
6. Group by at least `track`, `vehicle`, broad session class and compound. Split
   stints on `stint`, `tire_change` and pit transitions.
7. Prefer medians or trimmed means for baseline consumption. Report sample count
   and dispersion; do not present a one-lap estimate as a stable strategy input.
8. Model tyre degradation per wheel and by axle. Control for fuel burn, track
   temperature, wetness, compound and lap validity before attributing lap-time
   loss to tyre wear.
9. Keep pit/non-green/invalid rows for audit and resource-balance reconstruction,
   even when excluded from clean pace fitting.

Useful derived values:

```text
front_wear_per_lap = mean(tire_wear_fl_pct, tire_wear_fr_pct)
rear_wear_per_lap  = mean(tire_wear_rl_pct, tire_wear_rr_pct)
fuel_laps_left     = fuel_end_l / robust_clean_fuel_used_l
energy_laps_left   = energy_end_pct / robust_clean_energy_used_pct
damage_change      = damage_end_pct - damage_start_pct
```

Only calculate autonomy when the denominator is positive and supported by enough
comparable clean laps. Do not add an arbitrary safety lap unless the user asks
for an explicit reserve policy.

## Minimal parser example

```python
import pandas as pd

laps = pd.read_csv("blackrack-strategy-....csv", sep=";", encoding="utf-8-sig")
clean = laps[
    laps["valid"]
    & laps["green"]
    & ~laps["pit_lap"]
    & ~laps["tire_change"]
    & (laps["lap_time_s"] > 0)
]
```

## AI response rules

When an AI analyzes one or more files, it should:

- state which rows were excluded and why;
- state sample sizes for every estimate;
- distinguish observed values from projections;
- flag zeros/blanks that mean unavailable;
- avoid mixing tracks, vehicles, compounds or session classes silently;
- identify tyre changes and pit laps before calculating degradation;
- give uncertainty or a range when the sample is small or variable;
- never claim causal effects from simple correlations;
- preserve the original units in results and label every converted unit;
- explain any reserve, traffic-loss, pit-loss or degradation assumption added by
  the simulation, because those values are not contained in this CSV.
