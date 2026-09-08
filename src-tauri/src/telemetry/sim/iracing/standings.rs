//! Car roster and live ordering for the standings-family overlays.
//!
//! iRacing does not expose a single authoritative standings array. Static
//! identity comes from DriverInfo and live order comes from the CarIdx
//! lap/distance arrays, with SessionInfo results supplying the
//! initial/classification data when available.

use std::cmp::Ordering;
use std::collections::HashMap;

use super::irsdk::Connection;
use super::session::{Driver, ResultPosition, Session};
use crate::telemetry::StandingEntry;

const SURFACE_NOT_IN_WORLD: i32 = -1;
const DEFAULT_LAP_TIME: f64 = 100.0;

#[derive(Clone)]
struct CarSample<'a> {
    driver: &'a Driver,
    result: Option<ResultPosition>,
    laps: i32,
    progress: Option<f64>,
    surface: i32,
    in_pits: bool,
    best_lap: f64,
    last_lap: f64,
}

impl CarSample<'_> {
    fn car_idx(&self) -> i32 {
        self.driver.car_idx
    }

    fn in_garage(&self) -> bool {
        self.surface == SURFACE_NOT_IN_WORLD && !self.in_pits
    }

    fn pace(&self) -> f64 {
        positive(self.best_lap)
            .or_else(|| positive(self.last_lap))
            .unwrap_or(DEFAULT_LAP_TIME)
    }
}

fn positive(value: f64) -> Option<f64> {
    value
        .is_finite()
        .then_some(value)
        .filter(|value| *value > 1.0)
}

fn result_for(results: &[ResultPosition], car_idx: i32) -> Option<ResultPosition> {
    results
        .iter()
        .find(|result| result.car_idx == car_idx)
        .copied()
}

fn sample<'a>(
    session: &Session,
    sdk: &Connection,
    driver: &'a Driver,
    session_number: i32,
) -> CarSample<'a> {
    let result = result_for(session.results(session_number), driver.car_idx);
    let laps = sdk
        .integer_at("CarIdxLap", driver.car_idx as usize)
        .filter(|laps| *laps >= 0)
        .or_else(|| {
            result
                .map(|result| result.laps_complete)
                .filter(|laps| *laps >= 0)
        })
        .unwrap_or(-1);
    let surface = sdk
        .integer_at("CarIdxTrackSurface", driver.car_idx as usize)
        .unwrap_or(SURFACE_NOT_IN_WORLD);
    let in_pits = sdk
        .integer_at("CarIdxOnPitRoad", driver.car_idx as usize)
        .is_some_and(|value| value != 0);
    let lap_fraction = sdk
        .number_at("CarIdxLapDistPct", driver.car_idx as usize)
        .filter(|fraction| fraction.is_finite() && (0.0..=1.0).contains(fraction));
    // CarIdxLap is the live zero-based completed-lap count. ResultsPositions is
    // the fallback while the live array is not published yet.
    let progress = if laps >= 0 {
        lap_fraction
            .map(|fraction| laps as f64 + fraction)
            .or_else(|| Some(laps as f64))
    } else {
        None
    };
    let result_best = result.map_or(0.0, |result| result.fastest_lap_time);
    let result_last = result.map_or(0.0, |result| result.last_lap_time);
    let best_lap = if driver.car_idx == session.player_car_idx {
        positive(sdk.number("LapBestLapTime").unwrap_or(0.0)).unwrap_or(result_best)
    } else {
        result_best
    };
    let last_lap = if driver.car_idx == session.player_car_idx {
        positive(sdk.number("LapLastLapTime").unwrap_or(0.0)).unwrap_or(result_last)
    } else {
        result_last
    };
    CarSample {
        driver,
        result,
        laps: laps.max(0),
        progress,
        surface,
        in_pits,
        best_lap,
        last_lap,
    }
}

fn result_order(sample: &CarSample) -> (i32, i32, i32) {
    let result = sample.result.unwrap_or_default();
    (
        result.overall_position.unwrap_or(i32::MAX),
        result.class_position.unwrap_or(i32::MAX),
        sample.car_idx(),
    )
}

fn compare_samples(left: &CarSample, right: &CarSample, race: bool) -> Ordering {
    if race {
        match (left.progress, right.progress) {
            (Some(left_progress), Some(right_progress)) => right_progress
                .total_cmp(&left_progress)
                .then_with(|| result_order(left).cmp(&result_order(right))),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => result_order(left).cmp(&result_order(right)),
        }
    } else {
        match (positive(left.best_lap), positive(right.best_lap)) {
            (Some(left_best), Some(right_best)) => left_best
                .total_cmp(&right_best)
                .then_with(|| result_order(left).cmp(&result_order(right))),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => result_order(left).cmp(&result_order(right)),
        }
    }
    .then_with(|| left.car_idx().cmp(&right.car_idx()))
}

