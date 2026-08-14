use serde::Serialize;

#[derive(Clone, Copy, Debug)]
pub(super) struct ResourceStrategyInput {
    pub current: f64,
    pub capacity: f64,
    pub consumption: f64,
    pub laps_remaining: f64,
    pub lap_progress: f64,
    pub completed_laps: i32,
    pub pit_cycle_consumption: f64,
    pub pit_out_consumption: f64,
    pub pit_out_lap: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct ResourceStrategy {
    pub stops: u32,
    pub target_stops: u32,
    pub target_consumption: f64,
    pub saving_percent: f64,
    pub autonomy: f64,
    pub minutes: f64,
    pub earliest_pit_lap: i32,
    pub latest_pit_lap: i32,
    pub next_fill: f64,
    pub total_additional: f64,
    pub end_remaining: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct FuelStrategies {
    pub active: Option<ResourceStrategy>,
    pub fuel: Option<ResourceStrategy>,
    pub estimated: Option<ResourceStrategy>,
    pub average: Option<ResourceStrategy>,
    pub qualifying: Option<ResourceStrategy>,
    pub last: Option<ResourceStrategy>,
}

fn valid_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn pit_adjustment(consumption: f64, pit_cycle_consumption: f64) -> f64 {
    if pit_cycle_consumption > 0.0 {
        pit_cycle_consumption - 2.0 * consumption
    } else {
        0.0
    }
}

fn remaining_pit_adjustment(consumption: f64, input: ResourceStrategyInput, stops: u32) -> f64 {
    if input.pit_out_lap && input.pit_out_consumption > 0.0 {
        let later_stops = stops.saturating_sub(1);
        input.pit_out_consumption - consumption
            + pit_adjustment(consumption, input.pit_cycle_consumption) * later_stops as f64
    } else {
        pit_adjustment(consumption, input.pit_cycle_consumption) * stops as f64
    }
}

fn required_for_stops(input: ResourceStrategyInput, stops: u32, consumption: f64) -> f64 {
    (consumption * input.laps_remaining + remaining_pit_adjustment(consumption, input, stops))
        .max(0.0)
}

fn available_for_stops(current: f64, capacity: f64, stops: u32) -> f64 {
    current.max(0.0) + capacity.max(0.0) * stops as f64
}

pub(super) fn stops_required(input: ResourceStrategyInput) -> u32 {
    if !valid_positive(input.consumption)
        || !valid_positive(input.capacity)
        || !valid_positive(input.laps_remaining)
    {
        return 0;
    }

    for stops in 0..=100 {
        if available_for_stops(input.current, input.capacity, stops) + 1e-6
            >= required_for_stops(input, stops, input.consumption)
        {
            return stops;
        }
    }
    100
}

fn consumption_for_stops(input: ResourceStrategyInput, stops: u32) -> f64 {
    let available = available_for_stops(input.current, input.capacity, stops);
    let active_pit_out = input.pit_out_lap && input.pit_out_consumption > 0.0;
    let later_stops = stops.saturating_sub(u32::from(active_pit_out));
    let replaced_laps = u32::from(active_pit_out) + 2 * later_stops;
    let fixed_pit_consumption = if active_pit_out {
        input.pit_out_consumption
    } else {
        0.0
    } + input.pit_cycle_consumption * later_stops as f64;
    let denominator = input.laps_remaining - replaced_laps as f64;
    if denominator <= 0.0 {
        return available / input.laps_remaining;
    }
    ((available - fixed_pit_consumption) / denominator).max(0.0)
}

pub(super) fn calculate_resource_strategy(
    input: ResourceStrategyInput,
    lap_seconds: f64,
    minimum_stops: u32,
) -> Option<ResourceStrategy> {
    if !valid_positive(input.consumption)
        || !valid_positive(input.capacity)
        || !valid_positive(input.laps_remaining)
        || !input.current.is_finite()
    {
        return None;
    }

    let stops = stops_required(input).max(minimum_stops);
    let target_stops = if input.pit_out_lap {
        stops
    } else {
        stops.saturating_sub(1)
    };
    let target_consumption = consumption_for_stops(input, target_stops);
    let saving_percent =
        ((input.consumption - target_consumption) / input.consumption * 100.0).max(0.0);
    let autonomy = input.current.max(0.0) / input.consumption;
    let minutes = if lap_seconds > 0.0 {
        autonomy * lap_seconds / 60.0
    } else {
        0.0
    };

    let crossings_to_latest = (autonomy + input.lap_progress.clamp(0.0, 1.0) + 1e-6)
        .floor()
        .max(0.0) as i32;
    let (mut earliest_pit_lap, mut latest_pit_lap, mut next_fill) = (0, 0, 0.0);
    if stops > 0 {
        let full_stint_distance = input.capacity.max(0.0) / input.consumption;
        let distance_needed_before_pit =
            (input.laps_remaining - full_stint_distance * stops as f64).max(0.0);
        let earliest_crossing = (distance_needed_before_pit + input.lap_progress - 1e-6)
            .ceil()
            .max(1.0) as i32;
        let latest_crossing = crossings_to_latest.max(earliest_crossing);
        earliest_pit_lap = input.completed_laps + earliest_crossing;
        latest_pit_lap = input.completed_laps + latest_crossing;

        next_fill = if input.pit_out_lap {
            (required_for_stops(input, stops, input.consumption)
                - input.capacity * stops.saturating_sub(1) as f64)
                .max(0.0)
                .min(input.capacity)
        } else {
            let distance_to_pit = (latest_crossing as f64 - input.lap_progress).max(0.0);
            let remaining_after_pit = (input.laps_remaining - distance_to_pit).max(0.0);
            (input.consumption * remaining_after_pit / stops as f64).min(input.capacity)
        };
    }

    let required = required_for_stops(input, stops, input.consumption);
    let total_additional = (required - input.current).max(0.0);
    let end_remaining = (input.current + total_additional - required).max(0.0);
    Some(ResourceStrategy {
        stops,
        target_stops,
        target_consumption,
        saving_percent,
        autonomy,
        minutes,
        earliest_pit_lap,
        latest_pit_lap,
        next_fill,
        total_additional,
        end_remaining,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ResourceStrategyInput {
        ResourceStrategyInput {
            current: 35.0,
            capacity: 100.0,
            consumption: 10.0,
            laps_remaining: 18.5,
            lap_progress: 0.4,
            completed_laps: 12,
            pit_cycle_consumption: 17.0,
            pit_out_consumption: 8.0,
            pit_out_lap: false,
        }
    }

    #[test]
    fn calculates_stops_target_window_and_fill() {
        let strategy = calculate_resource_strategy(input(), 120.0, 0).unwrap();
        assert_eq!(strategy.stops, 2);
        assert_eq!(strategy.target_stops, 1);
        assert_eq!(strategy.earliest_pit_lap, 13);
        assert_eq!(strategy.latest_pit_lap, 15);
        assert!((strategy.autonomy - 3.5).abs() < 1e-9);
        assert!((strategy.minutes - 7.0).abs() < 1e-9);
        assert!((strategy.next_fill - 79.5).abs() < 1e-9);
    }

    #[test]
    fn parallel_resource_can_enforce_a_minimum_stop_count() {
        let strategy = calculate_resource_strategy(input(), 120.0, 3).unwrap();
        assert_eq!(strategy.stops, 3);
        assert_eq!(strategy.target_stops, 2);
    }

    #[test]
    fn active_pit_out_cannot_be_removed_by_saving() {
        let strategy = calculate_resource_strategy(
            ResourceStrategyInput {
                pit_out_lap: true,
                ..input()
            },
            120.0,
            0,
        )
        .unwrap();
        assert_eq!(strategy.target_stops, strategy.stops);
    }

    #[test]
    fn active_pit_stop_uses_only_the_remaining_out_lap_adjustment() {
        let strategy = calculate_resource_strategy(
            ResourceStrategyInput {
                current: 2.968,
                capacity: 75.0,
                consumption: 2.872,
                laps_remaining: 12.0,
                lap_progress: 0.0,
                completed_laps: 0,
                pit_cycle_consumption: 4.807,
                pit_out_consumption: 2.754,
                pit_out_lap: true,
            },
            0.0,
            0,
        )
        .unwrap();
        let required = 2.968 + strategy.total_additional - strategy.end_remaining;
        assert!((required - 34.346).abs() < 0.001);
        assert!((strategy.total_additional - 31.378).abs() < 0.001);
        assert_eq!(strategy.stops, 1);
    }

    #[test]
    fn invalid_inputs_have_no_strategy() {
        assert!(calculate_resource_strategy(
            ResourceStrategyInput {
                consumption: 0.0,
                ..input()
            },
            120.0,
            0,
        )
        .is_none());
    }
}
