use super::{
    fuel_energy_ratio, lmu_snapshot_size, rear_wing_detached, suspension_damage_by_wheel_percent,
    suspension_damage_percent, synchronized_lap_progress, CarHistory, LmuSnapshot,
    LmuStandingEntry, LmuTelemetrySource, PlayerLapDistanceEstimator, PlayerLapDistanceSample,
    PlayerLapTimeHistory, SessionContext, TireWearTracker,
};
use crate::telemetry::sim::lmu::event_split::DriverRankSettings;
use crate::telemetry::sim::lmu::rest::{RestCompoundCondition, RestStanding};
use crate::telemetry::StandingEntry;
use std::collections::{HashMap, HashSet};

fn lap_distance_sample(
    raw_distance: f64,
    current_lap_seconds: f64,
    speed_kph: f64,
    gear: i32,
    lap_number: i32,
    track_length: f64,
    lap_changed: bool,
) -> PlayerLapDistanceSample {
    PlayerLapDistanceSample {
        raw_distance,
        current_lap_seconds,
        speed_kph,
        gear,
        lap_number,
        track_length,
        lap_changed,
    }
}

#[test]
fn fuel_energy_ratio_requires_both_valid_consumptions() {
    assert!((fuel_energy_ratio(12.0, 8.0) - 1.5).abs() < 1e-9);
    assert_eq!(fuel_energy_ratio(12.0, 0.0), 0.0);
    assert_eq!(fuel_energy_ratio(f64::NAN, 8.0), 0.0);
}

#[test]
fn player_persistence_uses_generic_model_instead_of_livery() {
    let mut snapshot = LmuSnapshot::default();
    set_chars(&mut snapshot.vehicle_name, "AMR GT3 Custom Team 2025 #397");
    set_chars(
        &mut snapshot.vehicle_model,
        "Aston Martin Vantage AMR LMGT3",
    );

    assert_eq!(
        LmuTelemetrySource::player_vehicle_names(&snapshot),
        (
            "Aston Martin Vantage AMR LMGT3".into(),
            "AMR GT3 Custom Team 2025 #397".into()
        )
    );
}

#[test]
fn player_persistence_falls_back_when_model_is_unavailable() {
    let mut snapshot = LmuSnapshot::default();
    set_chars(&mut snapshot.vehicle_name, "Legacy vehicle name");

    assert_eq!(
        LmuTelemetrySource::player_vehicle_names(&snapshot),
        ("Legacy vehicle name".into(), "Legacy vehicle name".into())
    );
}

#[test]
fn lap_progress_suppresses_stale_finish_distance_after_telemetry_boundary() {
    assert_eq!(synchronized_lap_progress(0.998, 0.02, true), 0.0);
    assert_eq!(synchronized_lap_progress(0.999, 0.12, false), 0.0);
    assert_eq!(synchronized_lap_progress(0.002, 0.22, false), 0.002);
    assert_eq!(synchronized_lap_progress(0.95, 96.0, false), 0.95);
}

#[test]
fn player_lap_distance_advances_between_scoring_updates() {
    let mut estimator = PlayerLapDistanceEstimator::default();
    assert_eq!(
        estimator.update(lap_distance_sample(0.0, 0.0, 180.0, 3, 4, 5_000.0, true)),
        0.0
    );
    assert_eq!(
        estimator.update(lap_distance_sample(0.0, 0.02, 180.0, 3, 4, 5_000.0, false)),
        1.0
    );
    assert_eq!(
        estimator.update(lap_distance_sample(0.0, 0.04, 180.0, 3, 4, 5_000.0, false)),
        2.0
    );

    let refreshed = estimator.update(lap_distance_sample(10.0, 0.2, 180.0, 3, 4, 5_000.0, false));
    assert!((refreshed - 10.0).abs() < 0.001);
    let held = estimator.update(lap_distance_sample(10.0, 0.22, 180.0, 3, 4, 5_000.0, false));
    assert!((held - 11.0).abs() < 0.001);
}

#[test]
fn player_lap_distance_resets_at_the_lap_boundary() {
    let mut estimator = PlayerLapDistanceEstimator::default();
    estimator.update(lap_distance_sample(
        4_990.0, 95.0, 180.0, 4, 3, 5_000.0, false,
    ));
    let reset = estimator.update(lap_distance_sample(0.0, 0.02, 180.0, 3, 4, 5_000.0, true));
    assert_eq!(reset, 0.0);
}

#[test]
fn player_lap_history_reconstructs_an_omitted_invalid_duration() {
    let mut history = PlayerLapTimeHistory::default();

    assert_eq!(history.update(400.0, 2.0), 0.0);
    assert_eq!(history.update(445.0, 0.5), 0.0);
    assert_eq!(history.update(445.0, 1.5), 45.0);
}

#[test]
fn live_weather_icon_combines_cloud_cover_and_rain_intensity() {
    assert_eq!(LmuTelemetrySource::live_weather_icon(3, 0.0), 3);
    assert_eq!(LmuTelemetrySource::live_weather_icon(9, 0.0), 4);
    assert_eq!(LmuTelemetrySource::live_weather_icon(2, 8.0), 5);
    assert_eq!(LmuTelemetrySource::live_weather_icon(2, 18.0), 7);
    assert_eq!(LmuTelemetrySource::live_weather_icon(2, 55.0), 9);
    assert_eq!(LmuTelemetrySource::live_weather_icon(2, 75.0), 10);
}

#[test]
fn track_grip_level_maps_to_lmu_grip_fraction() {
    assert_eq!(LmuTelemetrySource::track_grip_percent(0), 0.0);
    assert_eq!(LmuTelemetrySource::track_grip_percent(1), 25.0);
    assert_eq!(LmuTelemetrySource::track_grip_percent(2), 50.0);
    assert_eq!(LmuTelemetrySource::track_grip_percent(3), 75.0);
    assert_eq!(LmuTelemetrySource::track_grip_percent(4), 90.0);
    assert_eq!(LmuTelemetrySource::track_grip_percent(5), 0.0);
}

#[test]
fn track_rubber_estimate_uses_session_base_and_all_valid_completed_laps() {
    let mut snapshot = LmuSnapshot {
        session_type: 0,
        standings_count: 3,
        ..LmuSnapshot::default()
    };
    snapshot.standings[0] = LmuStandingEntry {
        vehicle_id: 1,
        position: 1,
        total_laps: 10,
        ..LmuStandingEntry::default()
    };
    snapshot.standings[1] = LmuStandingEntry {
        vehicle_id: 2,
        position: 2,
        total_laps: 17,
        ..LmuStandingEntry::default()
    };
    snapshot.standings[2] = LmuStandingEntry {
        vehicle_id: 3,
        position: 3,
        total_laps: 10_000,
        ..LmuStandingEntry::default()
    };

    assert_eq!(LmuTelemetrySource::track_rubber_percent(&snapshot), 26.0125);

    snapshot.session_type = 5;
    snapshot.standings_count = 0;
    assert_eq!(LmuTelemetrySource::track_rubber_percent(&snapshot), 50.0);
}

#[test]
fn track_surface_state_uses_dry_and_lmu_wetness_bands() {
    assert_eq!(LmuTelemetrySource::track_surface_state(0.99), "dry");
    assert_eq!(LmuTelemetrySource::track_surface_state(1.0), "damp");
    assert_eq!(LmuTelemetrySource::track_surface_state(15.0), "wet");
    assert_eq!(LmuTelemetrySource::track_surface_state(40.0), "heavy");
    assert_eq!(LmuTelemetrySource::track_surface_state(70.0), "saturated");
}

#[test]
fn wind_uses_official_rest_node_compass_values() {
    assert_eq!(
        LmuTelemetrySource::resolve_wind(Some((7.0, 90.0))),
        (7.0, 90.0)
    );
    assert_eq!(LmuTelemetrySource::resolve_wind(None), (0.0, 0.0));
}

#[test]
fn wind_arrow_is_relative_to_vehicle_orientation() {
    assert_eq!(
        LmuTelemetrySource::relative_wind_direction_degrees(90.0, 0.0, 1.0),
        270.0
    );
    assert_eq!(
        LmuTelemetrySource::relative_wind_direction_degrees(0.0, 1.0, 0.0),
        270.0
    );
}

#[test]
fn flat_spot_wear_only_accumulates_during_a_localized_slide() {
    let mut tracker = TireWearTracker::default();
    tracker.update([100.0; 4], [0.0; 4], [0.0; 4], 0.0, false);

    let normal_wear = tracker.update([99.9; 4], [-0.1; 4], [0.8; 4], 0.0, false);
    assert_eq!(normal_wear, [0.0; 4]);

    let low_grip_wear = tracker.update([99.8; 4], [-0.5; 4], [0.4; 4], 0.0, false);
    assert_eq!(low_grip_wear, [0.0; 4]);

    let slide_wear = tracker.update(
        [99.6, 99.7, 99.8, 99.8],
        [-0.5, -0.5, 0.0, 0.0],
        [0.8, 0.6, 0.8, 0.8],
        0.0,
        false,
    );
    assert!((slide_wear[0] - 0.2).abs() < 0.001);
    assert!((slide_wear[1] - 0.1).abs() < 0.001);
    assert_eq!(slide_wear[2], 0.0);
    assert_eq!(slide_wear[3], 0.0);
}

#[test]
fn flat_spot_wear_uses_braking_as_a_fallback_when_sliding_fraction_is_missing() {
    let mut tracker = TireWearTracker::default();
    tracker.update([100.0; 4], [0.0; 4], [0.0; 4], 0.0, false);

    let lock_wear = tracker.update(
        [99.8, 99.9, 100.0, 100.0],
        [-0.5, -0.5, 0.0, 0.0],
        [0.0; 4],
        0.7,
        false,
    );

    assert!((lock_wear[0] - 0.2).abs() < 0.001);
    assert!((lock_wear[1] - 0.1).abs() < 0.001);
    assert_eq!(lock_wear[2], 0.0);
    assert_eq!(lock_wear[3], 0.0);
}