fn gap(ahead: &CarSample, behind: &CarSample, pace: f64) -> (i32, f64) {
    let Some(difference) = ahead
        .progress
        .zip(behind.progress)
        .map(|(ahead, behind)| (ahead - behind).max(0.0))
    else {
        return (0, 0.0);
    };
    crate::telemetry::standings_math::class_relative_gap(difference, None, pace)
}

fn relative_gap(sample: &CarSample, player: &CarSample, pace: f64) -> (f64, f64, i32) {
    let Some(delta) = sample
        .progress
        .zip(player.progress)
        .map(|(sample, player)| (sample - player) * pace)
    else {
        return (0.0, 0.0, 0);
    };
    let progress_delta = sample.progress.unwrap_or(0.0) - player.progress.unwrap_or(0.0);
    let laps = if progress_delta >= 1.0 {
        progress_delta.floor() as i32
    } else if progress_delta <= -1.0 {
        progress_delta.ceil() as i32
    } else {
        0
    };
    if delta < -0.05 {
        (delta, 0.0, laps)
    } else if delta > 0.05 {
        (0.0, delta, laps)
    } else {
        (0.0, 0.0, laps)
    }
}

fn driver_name(driver: &Driver) -> String {
    if !driver.user_name.trim().is_empty() {
        return driver.user_name.trim().to_owned();
    }
    if !driver.abbrev_name.trim().is_empty() {
        return driver.abbrev_name.trim().to_owned();
    }
    driver.initials.trim().to_owned()
}

fn class_name(driver: &Driver) -> String {
    let class = driver.car_class_short_name.trim();
    if class.is_empty() {
        "OTHER".into()
    } else {
        class.to_owned()
    }
}

fn car_number(driver: &Driver) -> String {
    driver
        .car_number
        .trim()
        .trim_start_matches('#')
        .trim()
        .to_owned()
}

