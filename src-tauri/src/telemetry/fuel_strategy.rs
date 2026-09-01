use serde::Serialize;

const MAX_GUIDANCE_SAVING_PERCENT: f64 = 15.0;
const STINT_TARGET_RESERVE: f64 = 0.2;

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
    pub pit_requested: bool,
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
    pub autonomy_delta: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct StintTarget {
    pub extra_laps: u32,
    pub target_consumption: f64,
    pub saving_percent: f64,
    pub stops_saved: u32,
    pub net_time_seconds: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct FuelStrategies {
    pub active: Option<ResourceStrategy>,
    pub fuel: Option<ResourceStrategy>,
    pub estimated: Option<ResourceStrategy>,
    pub average: Option<ResourceStrategy>,
    pub qualifying: Option<ResourceStrategy>,
    pub last: Option<ResourceStrategy>,
    pub conservative_next_fill: f64,
    pub conservative_fill_active: bool,
    pub stint_targets: [Option<StintTarget>; 3],
    pub next_stint_load: Option<f64>,
    pub next_stint_laps: Option<f64>,
    pub next_stint_minutes: Option<f64>,
}

pub(super) fn next_stint_autonomy(
    configured_load: Option<f64>,
    consumption: f64,
    lap_seconds: f64,
) -> (Option<f64>, Option<f64>, Option<f64>) {
    let Some(load) = configured_load.filter(|value| value.is_finite() && *value >= 0.0) else {
        return (None, None, None);
    };
    if !valid_positive(consumption) {
        return (Some(load), None, None);
    }
    let laps = load / consumption;
    let minutes = valid_positive(lap_seconds).then_some(laps * lap_seconds / 60.0);
    (Some(load), Some(laps), minutes)
}

impl FuelStrategies {
    pub(super) fn with_qualifying_guidance(mut self) -> Self {
        self.conservative_next_fill = self.active.map_or(0.0, |strategy| strategy.next_fill);

        let Some(qualifying) = self.qualifying else {
            return self;
        };
        for strategy in [
            &mut self.active,
            &mut self.estimated,
            &mut self.average,
            &mut self.qualifying,
            &mut self.last,
        ]
        .into_iter()
        .flatten()
        {
            strategy.autonomy_delta = strategy.autonomy - qualifying.autonomy;
        }

        if let Some(active) = self.active {
            self.conservative_fill_active = active.stops == 1 && qualifying.stops == 1;
            if self.conservative_fill_active {
                self.conservative_next_fill = active.next_fill.max(qualifying.next_fill);
            }
        }
        self
    }
}

pub(super) fn calculate_stint_targets(
    input: ResourceStrategyInput,
    lap_seconds: f64,
    minimum_stops: u32,
    consumption_into_lap: f64,
    pit_entry_bias: f64,
    qualifying_consumption: f64,
    qualifying_lap_seconds: f64,
    pit_stop_seconds: f64,
    resource_service_seconds: f64,
    other_service_seconds: f64,
    pit_traversal_seconds: f64,
) -> [Option<StintTarget>; 3] {
    if !valid_positive(input.consumption) || !valid_positive(input.capacity) {
        return [None; 3];
    }

    let Some(baseline_strategy) = calculate_resource_strategy(input, lap_seconds, minimum_stops)
    else {
        return [None; 3];
    };
    let baseline_stops = baseline_strategy.stops;
    // TinyPedal's saver works from the resource available at the start of the
    // current lap, not from a hypothetical full tank. Round autonomy to one
    // decimal before flooring to prevent a noisy boundary from flickering.
    let available_at_lap_start = (input.current.max(0.0) + consumption_into_lap.max(0.0)
        - STINT_TARGET_RESERVE
        + if baseline_stops > 0 {
            pit_entry_bias.clamp(0.0, 1.0) * input.consumption
        } else {
            0.0
        })
    .max(0.0);
    let rounded_autonomy = (available_at_lap_start / input.consumption * 10.0).round() / 10.0;
    let baseline_stint_laps = rounded_autonomy.floor().max(0.0) as u32;
    let pace_cost_per_resource = if valid_positive(qualifying_consumption)
        && qualifying_consumption > input.consumption + 1e-6
        && valid_positive(qualifying_lap_seconds)
        && lap_seconds >= qualifying_lap_seconds
    {
        Some((lap_seconds - qualifying_lap_seconds) / (qualifying_consumption - input.consumption))
    } else {
        None
    };

    std::array::from_fn(|index| {
        let extra_laps = index as u32 + 1;
        let target_consumption =
            available_at_lap_start / f64::from(baseline_stint_laps + extra_laps);
        let target_strategy = calculate_resource_strategy(
            ResourceStrategyInput {
                consumption: target_consumption,
                ..input
            },
            lap_seconds,
            minimum_stops,
        )?;
        let stops_saved = baseline_stops.saturating_sub(target_strategy.stops);
        let saving_percent =
            ((input.consumption - target_consumption) / input.consumption * 100.0).max(0.0);
        let net_time_seconds = pace_cost_per_resource.and_then(|pace_cost| {
            (valid_positive(pit_stop_seconds)
                && (stops_saved == 0 || valid_positive(pit_traversal_seconds)))
            .then(|| {
                let lap_cost = pace_cost * (input.consumption - target_consumption).max(0.0);
                let fallback_service_gain = f64::from(stops_saved) * pit_stop_seconds;
                let service_gain = if baseline_stops > 0
                    && valid_positive(resource_service_seconds)
                    && valid_positive(baseline_strategy.total_additional)
                {
                    let baseline_fill =
                        baseline_strategy.total_additional / f64::from(baseline_stops);
                    let seconds_per_unit = resource_service_seconds / baseline_fill;
                    let parallel_service =
                        resource_service_seconds.max(other_service_seconds.max(0.0));
                    let fixed_service = (pit_stop_seconds - parallel_service).max(0.0);
                    let baseline_service = pit_stop_seconds.max(parallel_service);
                    let target_service = if target_strategy.stops > 0 {
                        let target_fill =
                            target_strategy.total_additional / f64::from(target_strategy.stops);
                        fixed_service
                            + (target_fill * seconds_per_unit).max(other_service_seconds.max(0.0))
                    } else {
                        0.0
                    };
                    f64::from(baseline_stops) * baseline_service
                        - f64::from(target_strategy.stops) * target_service
                } else {
                    fallback_service_gain
                };
                let traversal_gain = f64::from(stops_saved) * pit_traversal_seconds.max(0.0);
                service_gain + traversal_gain - lap_cost * input.laps_remaining
            })
        });
        Some(StintTarget {
            extra_laps,
            target_consumption,
            saving_percent,
            stops_saved,
            net_time_seconds,
        })
    })
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
    let reduced_stops = stops.saturating_sub(1).max(minimum_stops);
    let reduced_consumption = consumption_for_stops(input, reduced_stops);
    let reduced_saving_percent =
        ((input.consumption - reduced_consumption) / input.consumption * 100.0).max(0.0);
    let shows_reduced_plan = !input.pit_out_lap
        && reduced_stops < stops
        && reduced_saving_percent <= MAX_GUIDANCE_SAVING_PERCENT + 1e-6;
    let (target_stops, target_consumption) = if shows_reduced_plan {
        (reduced_stops, reduced_consumption)
    } else {
        (stops, input.consumption)
    };
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
            let pit_crossing = if input.pit_requested {
                1
            } else {
                latest_crossing
            };
            let distance_to_pit = (pit_crossing as f64 - input.lap_progress).max(0.0);
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
        autonomy_delta: 0.0,
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
            pit_requested: false,
        }
    }

    #[test]
    fn calculates_stops_window_and_fill_without_an_exaggerated_target() {
        let strategy = calculate_resource_strategy(input(), 120.0, 0).unwrap();
        assert_eq!(strategy.stops, 2);
        assert_eq!(strategy.target_stops, 2);
        assert_eq!(strategy.earliest_pit_lap, 13);
        assert_eq!(strategy.latest_pit_lap, 15);
        assert!((strategy.autonomy - 3.5).abs() < 1e-9);
        assert!((strategy.minutes - 7.0).abs() < 1e-9);
        assert!((strategy.next_fill - 79.5).abs() < 1e-9);
    }

    #[test]
    fn parallel_resource_floor_cannot_be_removed_by_the_target() {
        let strategy = calculate_resource_strategy(input(), 120.0, 3).unwrap();
        assert_eq!(strategy.stops, 3);
        assert_eq!(strategy.target_stops, 3);
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
                pit_requested: false,
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
    fn pit_request_calculates_the_refill_for_the_requested_lap() {
        let strategy = calculate_resource_strategy(
            ResourceStrategyInput {
                pit_requested: true,
                ..input()
            },
            120.0,
            0,
        )
        .unwrap();

        assert_eq!(strategy.stops, 2);
        assert!((strategy.next_fill - 89.5).abs() < 1e-9);
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

    #[test]
    fn shows_a_reduced_plan_as_a_reasonable_saving_reference() {
        let strategy = calculate_resource_strategy(
            ResourceStrategyInput {
                current: 57.25,
                ..input()
            },
            120.0,
            0,
        )
        .unwrap();

        assert_eq!(strategy.stops, 2);
        assert_eq!(strategy.target_stops, 1);
        assert!((strategy.target_consumption - 8.5).abs() < 1e-9);
        assert!((strategy.saving_percent - 15.0).abs() < 1e-9);
    }

    #[test]
    fn hides_a_reduced_plan_when_the_required_saving_is_exaggerated() {
        let strategy = calculate_resource_strategy(input(), 120.0, 0).unwrap();

        assert_eq!(strategy.stops, 2);
        assert_eq!(strategy.target_stops, 2);
        assert_eq!(strategy.target_consumption, 10.0);
        assert_eq!(strategy.saving_percent, 0.0);
    }

    #[test]
    fn scenario_autonomy_is_compared_with_the_qualifying_reference() {
        let qualifying = calculate_resource_strategy(input(), 120.0, 0).unwrap();
        let last = calculate_resource_strategy(
            ResourceStrategyInput {
                consumption: 8.0,
                ..input()
            },
            120.0,
            0,
        )
        .unwrap();
        let strategies = FuelStrategies {
            active: Some(last),
            qualifying: Some(qualifying),
            last: Some(last),
            ..FuelStrategies::default()
        }
        .with_qualifying_guidance();

        assert!((strategies.qualifying.unwrap().autonomy_delta).abs() < 1e-9);
        assert!((strategies.last.unwrap().autonomy_delta - 0.875).abs() < 1e-9);
    }

    #[test]
    fn final_stop_uses_the_more_conservative_qualifying_fill() {
        let final_stint_input = ResourceStrategyInput {
            current: 35.0,
            capacity: 100.0,
            laps_remaining: 8.0,
            pit_cycle_consumption: 0.0,
            pit_out_consumption: 0.0,
            ..input()
        };
        let active = calculate_resource_strategy(
            ResourceStrategyInput {
                consumption: 8.0,
                ..final_stint_input
            },
            120.0,
            0,
        )
        .unwrap();
        let qualifying = calculate_resource_strategy(
            ResourceStrategyInput {
                consumption: 10.0,
                ..final_stint_input
            },
            120.0,
            0,
        )
        .unwrap();
        let strategies = FuelStrategies {
            active: Some(active),
            qualifying: Some(qualifying),
            ..FuelStrategies::default()
        }
        .with_qualifying_guidance();

        assert_eq!(active.stops, 1);
        assert_eq!(qualifying.stops, 1);
        assert!(strategies.conservative_fill_active);
        assert_eq!(strategies.conservative_next_fill, qualifying.next_fill);
        assert!(strategies.conservative_next_fill > active.next_fill);
    }

    #[test]
    fn stint_targets_extend_the_current_stint_range() {
        let targets = calculate_stint_targets(
            ResourceStrategyInput {
                current: 90.0,
                ..input()
            },
            120.0,
            0,
            10.2,
            0.0,
            11.0,
            118.0,
            30.0,
            10.0,
            0.0,
            0.0,
        );

        let plus_one = targets[0].unwrap();
        assert_eq!(plus_one.extra_laps, 1);
        assert!((plus_one.target_consumption - (100.0 / 11.0)).abs() < 1e-9);
        assert!(plus_one.saving_percent > 9.0);
    }

    #[test]
    fn stint_targets_use_current_resource_reserve_and_pit_entry_bias() {
        let target_input = ResourceStrategyInput {
            current: 49.0,
            capacity: 100.0,
            consumption: 10.0,
            laps_remaining: 20.0,
            pit_cycle_consumption: 0.0,
            pit_out_consumption: 0.0,
            ..input()
        };
        let without_bias = calculate_stint_targets(
            target_input,
            120.0,
            0,
            1.2,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        )[0]
        .unwrap();
        let with_bias = calculate_stint_targets(
            target_input,
            120.0,
            0,
            1.2,
            0.2,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        )[0]
        .unwrap();

        assert!((without_bias.target_consumption - 50.0 / 6.0).abs() < 1e-9);
        assert!((with_bias.target_consumption - 52.0 / 6.0).abs() < 1e-9);
        assert!(with_bias.target_consumption < input().consumption);
    }

    #[test]
    fn stint_target_time_compares_saved_stops_with_estimated_pace_cost() {
        let targets = calculate_stint_targets(
            ResourceStrategyInput {
                current: 90.0,
                capacity: 100.0,
                consumption: 10.0,
                laps_remaining: 20.0,
                pit_cycle_consumption: 0.0,
                pit_out_consumption: 0.0,
                ..input()
            },
            101.0,
            0,
            10.2,
            0.0,
            11.0,
            100.0,
            30.0,
            10.0,
            0.0,
            42.0,
        );

        let plus_one = targets[0].unwrap();
        assert_eq!(plus_one.stops_saved, 1);
        assert!(plus_one.net_time_seconds.unwrap() > 0.0);
    }

    #[test]
    fn stint_target_time_is_unknown_without_a_pace_or_pit_reference() {
        let targets =
            calculate_stint_targets(input(), 120.0, 0, 0.2, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert!(targets
            .iter()
            .flatten()
            .all(|target| target.net_time_seconds.is_none()));
    }

    #[test]
    fn removed_stop_needs_learned_pitlane_time_before_showing_a_balance() {
        let targets = calculate_stint_targets(
            ResourceStrategyInput {
                current: 90.0,
                capacity: 100.0,
                consumption: 10.0,
                laps_remaining: 20.0,
                pit_cycle_consumption: 0.0,
                pit_out_consumption: 0.0,
                ..input()
            },
            101.0,
            0,
            10.2,
            0.0,
            11.0,
            100.0,
            30.0,
            10.0,
            0.0,
            0.0,
        );

        assert_eq!(targets[0].unwrap().stops_saved, 1);
        assert!(targets[0].unwrap().net_time_seconds.is_none());
    }

    #[test]
    fn stint_target_time_includes_shorter_refuelling_with_the_same_stop_count() {
        let targets = calculate_stint_targets(
            ResourceStrategyInput {
                current: 50.0,
                capacity: 100.0,
                consumption: 10.0,
                laps_remaining: 12.0,
                pit_cycle_consumption: 0.0,
                pit_out_consumption: 0.0,
                ..input()
            },
            100.0,
            0,
            0.2,
            0.0,
            11.0,
            100.0,
            10.0,
            10.0,
            0.0,
            0.0,
        );

        let plus_one = targets[0].unwrap();
        assert_eq!(plus_one.stops_saved, 0);
        assert!(plus_one.net_time_seconds.unwrap() > 0.0);
    }

    #[test]
    fn parallel_driver_swap_sets_a_floor_instead_of_adding_to_refuelling() {
        let target_input = ResourceStrategyInput {
            current: 50.0,
            capacity: 100.0,
            consumption: 10.0,
            laps_remaining: 12.0,
            pit_cycle_consumption: 0.0,
            pit_out_consumption: 0.0,
            ..input()
        };
        let refuelling_is_longer = calculate_stint_targets(
            target_input,
            100.0,
            0,
            0.2,
            0.0,
            11.0,
            100.0,
            30.0,
            30.0,
            26.0,
            0.0,
        )[0]
        .unwrap();
        let driver_swap_is_longer = calculate_stint_targets(
            target_input,
            100.0,
            0,
            0.2,
            0.0,
            11.0,
            100.0,
            26.0,
            20.0,
            26.0,
            0.0,
        )[0]
        .unwrap();
        let fixed_overhead_is_preserved = calculate_stint_targets(
            target_input,
            100.0,
            0,
            0.2,
            0.0,
            11.0,
            100.0,
            34.0,
            30.0,
            26.0,
            0.0,
        )[0]
        .unwrap();

        assert!((refuelling_is_longer.net_time_seconds.unwrap() - 4.0).abs() < 1e-9);
        assert!(driver_swap_is_longer.net_time_seconds.unwrap().abs() < 1e-9);
        assert!((fixed_overhead_is_preserved.net_time_seconds.unwrap() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn next_stint_autonomy_uses_the_absolute_mfd_load() {
        let (load, laps, minutes) = next_stint_autonomy(Some(75.0), 10.0, 120.0);

        assert_eq!(load, Some(75.0));
        assert_eq!(laps, Some(7.5));
        assert_eq!(minutes, Some(15.0));
    }
}