#[test]
fn flat_spot_wear_resets_when_tyres_are_changed_in_pits() {
    let mut tracker = TireWearTracker::default();
    tracker.update([90.0; 4], [0.0; 4], [0.0; 4], 0.0, false);
    tracker.update([89.5; 4], [-0.5; 4], [0.8; 4], 0.0, false);

    let after_change = tracker.update([100.0; 4], [0.0; 4], [0.0; 4], 0.0, true);
    assert_eq!(after_change, [0.0; 4]);
}

#[test]
fn tire_life_uses_clean_lap_wear_and_the_limiting_wheel() {
    let mut tracker = TireWearTracker::default();
    tracker.observe_lap([100.0; 4], true, false);
    tracker.observe_lap([98.0, 99.0, 99.0, 99.0], true, true);

    let model = tracker.life_model([98.0, 99.0, 99.0, 99.0], 10.0).unwrap();
    assert_eq!(model.remaining_laps, 49.0);
    assert_eq!(model.remaining_stints, 4.9);
    assert_eq!(model.projected_remaining_percent, [78.0, 58.0, 38.0]);

    tracker.update([98.0, 99.0, 99.0, 99.0], [0.0; 4], [0.0; 4], 0.0, false);
    tracker.update([100.0; 4], [0.0; 4], [0.0; 4], 0.0, true);
    assert!(tracker.life_model([100.0; 4], 10.0).is_none());
}

#[test]
fn suspension_damage_keeps_each_wheel_independent() {
    let damage = [0.02, 0.18, 0.51, 1.4];

    assert_eq!(
        suspension_damage_by_wheel_percent(Some(damage), [0, 1, 0, 0]),
        [2.0, 18.0, 51.0, 100.0]
    );
    assert_eq!(
        suspension_damage_by_wheel_percent(None, [0, 0, 1, 0]),
        [-1.0, -1.0, 100.0, -1.0]
    );
    assert_eq!(suspension_damage_percent(Some(damage), [0, 1, 0, 0]), 100.0);
    assert_eq!(suspension_damage_percent(None, [0, 1, 0, 0]), 100.0);
    assert_eq!(suspension_damage_percent(None, [0; 4]), -1.0);
}

#[test]
fn rear_wing_loss_follows_the_detached_body_part_flag() {
    assert!(rear_wing_detached(true));
    assert!(!rear_wing_detached(false));
}
use std::ffi::c_char;
use std::time::{Duration, Instant};

fn set_chars<const N: usize>(target: &mut [c_char; N], value: &str) {
    *target = [0; N];
    for (destination, source) in target.iter_mut().zip(value.as_bytes()) {
        *destination = *source as c_char;
    }
}

fn rejoin_snapshot() -> LmuSnapshot {
    let mut snapshot = LmuSnapshot {
        standings_count: 2,
        speed_kph: 100.0,
        track_length: 5_000.0,
        ..LmuSnapshot::default()
    };
    snapshot.standings[0] = LmuStandingEntry {
        vehicle_id: 10,
        position: 1,
        is_player: 1,
        speed_kph: 100.0,
        lap_distance: 1_000.0,
        time_into_lap: 20.0,
        estimated_lap_time: 100.0,
        ..LmuStandingEntry::default()
    };
    snapshot.standings[1] = LmuStandingEntry {
        vehicle_id: 20,
        position: 2,
        speed_kph: 200.0,
        lap_distance: 500.0,
        time_into_lap: 10.0,
        estimated_lap_time: 100.0,
        ..LmuStandingEntry::default()
    };
    snapshot
}

#[test]
fn vehicle_identity_cache_updates_on_driver_swap_and_resets_with_session() {
    let mut source = LmuTelemetrySource::new();
    let mut entry = LmuStandingEntry {
        vehicle_id: 29,
        ..LmuStandingEntry::default()
    };
    set_chars(&mut entry.driver_name, "Primer Piloto");
    set_chars(&mut entry.vehicle_class, "LMGT3");
    set_chars(&mut entry.vehicle_name, "Ferrari 296 #29");

    source.ensure_vehicle_identity(&entry);
    assert_eq!(source.vehicle_identities[&29].driver_name, "Primer Piloto");
    assert_eq!(source.vehicle_identities[&29].fallback_car_number, "29");

    set_chars(&mut entry.driver_name, "Segundo Piloto");
    source.ensure_vehicle_identity(&entry);
    assert_eq!(source.vehicle_identities[&29].driver_name, "Segundo Piloto");

    source.scored_finish_positions.insert(29, 2);
    source.update_session(1);
    assert!(source.vehicle_identities.is_empty());
    assert!(source.scored_finish_positions.is_empty());
}

#[test]
fn spectator_resolves_rest_focus_by_driver_identity() {
    let mut source = LmuTelemetrySource::new();
    source.local_rest.seed_standings(vec![RestStanding {
        slot_id: 99,
        driver_name: "Ricardo Fernandez#9006".into(),
        focus: true,
        has_focus: true,
        ..RestStanding::default()
    }]);
    let mut snapshot = LmuSnapshot {
        standings_count: 2,
        ..LmuSnapshot::default()
    };
    snapshot.standings[0].vehicle_id = 20;
    set_chars(&mut snapshot.standings[0].driver_name, "Otro piloto");
    snapshot.standings[1].vehicle_id = 21;
    set_chars(
        &mut snapshot.standings[1].driver_name,
        "RICARDO FERNANDEZ#9006",
    );
    source.last_valid_snapshot = Some(snapshot);

    assert_eq!(source.spectator_vehicle_id(), Some(21));
}

#[test]
fn team_mode_resolves_registered_team_independently_of_focus() {
    let mut source = LmuTelemetrySource::new();
    source.local_rest.seed_team_reference(
        vec!["Manuel Rodriguez Alvarez".into(), "Compañero Equipo".into()],
        "BlackRack Racing",
        "Ferrari 296 #29",
    );
    let mut snapshot = LmuSnapshot {
        standings_count: 2,
        ..LmuSnapshot::default()
    };
    snapshot.standings[0].vehicle_id = 20;
    set_chars(&mut snapshot.standings[0].driver_name, "Piloto observado");
    snapshot.standings[1].vehicle_id = 29;
    set_chars(
        &mut snapshot.standings[1].driver_name,
        "COMPAÑERO EQUIPO#9006",
    );
    source.last_valid_snapshot = Some(snapshot);

    assert_eq!(source.team_vehicle_id(), Some(29));
}

#[test]
fn maps_each_wheel_compound_and_deduplicates_the_summary() {
    assert_eq!(
        LmuTelemetrySource::tire_compounds(&[0, 1, 2, 3], ("", "")),
        ["S", "M", "H", "W"]
    );
    assert_eq!(
        LmuTelemetrySource::tire_compound(&[1, 2, 1, 2], ("", "")),
        "M/H"
    );
}

/// The compound the game names on each axle answers before anything else, so a
/// car whose axle names read Medium keeps its mediums.
#[test]
fn the_names_the_game_publishes_decide_the_compound() {
    assert_eq!(
        LmuTelemetrySource::tire_compounds(&[3, 3, 3, 3], ("Medium", "Medium")),
        ["M", "M", "M", "M"]
    );
}

/// Each axle is named on its own, and shared memory orders the wheels front
/// left, front right, rear left, rear right.
#[test]
fn front_and_rear_axles_keep_their_own_compound() {
    assert_eq!(
        LmuTelemetrySource::tire_compounds(&[0; 4], ("Soft", "Medium")),
        ["S", "S", "M", "M"]
    );
    assert_eq!(
        LmuTelemetrySource::tire_compound(&[0; 4], ("Soft", "Medium")),
        "S/M"
    );
}

/// A wet name wins over the dry ladder, so an intermediate or a `Medium Wet` is
/// never reported as a medium slick.
#[test]
fn wet_names_are_read_before_the_dry_ladder() {
    assert_eq!(
        LmuTelemetrySource::tire_compounds(&[1; 4], ("Medium Wet", "Medium Wet")),
        ["W", "W", "W", "W"]
    );
    assert_eq!(
        LmuTelemetrySource::tire_compounds(&[1; 4], ("Intermediate", "Intermediate")),
        ["I", "I", "I", "I"]
    );
}

/// A name that spells out no compound, such as a bare class name, leaves the
/// enum in charge.
#[test]
fn a_name_without_a_compound_leaves_the_enum_in_charge() {
    assert_eq!(
        LmuTelemetrySource::tire_compounds(&[1; 4], ("GTE", "GTE")),
        ["M", "M", "M", "M"]
    );
}

/// `mCompoundType` is the SDK's soft/medium/hard/wet enum, not an index into the
/// garage list. An LMP2 carrying only Medium and Wet still reports 1 for its
/// mediums, and reading that as the list's second entry called them wets.
#[test]
fn the_compound_enum_is_not_an_index_into_the_garage_list() {
    let conditions = [
        RestCompoundCondition {
            compound_type: "Medium".into(),
            optimal_temperature: 89.0,
        },
        RestCompoundCondition {
            compound_type: "Wet".into(),
            optimal_temperature: 52.0,
        },
    ];

    assert_eq!(
        LmuTelemetrySource::tire_compounds(&[1; 4], ("", "")),
        ["M", "M", "M", "M"]
    );
    assert_eq!(
        LmuTelemetrySource::tire_optimal_temperatures(&[1, 1, 3, 3], ("", ""), &conditions),
        [89.0, 89.0, 52.0, 52.0]
    );
    // A compound the list does not describe stays unknown instead of borrowing a
    // neighbour's optimum.
    assert_eq!(
        LmuTelemetrySource::tire_optimal_temperatures(&[2; 4], ("", ""), &conditions),
        [-1.0; 4]
    );
    assert_eq!(
        LmuTelemetrySource::tire_optimal_temperatures(&[1; 4], ("", ""), &[]),
        [-1.0; 4]
    );
}

/// Names such as "LMP2 - Hard" resolve by the compound word rather than the
/// class prefix, on the axle name and on the garage list alike.
#[test]
fn compound_words_are_read_past_a_class_prefix() {
    let conditions = [RestCompoundCondition {
        compound_type: "LMP2 - Hard".into(),
        optimal_temperature: 100.0,
    }];

    assert_eq!(
        LmuTelemetrySource::tire_compounds(&[2; 4], ("LMP2 - Hard", "LMP2 - Hard")),
        ["H", "H", "H", "H"]
    );
    assert_eq!(
        LmuTelemetrySource::tire_optimal_temperatures(&[2; 4], ("", ""), &conditions),
        [100.0; 4]
    );
}