/// Builds one row per non-spectator car and keeps the source's baseline class
/// positions for the race gain marker. A source connected after the race has
/// started has no honest baseline and therefore reports a neutral change.
pub(super) fn build(
    session: &Session,
    sdk: &Connection,
    session_number: i32,
    session_type: i32,
    session_state: i32,
    starting_positions: &mut HashMap<i32, i32>,
) -> Vec<StandingEntry> {
    let race = (10..=13).contains(&session_type);
    let mut samples = session
        .cars()
        .into_iter()
        .filter(|driver| (0..64).contains(&driver.car_idx))
        .map(|driver| sample(session, sdk, driver, session_number))
        .collect::<Vec<_>>();
    samples.sort_by(|left, right| compare_samples(left, right, race));

    let mut class_sizes = HashMap::<String, i32>::new();
    for sample in &samples {
        *class_sizes.entry(class_name(&sample.driver)).or_default() += 1;
    }

    let mut class_positions = HashMap::<String, i32>::new();
    if race && session_state < 4 {
        for sample in &samples {
            let class = class_name(&sample.driver);
            let position = class_positions.entry(class).or_default();
            *position += 1;
            starting_positions
                .entry(sample.car_idx())
                .or_insert(*position);
        }
        class_positions.clear();
    }

    let mut class_leaders = HashMap::<String, usize>::new();
    let mut class_previous = HashMap::<String, usize>::new();
    let mut class_fastest = HashMap::<String, f64>::new();
    for (index, sample) in samples.iter().enumerate() {
        let class = class_name(&sample.driver);
        *class_positions.entry(class.clone()).or_default() += 1;
        class_leaders.entry(class.clone()).or_insert(index);
        class_previous.insert(class.clone(), index);
        if let Some(best) = positive(sample.best_lap) {
            class_fastest
                .entry(class)
                .and_modify(|fastest| *fastest = fastest.min(best))
                .or_insert(best);
        }
    }

    let player_index = samples
        .iter()
        .position(|sample| sample.car_idx() == session.player_car_idx);
    let player_pace = player_index.map_or(DEFAULT_LAP_TIME, |index| samples[index].pace());

    class_positions.clear();
    class_previous.clear();
    let mut entries = Vec::with_capacity(samples.len());
    for (overall_index, sample) in samples.iter().enumerate() {
        let class = class_name(&sample.driver);
        let class_position = class_positions.entry(class.clone()).or_default();
        *class_position += 1;
        let class_position = *class_position;
        let leader_index = *class_leaders.get(&class).unwrap_or(&overall_index);
        let leader = &samples[leader_index];
        let class_pace = leader.pace();
        let (laps_behind_leader, time_behind_leader) = gap(leader, sample, class_pace);
        let (laps_behind_next, interval) = class_previous
            .get(&class)
            .map(|previous| gap(&samples[*previous], sample, class_pace))
            .unwrap_or((0, 0.0));
        class_previous.insert(class.clone(), overall_index);

        let (relative_ahead_seconds, relative_behind_seconds, laps_relative_to_player) =
            player_index.map_or((0.0, 0.0, 0), |player_index| {
                if player_index == overall_index {
                    (0.0, 0.0, 0)
                } else {
                    relative_gap(sample, &samples[player_index], player_pace)
                }
            });
        let relative_gap_seconds = if relative_ahead_seconds.abs() > 0.0 {
            relative_ahead_seconds
        } else {
            relative_behind_seconds
        };
        let fastest_lap = class_fastest.get(&class).copied().unwrap_or(0.0);
        let result = sample.result.unwrap_or_default();
        let driver_rank = (sample.driver.irating > 0).then(|| sample.driver.irating.to_string());
        let driver_rank = driver_rank.unwrap_or_default();
        let starting_position = if race {
            starting_positions
                .get(&sample.car_idx())
                .copied()
                .unwrap_or(class_position)
        } else {
            class_position
        };
        let is_player = sample.car_idx() == session.player_car_idx;
        let total_laps = result.laps_complete.max(sample.laps).max(0);

        entries.push(StandingEntry {
            vehicle_id: sample.car_idx(),
            overall_position: (overall_index + 1) as i32,
            position: class_position,
            position_change: starting_position - class_position,
            car_number: car_number(&sample.driver),
            driver_name: driver_name(&sample.driver),
            // iRacing's native equivalent of the DR/SR pair is iRating and
            // license. The renderer keeps its compact two-badge layout.
            driver_rank,
            driver_rank_progress: -1.0,
            estimated_driver_rank_gain: 0.0,
            estimated_driver_rank_gain_available: false,
            safety_rank: sample.driver.license.clone(),
            safety_rank_progress: -1.0,
            nationality: sample.driver.nationality.clone(),
            driver_badge: String::new(),
            team_name: sample.driver.team_name.clone(),
            vehicle_name: sample.driver.car_screen_name.clone(),
            vehicle_class: class,
            initial_class_count: class_sizes
                .get(&class_name(&sample.driver))
                .copied()
                .unwrap_or(0) as usize,
            laps_relative_to_player,
            total_laps,
            laps_behind_leader,
            laps_behind_next,
            time_behind_leader,
            interval,
            relative_gap_seconds,
            relative_ahead_seconds,
            relative_behind_seconds,
            best_lap_seconds: positive(sample.best_lap).unwrap_or(0.0),
            last_lap_seconds: positive(sample.last_lap).unwrap_or(0.0),
            // iRacing does not publish the five-lap history in this source;
            // leave AVG empty rather than presenting the best as an average.
            average_lap_seconds: 0.0,
            virtual_energy_active: false,
            virtual_energy_percent: 0.0,
            virtual_energy_per_lap: 0.0,
            damage_percent: 0.0,
            track_limits_steps: None,
            pit_stops: 0,
            pit_stop_requested: false,
            pit_stop_lap: None,
            pit_stop_time_seconds: None,
            tire_compound: String::new(),
            tire_compounds: std::array::from_fn(|_| String::new()),
            flag: 0,
            causing_yellow: false,
            has_fastest_lap: fastest_lap > 0.0
                && positive(sample.best_lap)
                    .is_some_and(|best| (best - fastest_lap).abs() <= 0.001),
            in_pits: sample.in_pits,
            in_garage: sample.in_garage(),
            is_out_lap: false,
            last_lap_valid: true,
            penalty_count: 0,
            finish_status: 0,
            is_player,
        });
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::{car_number, class_name, driver_name, positive, relative_gap, CarSample};
    use crate::telemetry::sim::iracing::session::Driver;

    #[test]
    fn normalizes_identity_fallbacks() {
        let driver = Driver {
            car_number: " #42 ".into(),
            user_name: " Driver One ".into(),
            car_class_short_name: "GT3".into(),
            ..Driver::default()
        };
        assert_eq!(car_number(&driver), "42");
        assert_eq!(driver_name(&driver), "Driver One");
        assert_eq!(class_name(&driver), "GT3");
    }

    #[test]
    fn rejects_non_lap_times() {
        assert_eq!(positive(0.0), None);
        assert_eq!(positive(-1.0), None);
        assert_eq!(positive(92.5), Some(92.5));
    }

    #[test]
    fn reports_lap_relation_only_after_a_complete_progress_difference() {
        let default_driver = Driver::default();
        let player = CarSample {
            driver: &default_driver,
            result: None,
            laps: 2,
            progress: Some(2.0),
            surface: -1,
            in_pits: false,
            best_lap: 100.0,
            last_lap: 100.0,
        };
        let same_lap = CarSample {
            progress: Some(2.75),
            ..player.clone()
        };
        let lap_ahead = CarSample {
            progress: Some(3.0),
            ..player.clone()
        };
        let lap_behind = CarSample {
            progress: Some(0.75),
            ..player.clone()
        };

        assert_eq!(relative_gap(&same_lap, &player, 100.0).2, 0);
        assert_eq!(relative_gap(&lap_ahead, &player, 100.0).2, 1);
        assert_eq!(relative_gap(&lap_behind, &player, 100.0).2, -1);
    }
}