#[test]
fn converts_rank_progress_to_a_continuous_rating_score() {
    assert_eq!(
        LmuTelemetrySource::driver_rank_score("B3", 78.0),
        Some(378.0)
    );
    assert_eq!(
        LmuTelemetrySource::driver_rank_score("S1", 52.0),
        Some(452.0)
    );
    assert_eq!(
        LmuTelemetrySource::driver_rank_score("P3", 100.0),
        Some(1300.0)
    );
    assert_eq!(LmuTelemetrySource::driver_rank_score("", 50.0), None);
}

#[test]
fn scored_class_positions_require_complete_category_coverage() {
    let entries = vec![
        StandingEntry {
            vehicle_id: 1,
            vehicle_class: "GT3".to_owned(),
            position: 1,
            ..StandingEntry::default()
        },
        StandingEntry {
            vehicle_id: 2,
            vehicle_class: "GT3".to_owned(),
            position: 2,
            ..StandingEntry::default()
        },
        StandingEntry {
            vehicle_id: 3,
            vehicle_class: "GT3".to_owned(),
            position: 3,
            ..StandingEntry::default()
        },
    ];

    let partial =
        LmuTelemetrySource::scored_class_positions(&entries, &HashMap::from([(1, 1), (2, 3)]));
    assert!(partial.is_empty());

    let complete = LmuTelemetrySource::scored_class_positions(
        &entries,
        &HashMap::from([(1, 1), (2, 3), (3, 2)]),
    );
    assert_eq!(complete, HashMap::from([(1, 1), (2, 3), (3, 2)]));
}

#[test]
fn qualifying_grid_converts_complete_overall_order_and_never_mixes_partial_rest() {
    let entries = vec![
        StandingEntry {
            vehicle_id: 1,
            vehicle_class: "HYPERCAR".to_owned(),
            position: 1,
            ..StandingEntry::default()
        },
        StandingEntry {
            vehicle_id: 2,
            vehicle_class: "HYPERCAR".to_owned(),
            position: 2,
            ..StandingEntry::default()
        },
        StandingEntry {
            vehicle_id: 3,
            vehicle_class: "LMP2".to_owned(),
            position: 1,
            ..StandingEntry::default()
        },
        StandingEntry {
            vehicle_id: 4,
            vehicle_class: "LMP2".to_owned(),
            position: 2,
            ..StandingEntry::default()
        },
        StandingEntry {
            vehicle_id: 5,
            vehicle_class: "LMP2".to_owned(),
            position: 3,
            ..StandingEntry::default()
        },
    ];
    let fallback = HashMap::from([(1, 1), (2, 2), (3, 3), (4, 1), (5, 2)]);

    let complete = LmuTelemetrySource::class_positions_with_preferred_class_order(
        &entries,
        &LmuTelemetrySource::scored_class_positions(
            &entries,
            &HashMap::from([(1, 1), (2, 2), (3, 10), (4, 30), (5, 20)]),
        ),
        &fallback,
    );
    assert_eq!(
        complete,
        HashMap::from([(1, 1), (2, 2), (3, 1), (4, 3), (5, 2)])
    );

    let partial = LmuTelemetrySource::class_positions_with_preferred_class_order(
        &entries,
        &LmuTelemetrySource::scored_class_positions(
            &entries,
            &HashMap::from([(1, 1), (2, 2), (3, 10), (4, 30)]),
        ),
        &fallback,
    );
    assert_eq!(partial, fallback);
}

#[test]
fn latched_race_grid_ignores_a_later_reordered_qualifying_read() {
    let mut latched = HashMap::new();
    let grid = HashMap::from([(1, 1), (2, 2), (3, 3)]);

    LmuTelemetrySource::latch_race_qualifying_positions(&mut latched, &grid);
    assert_eq!(latched, grid);

    // Una actualización de RaceControl puede llegar completa pero con la
    // parrilla reordenada. La lectura original manda.
    LmuTelemetrySource::latch_race_qualifying_positions(
        &mut latched,
        &HashMap::from([(1, 3), (2, 1), (3, 2)]),
    );
    assert_eq!(latched, grid);

    // Un coche que no se había visto todavía sí entra.
    LmuTelemetrySource::latch_race_qualifying_positions(&mut latched, &HashMap::from([(4, 4)]));
    assert_eq!(latched, HashMap::from([(1, 1), (2, 2), (3, 3), (4, 4)]));
}

#[test]
fn driver_rank_estimate_prefers_complete_server_scored_finish_order() {
    let entries = vec![
        StandingEntry {
            vehicle_id: 1,
            position: 1,
            vehicle_class: "GT3".to_owned(),
            driver_rank: "S1".to_owned(),
            driver_rank_progress: 0.0,
            ..StandingEntry::default()
        },
        StandingEntry {
            vehicle_id: 2,
            position: 2,
            vehicle_class: "GT3".to_owned(),
            driver_rank: "S1".to_owned(),
            driver_rank_progress: 0.0,
            is_player: true,
            ..StandingEntry::default()
        },
        StandingEntry {
            vehicle_id: 3,
            position: 3,
            vehicle_class: "GT3".to_owned(),
            driver_rank: "S1".to_owned(),
            driver_rank_progress: 0.0,
            ..StandingEntry::default()
        },
    ];
    let rank_scores = HashMap::from([(1, 400.0), (2, 400.0), (3, 400.0)]);
    let qualifying_positions = HashMap::from([(1, 1), (2, 2), (3, 3)]);
    let settings = DriverRankSettings::default();

    let mut live_entries = entries.clone();
    let live = LmuTelemetrySource::update_driver_rank_estimates(
        &mut live_entries,
        &rank_scores,
        &qualifying_positions,
        &HashMap::new(),
        10,
        settings,
    )
    .unwrap();

    let mut scored_entries = entries;
    let scored = LmuTelemetrySource::update_driver_rank_estimates(
        &mut scored_entries,
        &rank_scores,
        &qualifying_positions,
        &HashMap::from([(1, 1), (2, 3), (3, 2)]),
        10,
        settings,
    )
    .unwrap();

    assert_eq!(live.race_position, 2);
    assert_eq!(live.opponents.len(), 2);
    assert!(live
        .opponents
        .iter()
        .all(|opponent| (opponent.expected - 0.5).abs() < f64::EPSILON));
    assert_eq!(scored.live_race_position, 2);
    assert_eq!(scored.race_position, 3);
    assert_eq!(scored.race_position_source, "rest_server_scored");
    assert!(scored.estimated_gain.unwrap() < live.estimated_gain.unwrap());
}

#[test]
fn driver_rank_log_reports_current_rank_without_estimating_before_race() {
    for session_type in [1, 5] {
        let mut entries = vec![StandingEntry {
            vehicle_id: 7,
            position: 3,
            vehicle_class: "LMP2".to_owned(),
            driver_rank: "S1".to_owned(),
            driver_rank_progress: 90.0,
            is_player: true,
            ..StandingEntry::default()
        }];

        let diagnostic = LmuTelemetrySource::update_driver_rank_estimates(
            &mut entries,
            &HashMap::from([(7, 490.0)]),
            &HashMap::new(),
            &HashMap::new(),
            session_type,
            DriverRankSettings::default(),
        )
        .unwrap();

        assert_eq!(diagnostic.status, "current_rank");
        assert_eq!(diagnostic.visual_score, Some(490.0));
        assert_eq!(diagnostic.estimated_gain, None);
        assert!(!entries[0].estimated_driver_rank_gain_available);
    }
}

#[test]
fn driver_rank_estimate_uses_pairwise_gain_normalization() {
    let settings = DriverRankSettings::default();
    let expected = LmuTelemetrySource::driver_rank_expected(600.0, 600.0, settings);
    assert_eq!(expected, 0.5);

    let gain = LmuTelemetrySource::driver_rank_gain(1.0 - expected, 1.0 - expected, 1, settings);
    assert!((gain - 8.823_529_411_764_707).abs() < 1e-9);
}

#[test]
fn driver_rank_validation_prefers_raw_elo_and_falls_back_to_visual_score() {
    let raw = LmuTelemetrySource::driver_rank_actual_gain(
        Some(1_290.0),
        Some(1_299.0),
        Some(430.0),
        Some(500.0),
    )
    .unwrap();
    assert_eq!(raw, (3.0, "raw_elo"));

    let visual =
        LmuTelemetrySource::driver_rank_actual_gain(Some(0.0), Some(0.0), Some(430.0), Some(433.0))
            .unwrap();
    assert_eq!(visual, (3.0, "visual_score"));
}

#[test]
fn maps_rest_finish_statuses_used_by_the_standings_counter() {
    assert_eq!(
        LmuTelemetrySource::rest_finish_status("FSTAT_FINISHED"),
        Some(1)
    );
    assert_eq!(LmuTelemetrySource::rest_finish_status("FSTAT_DNF"), Some(2));
    assert_eq!(LmuTelemetrySource::rest_finish_status("FSTAT_DQ"), Some(3));
    assert_eq!(LmuTelemetrySource::rest_finish_status("FSTAT_NONE"), None);
}

#[test]
fn standings_history_averages_the_last_five_plausible_completed_laps() {
    let mut history = CarHistory::default();
    let mut entry = LmuStandingEntry {
        best_lap_seconds: 100.0,
        lap_start_elapsed_seconds: 1_000.0,
        elapsed_seconds: 1_002.0,
        ..LmuStandingEntry::default()
    };

    history.update(&entry, 100.0);
    for lap in 1..=6 {
        entry.total_laps = lap;
        entry.last_lap_seconds = 100.0 + f64::from(lap);
        entry.lap_start_elapsed_seconds += entry.last_lap_seconds;
        entry.elapsed_seconds = entry.lap_start_elapsed_seconds + 2.0;
        history.update(&entry, 100.0 - f64::from(lap) * 2.0);
    }

    assert_eq!(history.recent_lap_times.len(), 5);
    assert!((history.average_lap_time() - 104.0).abs() < 0.001);
    assert!((history.average_energy_usage() - 2.0).abs() < 0.001);
}

#[test]
fn standings_delta_uses_player_minus_opponent_and_keeps_empty_slots() {
    let mut player = CarHistory::default();
    let mut opponent = CarHistory::default();
    player.delta_lap_times = [0.0, 101.0, 102.0, 103.0, 104.0];
    opponent.delta_lap_times = [100.0, 100.0, 103.0, 102.0, 105.0];

    assert_eq!(
        opponent.last_lap_delta_seconds(player.delta_lap_times()),
        [None, Some(1.0), Some(-1.0), Some(1.0), Some(-1.0)]
    );
}

#[test]
fn out_lap_stays_active_until_the_next_finish_line_crossing() {
    let mut history = CarHistory::default();
    let mut entry = LmuStandingEntry {
        total_laps: 3,
        ..LmuStandingEntry::default()
    };

    history.update(&entry, 50.0);
    entry.in_pits = 1;
    entry.pit_stops = 1;
    history.update(&entry, 50.0);
    entry.in_pits = 0;
    history.update(&entry, 49.0);
    assert!(history.is_out_lap());

    entry.total_laps = 4;
    history.update(&entry, 45.0);
    assert!(!history.is_out_lap());
}

#[test]
fn completed_pit_summary_survives_after_the_out_lap() {
    let started = Instant::now();
    let mut history = CarHistory::default();
    let mut entry = LmuStandingEntry {
        total_laps: 5,
        ..LmuStandingEntry::default()
    };

    history.update_at(&entry, 80.0, started);
    entry.in_pits = 1;
    history.update_at(&entry, 79.0, started + Duration::from_secs(2));
    history.update_at(&entry, 78.0, started + Duration::from_secs(14));
    assert_eq!(history.pit_stop_time_seconds(), Some(12.0));
    assert_eq!(history.pit_stop_lap(), None);

    entry.pit_stops = 1;
    entry.in_pits = 0;
    history.update_at(&entry, 77.0, started + Duration::from_secs(20));
    assert!(history.is_out_lap());
    assert_eq!(history.pit_stop_time_seconds(), Some(18.0));
    assert_eq!(history.pit_stop_lap(), Some(6));

    history.update_at(&entry, 76.0, started + Duration::from_secs(30));
    assert_eq!(history.pit_stop_time_seconds(), Some(18.0));

    entry.total_laps = 6;
    history.update_at(&entry, 75.0, started + Duration::from_secs(31));
    assert!(!history.is_out_lap());
    assert_eq!(history.pit_stop_time_seconds(), Some(18.0));
    assert_eq!(history.pit_stop_lap(), Some(6));
}

#[test]
fn pit_timer_is_discarded_after_an_uncounted_pit_lane_passage() {
    let started = Instant::now();
    let mut history = CarHistory::default();
    let mut entry = LmuStandingEntry {
        total_laps: 5,
        ..LmuStandingEntry::default()
    };

    history.update_at(&entry, 80.0, started);
    entry.in_pits = 1;
    history.update_at(&entry, 79.0, started + Duration::from_secs(2));
    history.update_at(&entry, 78.0, started + Duration::from_secs(14));
    assert_eq!(history.pit_stop_time_seconds(), Some(12.0));

    entry.in_pits = 0;
    history.update_at(&entry, 77.0, started + Duration::from_secs(20));
    assert!(!history.is_out_lap());
    assert_eq!(history.pit_stop_time_seconds(), None);
    assert_eq!(history.pit_stop_lap(), None);
}

#[test]
fn pit_timer_accepts_a_counter_increase_after_pit_exit() {
    let started = Instant::now();
    let mut history = CarHistory::default();
    let mut entry = LmuStandingEntry {
        total_laps: 5,
        ..LmuStandingEntry::default()
    };

    history.update_at(&entry, 80.0, started);
    entry.in_pits = 1;
    history.update_at(&entry, 79.0, started + Duration::from_secs(2));
    entry.in_pits = 0;
    history.update_at(&entry, 78.0, started + Duration::from_secs(14));
    assert_eq!(history.pit_stop_time_seconds(), None);

    entry.pit_stops = 1;
    history.update_at(&entry, 77.0, started + Duration::from_secs(15));
    assert!(history.is_out_lap());
    assert_eq!(history.pit_stop_time_seconds(), Some(12.0));
    assert_eq!(history.pit_stop_lap(), Some(6));
}

#[test]
fn pit_timer_does_not_invent_elapsed_time_when_started_mid_stop() {
    let mut history = CarHistory::default();
    let entry = LmuStandingEntry {
        total_laps: 5,
        in_pits: 1,
        ..LmuStandingEntry::default()
    };

    history.update_at(&entry, 80.0, Instant::now());

    assert_eq!(history.pit_stop_time_seconds(), None);
    assert_eq!(history.pit_stop_lap(), None);
}

#[test]
fn standings_history_uses_tinypedal_filters_for_invalid_and_pit_laps() {
    let mut history = CarHistory::default();
    let mut entry = LmuStandingEntry {
        best_lap_seconds: 100.0,
        lap_start_elapsed_seconds: 1_000.0,
        elapsed_seconds: 1_002.0,
        ..LmuStandingEntry::default()
    };

    history.update(&entry, 60.0);
    entry.lap_invalidated = 1;
    history.update(&entry, 59.5);
    entry.lap_invalidated = 0;
    entry.total_laps = 1;
    entry.last_lap_seconds = 105.0;
    entry.lap_start_elapsed_seconds = 1_105.0;
    entry.elapsed_seconds = 1_107.0;
    history.update(&entry, 59.0);

    entry.in_pits = 1;
    history.update(&entry, 58.5);
    entry.in_pits = 0;
    entry.total_laps = 2;
    entry.last_lap_seconds = 130.0;
    entry.lap_start_elapsed_seconds = 1_235.0;
    entry.elapsed_seconds = 1_237.0;
    history.update(&entry, 58.0);

    assert_eq!(history.recent_lap_times.len(), 2);
    assert!((history.average_lap_time() - 105.0).abs() < 0.001);
    assert!(history.recent_energy_usage.is_empty());
}

#[test]
fn standings_history_reconstructs_and_keeps_an_invalid_last_lap_time() {
    let mut history = CarHistory::default();
    let mut entry = LmuStandingEntry {
        total_laps: 4,
        last_lap_seconds: 95.0,
        best_lap_seconds: 95.0,
        lap_start_elapsed_seconds: 400.0,
        elapsed_seconds: 402.0,
        ..LmuStandingEntry::default()
    };

    history.update(&entry, 60.0);
    assert!(history.is_last_lap_valid());
    assert_eq!(history.last_lap_seconds(entry.last_lap_seconds), 95.0);

    entry.lap_invalidated = 1;
    history.update(&entry, 59.0);
    entry.total_laps = 5;
    entry.lap_invalidated = 0;
    entry.last_lap_seconds = 0.0;
    entry.lap_start_elapsed_seconds = 418.58;
    entry.elapsed_seconds = 419.0;
    history.update(&entry, 58.0);

    assert!(history.is_last_lap_valid());
    assert_eq!(history.last_lap_seconds(entry.last_lap_seconds), 95.0);

    entry.lap_start_elapsed_seconds = 500.25;
    entry.elapsed_seconds = 500.8;
    history.update(&entry, 58.0);
    assert!((history.last_lap_seconds(entry.last_lap_seconds) - 100.25).abs() < 0.001);
    assert!(!history.is_last_lap_valid());
    assert!(history.just_crossed_finish_line(500.25));

    entry.elapsed_seconds = 501.5;
    history.update(&entry, 58.0);

    assert!(!history.is_last_lap_valid());
    assert!((history.last_lap_seconds(entry.last_lap_seconds) - 100.25).abs() < 0.001);
    assert!((history.average_lap_time() - 100.25).abs() < 0.001);
    assert!(history.just_crossed_finish_line(500.25));
    assert!(history.just_crossed_finish_line(504.25));
    assert!(!history.just_crossed_finish_line(504.251));
}

#[test]
fn standings_history_uses_negative_official_time_as_invalid_confirmation() {
    let mut history = CarHistory::default();
    let mut entry = LmuStandingEntry {
        total_laps: 4,
        last_lap_seconds: 95.0,
        best_lap_seconds: 95.0,
        lap_start_elapsed_seconds: 400.0,
        elapsed_seconds: 402.0,
        ..LmuStandingEntry::default()
    };

    history.update(&entry, 60.0);
    entry.total_laps = 5;
    entry.last_lap_seconds = -100.25;
    entry.lap_start_elapsed_seconds = 500.25;
    entry.elapsed_seconds = 500.8;
    history.update(&entry, 59.0);
    assert!((history.last_lap_seconds(entry.last_lap_seconds) - 100.25).abs() < 0.001);
    assert!(!history.is_last_lap_valid());
    assert!(history.just_crossed_finish_line(500.25));
    entry.elapsed_seconds = 501.5;
    history.update(&entry, 59.0);

    assert!(!history.is_last_lap_valid());
    assert!((history.last_lap_seconds(entry.last_lap_seconds) - 100.25).abs() < 0.001);
    assert!(history.just_crossed_finish_line(500.25));
}

#[test]
fn standings_history_marks_a_reconstructed_missing_official_lap_invalid() {
    let mut history = CarHistory::default();
    let mut entry = LmuStandingEntry {
        total_laps: 9,
        last_lap_seconds: 101.0,
        best_lap_seconds: 101.0,
        lap_start_elapsed_seconds: 4_019.166,
        elapsed_seconds: 4_021.166,
        ..LmuStandingEntry::default()
    };

    history.update(&entry, 60.0);
    entry.total_laps = 10;
    entry.last_lap_seconds = -1.0;
    entry.lap_start_elapsed_seconds = 4_121.367;
    entry.elapsed_seconds = 4_122.867;
    history.update(&entry, 59.0);

    assert!(!history.is_last_lap_valid());
    assert!((history.last_lap_seconds(entry.last_lap_seconds) - 102.201).abs() < 0.001);
}

#[test]
fn standings_history_ignores_residual_invalid_flag_after_lap_boundary() {
    let mut history = CarHistory::default();
    let mut entry = LmuStandingEntry {
        total_laps: 4,
        last_lap_seconds: 95.0,
        best_lap_seconds: 95.0,
        lap_start_elapsed_seconds: 400.0,
        elapsed_seconds: 402.0,
        ..LmuStandingEntry::default()
    };

    history.update(&entry, 60.0);
    entry.total_laps = 5;
    entry.last_lap_seconds = 100.0;
    entry.lap_start_elapsed_seconds = 500.0;
    entry.elapsed_seconds = 501.5;
    history.update(&entry, 59.0);
    assert!(history.is_last_lap_valid());

    entry.lap_invalidated = 1;
    entry.elapsed_seconds = 502.0;
    history.update(&entry, 58.5);
    entry.lap_invalidated = 0;
    entry.elapsed_seconds = 504.0;
    history.update(&entry, 58.0);

    entry.total_laps = 6;
    entry.last_lap_seconds = 100.0;
    entry.lap_start_elapsed_seconds = 600.0;
    entry.elapsed_seconds = 601.5;
    history.update(&entry, 57.0);

    assert!(history.is_last_lap_valid());
}

#[test]
fn standings_history_rejects_an_impossible_stable_partial_lap() {
    let mut history = CarHistory::default();
    let mut entry = LmuStandingEntry {
        total_laps: 4,
        last_lap_seconds: 95.0,
        best_lap_seconds: 95.0,
        estimated_lap_time: 96.0,
        lap_start_elapsed_seconds: 400.0,
        elapsed_seconds: 402.0,
        ..LmuStandingEntry::default()
    };

    history.update(&entry, 60.0);
    entry.total_laps = 5;
    entry.last_lap_seconds = 100.0;
    entry.lap_start_elapsed_seconds = 500.0;
    entry.elapsed_seconds = 501.5;
    history.update(&entry, 59.5);
    assert_eq!(history.delta_lap_times[4], 100.0);

    entry.total_laps = 6;
    entry.last_lap_seconds = 0.0;
    entry.lap_start_elapsed_seconds = 509.7;
    entry.elapsed_seconds = 511.0;
    history.update(&entry, 59.0);

    assert!(history.is_last_lap_valid());
    assert_eq!(history.last_lap_seconds(entry.last_lap_seconds), 0.0);
    assert_eq!(history.recent_lap_times.back().copied(), Some(100.0));
    assert_eq!(history.delta_lap_times[4], 0.0);
    assert!(!history.just_crossed_finish_line(509.7));
    assert!(!history.just_crossed_finish_line(511.0));
}

#[test]
fn standings_history_keeps_a_short_reconstructed_invalid_lap_visible() {
    let mut history = CarHistory::default();
    let mut entry = LmuStandingEntry {
        total_laps: 4,
        last_lap_seconds: 95.0,
        best_lap_seconds: 100.0,
        estimated_lap_time: 100.0,
        lap_start_elapsed_seconds: 400.0,
        elapsed_seconds: 402.0,
        ..LmuStandingEntry::default()
    };

    history.update(&entry, 60.0);
    entry.lap_invalidated = 1;
    entry.elapsed_seconds = 430.0;
    history.update(&entry, 59.5);
    entry.total_laps = 5;
    entry.lap_invalidated = 0;
    entry.last_lap_seconds = -1.0;
    entry.lap_start_elapsed_seconds = 445.0;
    entry.elapsed_seconds = 446.5;
    history.update(&entry, 59.0);

    assert!(!history.is_last_lap_valid());
    assert_eq!(history.last_lap_seconds(entry.last_lap_seconds), 45.0);
    assert!(history.just_crossed_finish_line(445.0));
    assert!(history.just_crossed_finish_line(449.0));
    assert!(!history.just_crossed_finish_line(449.001));
}

#[test]
fn standings_extracts_the_car_number_from_lmu_names() {
    assert_eq!(
        LmuTelemetrySource::car_number("Porsche Penske #6", "Porsche_963_2024"),
        "6"
    );
    assert_eq!(
        LmuTelemetrySource::car_number("Porsche 963", "Porsche_963_006"),
        "006"
    );
}

#[test]
fn standings_uses_rest_supplements_without_overriding_shared_pit_stops() {
    let mut source = LmuTelemetrySource::new();
    source.local_rest.seed_standings(vec![RestStanding {
        slot_id: 20,
        car_number: "29".into(),
        pitstops: 3,
        pit_state: "REQUEST".into(),
        pitting: true,
        ve_fraction: 0.64,
        ..RestStanding::default()
    }]);
    let mut snapshot = LmuSnapshot {
        standings_count: 2,
        track_length: 5_000.0,
        ..LmuSnapshot::default()
    };
    snapshot.standings[0] = LmuStandingEntry {
        vehicle_id: 10,
        position: 1,
        vehicle_class_id: 0,
        total_laps: 4,
        lap_distance: 3_000.0,
        time_into_lap: 60.0,
        estimated_lap_time: 100.0,
        ..LmuStandingEntry::default()
    };
    snapshot.standings[1] = LmuStandingEntry {
        vehicle_id: 20,
        position: 2,
        vehicle_class_id: 0,
        pit_stops: 1,
        track_limits_steps: 7,
        track_limits_available: 1,
        total_laps: 4,
        lap_distance: 2_500.0,
        time_into_lap: 50.0,
        ..LmuStandingEntry::default()
    };

    let standings = source.standings(&snapshot, &HashSet::new());
    let entry = standings
        .iter()
        .find(|entry| entry.vehicle_id == 20)
        .unwrap();
    assert_eq!(entry.car_number, "29");
    assert_eq!(entry.laps_behind_leader, 0);
    assert!((entry.time_behind_leader - 10.0).abs() < 0.1);
    assert_eq!(entry.laps_behind_next, 0);
    assert!((entry.interval - 10.0).abs() < 0.1);
    assert_eq!(entry.pit_stops, 1);
    assert!(entry.pit_stop_requested);
    assert!(entry.in_pits);
    assert!((entry.virtual_energy_percent - 64.0).abs() < f64::EPSILON);
    assert_eq!(entry.track_limits_steps, Some(7));
    assert_eq!(standings[0].track_limits_steps, None);

    let stable = source.stable_standings(standings);
    assert_eq!(stable.len(), 2);
    let held_during_transient_empty = source.stable_standings(Vec::new());
    assert_eq!(held_during_transient_empty.len(), 2);

    source.clear_standings_cache();
    assert!(source.stable_standings(Vec::new()).is_empty());
}

#[test]
fn rust_snapshot_matches_cpp_bridge_layout_size() {
    assert_eq!(std::mem::size_of::<LmuSnapshot>(), unsafe {
        lmu_snapshot_size()
    });
}

#[test]
fn track_map_keeps_selected_player_without_scoring_roster() {
    let snapshot = LmuSnapshot {
        player_active: 1,
        player_lap_distance: 1_234.0,
        player_world_x: 42.0,
        player_world_y: -17.0,
        player_vehicle_id: 7,
        vehicle_class_id: 6,
        player_position: 0,
        player_total_laps: 2,
        lap_number: 2,
        ..LmuSnapshot::default()
    };

    let vehicles = LmuTelemetrySource::track_map_vehicles(&snapshot, &HashSet::new());

    assert_eq!(vehicles.len(), 1);
    let player = &vehicles[0];
    assert!(player.is_player);
    assert_eq!(player.vehicle_id, 7);
    assert_eq!(player.overall_position, 1);
    assert_eq!(player.lap_distance, 1_234.0);
    assert_eq!((player.world_x, player.world_y), (42.0, -17.0));
    assert_eq!(player.vehicle_class, "GT3");
    assert!(player.world_position_available);
}

#[test]
fn maps_explicit_tc_and_abs_activation_flags() {
    let mut snapshot = LmuSnapshot {
        tc_active: 1,
        abs_active: 0,
        ..LmuSnapshot::default()
    };
    assert_eq!(LmuTelemetrySource::driver_assists(&snapshot), (true, false));

    snapshot.tc_active = 0;
    snapshot.abs_active = 1;
    assert_eq!(LmuTelemetrySource::driver_assists(&snapshot), (false, true));
}

#[test]
fn converts_steering_fraction_to_game_wheel_degrees_and_clamps_force() {
    let snapshot = LmuSnapshot {
        steering: -0.5,
        steering_range_degrees: 900.0,
        force_feedback: 1.4,
        ..LmuSnapshot::default()
    };
    assert_eq!(
        LmuTelemetrySource::steering_and_force(&snapshot, None),
        (-225.0, 1.0)
    );
    assert_eq!(
        LmuTelemetrySource::steering_and_force(&snapshot, Some(360.0)),
        (-90.0, 1.0)
    );

    let invalid = LmuSnapshot {
        steering: f64::NAN,
        steering_range_degrees: f64::INFINITY,
        force_feedback: f64::NAN,
        ..LmuSnapshot::default()
    };
    assert_eq!(
        LmuTelemetrySource::steering_and_force(&invalid, Some(f64::NAN)),
        (0.0, 0.0)
    );
}

#[test]
fn standings_marks_slow_cars_without_waiting_for_a_sector_yellow() {
    let mut snapshot = LmuSnapshot {
        standings_count: 5,
        yellow_sectors: 0,
        ..LmuSnapshot::default()
    };
    snapshot.standings[0] = LmuStandingEntry {
        vehicle_id: 10,
        speed_kph: 28.79,
        ..LmuStandingEntry::default()
    };
    snapshot.standings[1] = LmuStandingEntry {
        vehicle_id: 20,
        speed_kph: 28.8,
        ..LmuStandingEntry::default()
    };
    snapshot.standings[2] = LmuStandingEntry {
        vehicle_id: 30,
        speed_kph: 0.0,
        in_pits: 1,
        ..LmuStandingEntry::default()
    };
    snapshot.standings[3] = LmuStandingEntry {
        vehicle_id: 40,
        speed_kph: 0.0,
        in_garage: 1,
        ..LmuStandingEntry::default()
    };
    snapshot.standings[4] = LmuStandingEntry {
        vehicle_id: 50,
        speed_kph: 0.0,
        pit_state: 2,
        ..LmuStandingEntry::default()
    };

    let slow = LmuTelemetrySource::slow_yellow_vehicles(&snapshot);
    assert_eq!(slow, HashSet::from([10, 30, 50]));
}

#[test]
fn class_gap_uses_time_into_lap_on_the_same_lap() {
    let ahead = LmuStandingEntry {
        total_laps: 8,
        lap_distance: 3_000.0,
        time_into_lap: 60.0,
        estimated_lap_time: 100.0,
        ..LmuStandingEntry::default()
    };
    let behind = LmuStandingEntry {
        total_laps: 8,
        lap_distance: 2_750.0,
        time_into_lap: 55.0,
        ..LmuStandingEntry::default()
    };
    let (laps, seconds) = LmuTelemetrySource::class_relative_gap(&ahead, &behind, 5_000.0);
    assert_eq!(laps, 0);
    assert!((seconds - 5.0).abs() < 0.001);
}

#[test]
fn class_gap_shows_laps_when_leader_fully_ahead() {
    let ahead = LmuStandingEntry {
        total_laps: 8,
        lap_distance: 4_000.0,
        ..LmuStandingEntry::default()
    };
    let behind = LmuStandingEntry {
        total_laps: 7,
        lap_distance: 4_000.0,
        ..LmuStandingEntry::default()
    };
    let (laps, seconds) = LmuTelemetrySource::class_relative_gap(&ahead, &behind, 5_000.0);
    assert_eq!(laps, 1);
    assert!((seconds - 0.0).abs() < 0.001);
}

#[test]
fn class_gap_stays_seconds_during_finish_line_transition() {
    let ahead = LmuStandingEntry {
        total_laps: 8,
        lap_distance: 100.0,
        time_into_lap: 2.0,
        estimated_lap_time: 100.0,
        ..LmuStandingEntry::default()
    };
    let behind = LmuStandingEntry {
        total_laps: 7,
        lap_distance: 4_900.0,
        time_into_lap: 98.0,
        estimated_lap_time: 100.0,
        ..LmuStandingEntry::default()
    };
    let (laps, seconds) = LmuTelemetrySource::class_relative_gap(&ahead, &behind, 5_000.0);
    assert_eq!(laps, 0);
    assert!((seconds - 4.0).abs() < 0.001);
}

#[test]
fn class_gap_corrects_negative_time_when_on_different_lap() {
    let ahead = LmuStandingEntry {
        total_laps: 8,
        lap_distance: 10.0,
        time_into_lap: 0.5,
        estimated_lap_time: 100.0,
        ..LmuStandingEntry::default()
    };
    let behind = LmuStandingEntry {
        total_laps: 7,
        lap_distance: 4_900.0,
        time_into_lap: 98.0,
        estimated_lap_time: 100.0,
        ..LmuStandingEntry::default()
    };
    let (laps, seconds) = LmuTelemetrySource::class_relative_gap(&ahead, &behind, 5_000.0);
    assert_eq!(laps, 0);
    assert!((seconds - 2.5).abs() < 0.1);
}

#[test]
fn relative_gap_uses_circular_estimated_time_into_lap() {
    let player = LmuStandingEntry {
        vehicle_id: 1,
        time_into_lap: 98.0,
        estimated_lap_time: 100.0,
        ..LmuStandingEntry::default()
    };
    let just_ahead_after_finish = LmuStandingEntry {
        vehicle_id: 2,
        total_laps: 8,
        time_into_lap: 2.0,
        ..LmuStandingEntry::default()
    };
    let just_behind = LmuStandingEntry {
        vehicle_id: 3,
        total_laps: 7,
        time_into_lap: 94.0,
        ..LmuStandingEntry::default()
    };

    assert_eq!(
        LmuTelemetrySource::relative_gaps_seconds(&player, &just_ahead_after_finish),
        (-4.0, 96.0)
    );
    assert_eq!(
        LmuTelemetrySource::relative_gaps_seconds(&player, &just_behind),
        (-96.0, 4.0)
    );
}

#[test]
fn relative_gap_ignores_completed_laps_for_lapped_traffic() {
    let player = LmuStandingEntry {
        vehicle_id: 1,
        total_laps: 10,
        time_into_lap: 50.0,
        estimated_lap_time: 100.0,
        ..LmuStandingEntry::default()
    };
    let lapped_car_ahead = LmuStandingEntry {
        vehicle_id: 2,
        total_laps: 9,
        time_into_lap: 55.0,
        ..LmuStandingEntry::default()
    };

    assert_eq!(
        LmuTelemetrySource::relative_gaps_seconds(&player, &lapped_car_ahead).0,
        -5.0
    );
}

#[test]
fn lap_relation_uses_continuous_progress_across_the_timing_line() {
    let player = LmuStandingEntry {
        vehicle_id: 1,
        total_laps: 8,
        time_into_lap: 98.0,
        estimated_lap_time: 100.0,
        ..LmuStandingEntry::default()
    };
    let just_ahead_after_finish = LmuStandingEntry {
        vehicle_id: 2,
        total_laps: 9,
        time_into_lap: 2.0,
        ..LmuStandingEntry::default()
    };
    assert_eq!(
        LmuTelemetrySource::laps_relative_to_player(&player, &just_ahead_after_finish),
        0
    );

    let player = LmuStandingEntry {
        time_into_lap: 50.0,
        ..player
    };
    let lap_ahead = LmuStandingEntry {
        time_into_lap: 60.0,
        ..just_ahead_after_finish
    };
    assert_eq!(
        LmuTelemetrySource::laps_relative_to_player(&player, &lap_ahead),
        1
    );

    let lap_behind = LmuStandingEntry {
        total_laps: 7,
        time_into_lap: 45.0,
        ..lap_ahead
    };
    assert_eq!(
        LmuTelemetrySource::laps_relative_to_player(&player, &lap_behind),
        -1
    );
}

#[test]
fn relative_omits_cars_in_the_garage() {
    let player = LmuStandingEntry {
        vehicle_id: 1,
        time_into_lap: 50.0,
        estimated_lap_time: 100.0,
        ..LmuStandingEntry::default()
    };
    let garage_car = LmuStandingEntry {
        vehicle_id: 2,
        in_garage: 1,
        time_into_lap: 55.0,
        ..LmuStandingEntry::default()
    };

    assert_eq!(
        LmuTelemetrySource::relative_gaps_seconds(&player, &garage_car),
        (0.0, 0.0)
    );
}

#[test]
fn checkered_flag_has_priority_over_blue_and_yellow() {
    let mut snapshot = LmuSnapshot {
        standings_count: 1,
        game_phase: 5,
        ..LmuSnapshot::default()
    };
    snapshot.standings[0] = LmuStandingEntry {
        vehicle_id: 10,
        position: 1,
        is_player: 1,
        finish_status: 1,
        flag: 6,
        ..LmuStandingEntry::default()
    };
    let yellow_culprits = std::collections::HashSet::from([10]);

    let warning = LmuTelemetrySource::flag_warning(&snapshot, &yellow_culprits);
    assert!(warning.active);
    assert_eq!(warning.kind, "checkered");
}

#[test]
fn blue_flag_selects_nearest_plausible_faster_car_behind() {
    let mut snapshot = LmuSnapshot {
        standings_count: 5,
        game_phase: 5,
        track_length: 5_000.0,
        ..LmuSnapshot::default()
    };
    snapshot.standings[0] = LmuStandingEntry {
        vehicle_id: 10,
        position: 8,
        is_player: 1,
        flag: 6,
        total_laps: 3,
        best_lap_seconds: 100.0,
        lap_distance: 1_000.0,
        ..LmuStandingEntry::default()
    };
    snapshot.standings[1] = LmuStandingEntry {
        vehicle_id: 20,
        position: 7,
        total_laps: 3,
        best_lap_seconds: 100.0,
        lap_distance: 950.0,
        ..LmuStandingEntry::default()
    };
    snapshot.standings[2] = LmuStandingEntry {
        vehicle_id: 30,
        position: 2,
        total_laps: 4,
        best_lap_seconds: 90.0,
        lap_distance: 900.0,
        ..LmuStandingEntry::default()
    };
    snapshot.standings[3] = LmuStandingEntry {
        vehicle_id: 40,
        position: 3,
        total_laps: 3,
        best_lap_seconds: 95.0,
        lap_distance: 800.0,
        ..LmuStandingEntry::default()
    };
    snapshot.standings[4] = LmuStandingEntry {
        vehicle_id: 50,
        position: 1,
        total_laps: 4,
        best_lap_seconds: 89.0,
        lap_distance: 700.0,
        in_pits: 1,
        ..LmuStandingEntry::default()
    };
    set_chars(&mut snapshot.standings[0].vehicle_class, "LMGT3");
    set_chars(&mut snapshot.standings[1].vehicle_class, "LMGT3");
    set_chars(&mut snapshot.standings[2].vehicle_class, "HYPERCAR");
    set_chars(&mut snapshot.standings[3].vehicle_class, "HYPERCAR");
    set_chars(&mut snapshot.standings[4].vehicle_class, "HYPERCAR");

    let warning = LmuTelemetrySource::flag_warning(&snapshot, &HashSet::new());

    assert!(warning.active);
    assert_eq!(warning.kind, "blue");
    assert_eq!(warning.distance_meters, 100.0);
    assert_eq!(warning.car_position, 2);
}

#[test]
fn yellow_overlay_requires_sector_yellow_and_tinypedal_range() {
    let mut snapshot = LmuSnapshot {
        standings_count: 2,
        game_phase: 5,
        track_length: 5_000.0,
        ..LmuSnapshot::default()
    };
    snapshot.standings[0] = LmuStandingEntry {
        vehicle_id: 10,
        position: 1,
        is_player: 1,
        speed_kph: 200.0,
        lap_distance: 1_000.0,
        ..LmuStandingEntry::default()
    };
    snapshot.standings[1] = LmuStandingEntry {
        vehicle_id: 20,
        position: 2,
        speed_kph: 0.0,
        lap_distance: 1_500.0,
        ..LmuStandingEntry::default()
    };
    let slow = HashSet::from([20]);

    let no_sector_yellow = LmuTelemetrySource::flag_warning(&snapshot, &slow);
    assert!(!no_sector_yellow.active);

    snapshot.yellow_sectors = 1;
    snapshot.standings[1].lap_distance = 1_501.0;
    let too_far_ahead = LmuTelemetrySource::flag_warning(&snapshot, &slow);
    assert!(!too_far_ahead.active);

    snapshot.standings[1].lap_distance = 1_500.0;
    let at_ahead_limit = LmuTelemetrySource::flag_warning(&snapshot, &slow);
    assert!(at_ahead_limit.active);
    assert_eq!(at_ahead_limit.kind, "yellow");
    assert_eq!(at_ahead_limit.distance_meters, 500.0);

    snapshot.standings[1].lap_distance = 949.0;
    let too_far_behind = LmuTelemetrySource::flag_warning(&snapshot, &slow);
    assert!(!too_far_behind.active);

    snapshot.standings[1].lap_distance = 950.0;
    let at_behind_limit = LmuTelemetrySource::flag_warning(&snapshot, &slow);
    assert!(at_behind_limit.active);
    assert_eq!(at_behind_limit.kind, "yellow");
    assert_eq!(at_behind_limit.distance_meters, -50.0);
}

#[test]
fn rejoin_risk_uses_both_distance_and_arrival_time() {
    assert_eq!(LmuTelemetrySource::rejoin_safety(80.0, 20.0), "danger");
    assert_eq!(LmuTelemetrySource::rejoin_safety(500.0, 5.0), "danger");
    assert_eq!(LmuTelemetrySource::rejoin_safety(180.0, 20.0), "caution");
    assert_eq!(LmuTelemetrySource::rejoin_safety(500.0, 9.0), "caution");
    assert_eq!(LmuTelemetrySource::rejoin_safety(500.0, 18.0), "safe");
    assert_eq!(
        LmuTelemetrySource::rejoin_safety(50.0, f64::INFINITY),
        "safe"
    );
}

#[test]
fn rejoin_activates_in_pits_and_holds_for_ten_seconds_after_exit() {
    let mut source = LmuTelemetrySource::new();
    let mut snapshot = rejoin_snapshot();
    snapshot.standings[0].in_pits = 1;

    let warning = source.update_rejoin_warning(&snapshot);
    assert!(warning.active);
    assert_eq!(warning.reason, "pit_exit");

    snapshot.standings[0].in_pits = 0;
    for _ in 0..199 {
        assert!(source.update_rejoin_warning(&snapshot).active);
    }
    assert!(!source.update_rejoin_warning(&snapshot).active);
}

#[test]
fn rejoin_activates_for_low_speed_or_four_offroad_wheels_without_a_yellow() {
    let mut low_speed_source = LmuTelemetrySource::new();
    let mut low_speed = rejoin_snapshot();
    low_speed.speed_kph = 20.0;
    assert!(low_speed_source.update_rejoin_warning(&low_speed).active);

    let mut offroad_source = LmuTelemetrySource::new();
    let mut offroad = rejoin_snapshot();
    offroad.player_offroad_wheels = 4;
    assert!(offroad_source.update_rejoin_warning(&offroad).active);
}

#[test]
fn rejoin_hides_traffic_at_or_beyond_fifteen_seconds() {
    let mut source = LmuTelemetrySource::new();
    let mut snapshot = rejoin_snapshot();
    snapshot.speed_kph = 20.0;
    snapshot.standings[1].lap_distance = 100.0;
    snapshot.standings[1].time_into_lap = 5.0;

    assert!(!source.update_rejoin_warning(&snapshot).active);
}

#[test]
fn rejoin_prioritizes_the_rear_car_that_arrives_first() {
    let mut source = LmuTelemetrySource::new();
    let mut snapshot = rejoin_snapshot();
    snapshot.speed_kph = 20.0;
    snapshot.standings_count = 3;
    snapshot.standings[2] = LmuStandingEntry {
        vehicle_id: 30,
        position: 3,
        speed_kph: 90.0,
        lap_distance: 750.0,
        time_into_lap: 15.0,
        estimated_lap_time: 100.0,
        ..LmuStandingEntry::default()
    };

    let warning = source.update_rejoin_warning(&snapshot);
    assert!(warning.active);
    assert_eq!(warning.distance_meters, 500.0);
}

#[test]
fn normalizes_virtual_energy_fraction_and_percentage() {
    assert_eq!(LmuTelemetrySource::virtual_energy_percent(0.75), 75.0);
    assert_eq!(LmuTelemetrySource::virtual_energy_percent(75.0), 75.0);
    assert_eq!(LmuTelemetrySource::virtual_energy_percent(150.0), 100.0);
}

#[test]
fn detects_regulated_classes_from_the_sdk() {
    let mut snapshot = LmuSnapshot {
        vehicle_class_id: 0,
        ..LmuSnapshot::default()
    };
    assert!(LmuTelemetrySource::uses_virtual_energy(&snapshot));
    snapshot.vehicle_class_id = 6;
    assert!(LmuTelemetrySource::uses_virtual_energy(&snapshot));
    snapshot.vehicle_class_id = 3;
    assert!(!LmuTelemetrySource::uses_virtual_energy(&snapshot));
}

#[test]
fn uses_the_actual_initial_energy_when_the_race_starts_below_full() {
    let mut source = LmuTelemetrySource::new();
    let mut snapshot = LmuSnapshot {
        vehicle_class_id: 6,
        lap_number: 1,
        virtual_energy: 0.48,
        ..LmuSnapshot::default()
    };
    source.update_session(10);

    let (initial, _, _) = source.update_energy_estimate(&snapshot, true, false);
    source.update_fuel_estimate(&snapshot, true, false);
    assert!((initial - 48.0).abs() < 0.001);

    snapshot.lap_number = 2;
    snapshot.virtual_energy = 0.44;
    let (current, consumption, autonomy) = source.update_energy_estimate(&snapshot, true, true);

    assert!((current - 44.0).abs() < 0.001);
    assert!((consumption - 4.0).abs() < 0.001);
    assert!((autonomy - 11.0).abs() < 0.001);
}

#[test]
fn qualifying_consumption_uses_the_official_fastest_lap_and_survives_the_race() {
    let mut source = LmuTelemetrySource::new();
    let mut snapshot = LmuSnapshot::default();

    source.update_session(5);
    source.fuel_last_lap = Some(10.0);
    source.energy_last_lap = Some(8.0);
    snapshot.session_type = 5;
    snapshot.last_lap_seconds = 105.0;
    snapshot.best_lap_seconds = 105.0;
    source.update_qualifying_reference(&snapshot, true, true);
    assert_eq!(source.fuel_qualifying_lap, Some(10.0));
    assert_eq!(source.energy_qualifying_lap, Some(8.0));

    // Una vuelta más rápida pero invalidada no cambia mBestLapTime.
    source.fuel_last_lap = Some(11.0);
    source.energy_last_lap = Some(8.5);
    snapshot.last_lap_seconds = 104.0;
    source.update_qualifying_reference(&snapshot, true, true);
    assert_eq!(source.fuel_qualifying_lap, Some(10.0));
    assert_eq!(source.energy_qualifying_lap, Some(8.0));

    // La nueva mejor vuelta oficial conserva su consumo asociado.
    source.fuel_last_lap = Some(12.0);
    source.energy_last_lap = Some(9.0);
    snapshot.last_lap_seconds = 103.0;
    snapshot.best_lap_seconds = 103.0;
    source.update_qualifying_reference(&snapshot, true, true);
    assert_eq!(source.fuel_qualifying_lap, Some(12.0));
    assert_eq!(source.energy_qualifying_lap, Some(9.0));

    source.fuel_per_lap = Some(11.0);
    source.energy_per_lap = Some(8.5);
    source.update_session(10);
    assert_eq!(source.fuel_qualifying_lap, Some(12.0));
    assert_eq!(source.energy_qualifying_lap, Some(9.0));
    assert_eq!(source.fuel_per_lap, None);
    assert_eq!(source.energy_per_lap, None);

    // Las vueltas de carrera nunca sustituyen la referencia de Qualy.
    source.fuel_last_lap = Some(20.0);
    source.energy_last_lap = Some(15.0);
    snapshot.session_type = 10;
    snapshot.last_lap_seconds = 100.0;
    snapshot.best_lap_seconds = 100.0;
    source.update_qualifying_reference(&snapshot, true, true);
    assert_eq!(source.fuel_qualifying_lap, Some(12.0));
    assert_eq!(source.energy_qualifying_lap, Some(9.0));
}

#[test]
fn session_change_resets_average_and_last_but_keeps_qualifying_phases() {
    let mut source = LmuTelemetrySource::new();
    source.update_session(5);
    source.fuel_qualifying_lap = Some(12.0);
    source.energy_qualifying_lap = Some(9.0);
    source.fuel_per_lap = Some(11.0);
    source.fuel_last_lap = Some(11.5);
    source.energy_per_lap = Some(8.5);
    source.energy_last_lap = Some(8.8);

    source.update_session(6);
    assert_eq!(source.fuel_qualifying_lap, Some(12.0));
    assert_eq!(source.energy_qualifying_lap, Some(9.0));
    assert_eq!(source.fuel_per_lap, None);
    assert_eq!(source.fuel_last_lap, None);
    assert_eq!(source.energy_per_lap, None);
    assert_eq!(source.energy_last_lap, None);

    // Una nueva tanda de entrenamientos inicia un evento limpio.
    source.update_session(1);
    assert_eq!(source.fuel_qualifying_lap, None);
    assert_eq!(source.energy_qualifying_lap, None);
}

#[test]
fn disconnecting_then_reentering_the_same_session_type_resets_session_state() {
    let mut source = LmuTelemetrySource::new();
    source.update_session(10);
    source.fuel_per_lap = Some(11.0);
    source.energy_per_lap = Some(8.0);
    source.current_session = None;
    source.local_rest.reset_session_history();
    source.update_session(10);

    assert_eq!(source.fuel_per_lap, None);
    assert_eq!(source.energy_per_lap, None);
}

#[test]
fn context_changes_reset_state_even_when_session_type_stays_the_same() {
    let mut source = LmuTelemetrySource::new();
    let context = |track: &str, max_time: f64| {
        SessionContext::from_values(
            10,
            "Porsche 963".into(),
            "Porsche 963 #6".into(),
            track.into(),
            13_626.0,
            0,
            max_time,
        )
    };

    source.update_session_context(context("Circuit de la Sarthe", 3_600.0));
    source.fuel_per_lap = Some(11.0);
    source.energy_per_lap = Some(8.0);

    // LMU may keep mSession == 10 while loading another circuit or changing
    // the race limit. Those changes must not inherit the previous estimates.
    source.update_session_context(context("Sebring International Raceway", 7_200.0));

    assert_eq!(source.current_session, Some(10));
    assert_eq!(source.fuel_per_lap, None);
    assert_eq!(source.energy_per_lap, None);
}

#[test]
fn unknown_context_values_do_not_replace_known_identity() {
    let mut source = LmuTelemetrySource::new();
    source.update_session_context(SessionContext::from_values(
        10,
        "Porsche 963".into(),
        "Porsche 963 #6".into(),
        "Circuit de la Sarthe".into(),
        13_626.0,
        0,
        3_600.0,
    ));
    source.fuel_per_lap = Some(11.0);

    source.update_session_context(SessionContext::from_values(
        10,
        String::new(),
        String::new(),
        String::new(),
        0.0,
        0,
        0.0,
    ));

    assert_eq!(source.fuel_per_lap, Some(11.0));
    assert_eq!(
        source
            .current_context
            .as_ref()
            .map(|context| context.track_name.as_str()),
        Some("Circuit de la Sarthe")
    );
}

#[test]
fn a_backward_session_clock_marks_an_in_place_restart() {
    assert!(!LmuTelemetrySource::session_elapsed_regressed(
        Some(120.0),
        119.0
    ));
    assert!(LmuTelemetrySource::session_elapsed_regressed(
        Some(120.0),
        0.0
    ));
    assert!(!LmuTelemetrySource::session_elapsed_regressed(
        Some(f64::NAN),
        0.0
    ));
}

#[test]
fn pit_lap_consumption_includes_the_resource_added_during_the_lap() {
    let mut source = LmuTelemetrySource::new();
    let mut snapshot = LmuSnapshot {
        vehicle_class_id: 0,
        lap_number: 1,
        fuel_liters: 50.0,
        virtual_energy: 0.60,
        standings_count: 1,
        ..LmuSnapshot::default()
    };
    source.update_session(10);

    snapshot.standings[0].is_player = 1;

    source.update_energy_estimate(&snapshot, true, false);
    source.update_fuel_estimate(&snapshot, true, false);

    // La recarga puede llegar en varios frames mientras el coche está en boxes.
    snapshot.standings[0].in_pits = 1;
    snapshot.fuel_liters = 65.0;
    snapshot.virtual_energy = 0.75;
    source.update_energy_estimate(&snapshot, false, false);
    source.update_fuel_estimate(&snapshot, false, false);

    snapshot.fuel_liters = 70.0;
    snapshot.virtual_energy = 0.90;
    source.update_energy_estimate(&snapshot, false, false);
    source.update_fuel_estimate(&snapshot, false, false);

    // Al completar la vuelta quedan 68 L y 87 %: se consumieron 2 L y 3 %
    // aunque el valor final sea mayor que al comenzar la vuelta.
    snapshot.standings[0].in_pits = 0;
    snapshot.lap_number = 2;
    snapshot.fuel_liters = 68.0;
    snapshot.virtual_energy = 0.87;
    source.update_energy_estimate(&snapshot, true, false);
    source.update_fuel_estimate(&snapshot, true, false);

    assert!((source.fuel_last_lap.unwrap() - 2.0).abs() < 0.001);
    assert!(source.fuel_per_lap.is_none());
    assert!((source.energy_last_lap.unwrap() - 3.0).abs() < 0.001);
    assert!(source.energy_per_lap.is_none());
}

#[test]
fn standings_remaining_laps_preserves_current_lap_progress() {
    let lap_race = LmuSnapshot {
        max_laps: 100,
        player_total_laps: 95,
        ..LmuSnapshot::default()
    };
    assert_eq!(
        LmuTelemetrySource::estimated_laps_remaining(&lap_race, 0.25, 120.0),
        4.75
    );

    let timed_race = LmuSnapshot {
        max_laps: 10_000,
        session_time_remaining: 300.0,
        ..LmuSnapshot::default()
    };
    assert_eq!(
        LmuTelemetrySource::estimated_laps_remaining(&timed_race, 0.25, 100.0),
        3.75
    );
}

#[test]
fn standings_remaining_laps_uses_the_first_hybrid_finish_criterion() {
    let mut snapshot = LmuSnapshot {
        max_laps: 100,
        player_total_laps: 95,
        leader_total_laps: 99,
        leader_lap_time: 120.0,
        leader_time_into_lap: 100.0,
        session_time_remaining: 300.0,
        ..LmuSnapshot::default()
    };
    assert_eq!(
        LmuTelemetrySource::estimated_laps_remaining(&snapshot, 0.25, 120.0),
        4.75
    );

    snapshot.session_time_remaining = 10.0;
    assert_eq!(
        LmuTelemetrySource::estimated_laps_remaining(&snapshot, 0.25, 120.0),
        0.75
    );
}

#[test]
fn timed_distance_is_player_based_and_leader_adjustment_is_separate() {
    let snapshot = LmuSnapshot {
        max_laps: 10_000,
        session_time_remaining: 100.0,
        leader_lap_time: 240.0,
        leader_time_into_lap: 230.0,
        player_total_laps: 20,
        player_position: 10,
        ..LmuSnapshot::default()
    };
    let base = LmuTelemetrySource::estimated_laps_remaining(&snapshot, 0.25, 130.0);
    assert_eq!(base, 1.75);
    assert_eq!(
        LmuTelemetrySource::total_laps_estimated(&snapshot, 0.25, 130.0),
        22.0
    );
    assert_eq!(
        LmuTelemetrySource::extra_laps_estimated(&snapshot, 0.25, 130.0, 0.0),
        Some(1)
    );
    assert_eq!(
        LmuTelemetrySource::extra_laps_estimated(&snapshot, 0.25, 130.0, 80.0),
        Some(0)
    );
    assert_eq!(
        LmuTelemetrySource::estimated_laps_remaining(&snapshot, 0.25, 130.0),
        base
    );
}

#[test]
fn distance_does_not_need_a_roster_and_missing_leader_hides_only_adjustment() {
    let mut snapshot = LmuSnapshot {
        max_laps: 10_000,
        session_time_remaining: 300.0,
        player_total_laps: 20,
        ..LmuSnapshot::default()
    };
    assert_eq!(
        LmuTelemetrySource::total_laps_estimated(&snapshot, 0.25, 100.0),
        24.0
    );
    assert_eq!(
        LmuTelemetrySource::total_laps_estimated(&snapshot, 0.25, 200.0),
        22.0
    );
    assert_eq!(
        LmuTelemetrySource::extra_laps_estimated(&snapshot, 0.25, 100.0, 0.0),
        None
    );
    snapshot.player_position = 1;
    assert_eq!(
        LmuTelemetrySource::extra_laps_estimated(&snapshot, 0.25, 100.0, 80.0),
        Some(0)
    );
    assert_eq!(
        LmuTelemetrySource::total_laps_estimated(&snapshot, 0.25, 0.0),
        0.0
    );
}

#[test]
fn lap_race_keeps_official_target_for_a_lapped_player() {
    let mut snapshot = LmuSnapshot {
        max_laps: 100,
        player_total_laps: 95,
        leader_total_laps: 99,
        leader_lap_time: 120.0,
        leader_time_into_lap: 100.0,
        ..LmuSnapshot::default()
    };
    assert_eq!(
        LmuTelemetrySource::estimated_laps_remaining(&snapshot, 0.25, 180.0),
        4.75
    );
    assert_eq!(
        LmuTelemetrySource::total_laps_estimated(&snapshot, 0.25, 180.0),
        100.0
    );
    assert_eq!(
        LmuTelemetrySource::extra_laps_estimated(&snapshot, 0.25, 180.0, 0.0),
        None
    );
    snapshot.player_total_laps = 100;
    assert_eq!(
        LmuTelemetrySource::total_laps_estimated(&snapshot, 0.0, 180.0),
        100.0
    );
}

#[test]
fn shared_distance_preserves_physical_progress_at_the_line_and_finish() {
    let mut snapshot = LmuSnapshot {
        max_laps: 10_000,
        session_time_remaining: 50.0,
        player_total_laps: 20,
        ..LmuSnapshot::default()
    };
    assert_eq!(
        LmuTelemetrySource::total_laps_estimated(&snapshot, 0.99, 100.0),
        22.0
    );
    snapshot.player_total_laps = 21;
    snapshot.session_time_remaining = 49.0;
    assert_eq!(
        LmuTelemetrySource::total_laps_estimated(&snapshot, 0.0, 100.0),
        22.0
    );
    snapshot.game_phase = 8;
    snapshot.standings_count = 1;
    snapshot.standings[0].is_player = 1;
    assert_eq!(
        LmuTelemetrySource::estimated_laps_remaining(&snapshot, 0.25, 100.0),
        0.75
    );
    assert_eq!(
        LmuTelemetrySource::total_laps_estimated(&snapshot, 0.25, 100.0),
        22.0
    );
    snapshot.player_total_laps = 22;
    snapshot.standings[0].finish_status = 1;
    assert_eq!(
        LmuTelemetrySource::estimated_laps_remaining(&snapshot, 0.0, 100.0),
        0.0
    );
    assert_eq!(
        LmuTelemetrySource::total_laps_estimated(&snapshot, 0.0, 100.0),
        22.0
    );
}
