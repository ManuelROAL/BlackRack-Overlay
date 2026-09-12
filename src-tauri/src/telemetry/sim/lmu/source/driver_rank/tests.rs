use super::*;

fn snapshot(session_type: i32, game_phase: u32, elapsed: f64) -> LmuSnapshot {
    LmuSnapshot {
        session_type,
        game_phase,
        session_elapsed_seconds: elapsed,
        ..LmuSnapshot::default()
    }
}

fn split(event_id: &str, k: f64, distance: f64) -> SessionSplit {
    let mut split = SessionSplit::default();
    split.event_id = event_id.to_owned();
    split.number = 2;
    split.count = 5;
    split.driver_rank_settings = DriverRankSettings {
        k,
        distance,
        multiplier: 1.0,
        logarithm: 10.0,
    };
    split
}

fn diagnostic(gain: f64, race_position: i32) -> DriverRankEstimateDiagnostic {
    DriverRankEstimateDiagnostic {
        status: "estimated",
        vehicle_id: 1,
        vehicle_class: "GTE".into(),
        driver_rank: "S1".into(),
        driver_rank_progress: 80.0,
        visual_score: Some(480.0),
        race_position,
        live_race_position: race_position,
        race_position_source: "rest_server_scored",
        qualifying_position: 10,
        same_class_rivals: 18,
        rated_opponents: 18,
        race_result_total: 0.0,
        qualifying_result_total: 0.0,
        gain_factor: 0.0,
        estimated_gain: Some(gain),
        opponents: Vec::new(),
    }
}

fn validate(
    source: &mut LmuTelemetrySource,
    sample: &DriverRankEstimateDiagnostic,
    session_type: i32,
    game_phase: u32,
    refresh_revision: u64,
) -> Vec<serde_json::Value> {
    let mut events = Vec::new();
    source.apply_driver_rank_validation(
        DriverRankValidationInput {
            session_type,
            game_phase,
            event_id: "race-event",
            split_number: 2,
            sample: Some(sample),
            player_raw_elo: Some(0.0),
            refresh_revision,
        },
        |event| events.push(event),
    );
    events
}

#[test]
fn captured_finishes_keep_their_event_parameters_after_registration_changes() {
    // Anonymous finish inputs from the user's 2026-09-11 capture:
    // dr-estimate-1789137961387-16240.jsonl, lines 143/161 and 752/760.
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/races-2026-09-11.json")).unwrap();
    for race in fixtures.as_array().unwrap() {
        let old_split = split(
            race["name"].as_str().unwrap(),
            race["settings"]["k"].as_f64().unwrap(),
            race["settings"]["distance"].as_f64().unwrap(),
        );
        let mut next_split = split(
            "next-event",
            race["next_event_settings"]["k"].as_f64().unwrap(),
            race["next_event_settings"]["distance"].as_f64().unwrap(),
        );
        next_split.number = 1;
        next_split.count = 3;
        let mut source = LmuTelemetrySource::new();
        assert!(source
            .driver_rank_race
            .update(&snapshot(10, 0, 0.0), &old_split, 1, 1));
        assert!(!source.driver_rank_event_ready(10, 0));
        assert!(!source
            .driver_rank_race
            .update(&snapshot(10, 5, 2_000.0), &old_split, 2, 2));
        assert!(source.driver_rank_event_ready(10, 5));
        assert!(!source
            .driver_rank_race
            .update(&snapshot(10, 8, 2_600.0), &old_split, 2, 2));
        assert!(!source
            .driver_rank_race
            .update(&snapshot(10, 8, 2_601.0), &next_split, 3, 3));
        let event = source.driver_rank_event_context(10);
        assert_eq!(event, DriverRankEventContext::from(&old_split));

        let player_id = race["player"]["vehicle_id"].as_i64().unwrap() as i32;
        let mut entries = Vec::new();
        let mut scores = HashMap::new();
        let mut qualifying = HashMap::new();
        let mut finish = HashMap::new();
        for car in
            std::iter::once(&race["player"]).chain(race["opponents"].as_array().unwrap().iter())
        {
            let id = car["vehicle_id"].as_i64().unwrap() as i32;
            let position = car["race_position"].as_i64().unwrap() as i32;
            entries.push(StandingEntry {
                vehicle_id: id,
                position,
                vehicle_class: race["name"].as_str().unwrap().to_owned(),
                is_player: id == player_id,
                ..StandingEntry::default()
            });
            scores.insert(id, car["visual_score"].as_f64().unwrap());
            qualifying.insert(id, car["qualifying_position"].as_i64().unwrap() as i32);
            finish.insert(id, position);
        }
        for (settings, expected) in [
            (
                event.driver_rank_settings,
                race["expected_gain"].as_f64().unwrap(),
            ),
            (
                next_split.driver_rank_settings,
                race["contaminated_gain"].as_f64().unwrap(),
            ),
        ] {
            let sample = LmuTelemetrySource::update_driver_rank_estimates(
                &mut entries,
                &scores,
                &qualifying,
                &finish,
                10,
                settings,
            )
            .unwrap();
            assert!((sample.estimated_gain.unwrap() - expected).abs() < 1e-12);
            assert_eq!(sample.rated_opponents, sample.same_class_rivals);
        }

        // The next actual race must acquire its own parameters.
        assert!(!source
            .driver_rank_race
            .update(&snapshot(1, 0, 0.0), &next_split, 3, 3));
        assert!(source
            .driver_rank_race
            .update(&snapshot(10, 0, 0.0), &next_split, 3, 3));
        assert!(!source.driver_rank_event_ready(10, 0));
        assert!(!source
            .driver_rank_race
            .update(&snapshot(10, 3, 10.0), &next_split, 4, 4));
        assert!(source.driver_rank_event_ready(10, 3));
        assert_eq!(
            source.driver_rank_event_context(10),
            DriverRankEventContext::from(&next_split)
        );
    }
}

#[test]
fn late_event_resolution_is_allowed_only_for_the_active_race() {
    let mut state = DriverRankRaceState::default();
    assert!(state.update(&snapshot(10, 0, 0.0), &SessionSplit::default(), 0, 0));
    let mut partial = split("race-event", 60.0, 400.0);
    partial.number = 0;
    partial.count = 1;
    assert!(!state.update(&snapshot(10, 3, 10.0), &partial, 1, 1));
    let complete = split("race-event", 60.0, 400.0);
    assert!(!state.update(&snapshot(10, 5, 30.0), &complete, 2, 2));
    assert_eq!(state.event, DriverRankEventContext::from(&complete));
    // Temporary loss and another registration must not replace the race.
    assert!(!state.update(&snapshot(10, 5, 31.0), &SessionSplit::default(), 3, 3));
    assert!(!state.update(
        &snapshot(10, 5, 32.0),
        &split("next-event", 30.0, 500.0),
        4,
        4
    ));
    assert_eq!(state.event, DriverRankEventContext::from(&complete));

    let mut unresolved = DriverRankRaceState::default();
    unresolved.update(&snapshot(10, 0, 0.0), &SessionSplit::default(), 0, 0);
    unresolved.update(&snapshot(10, 8, 2_600.0), &SessionSplit::default(), 1, 1);
    unresolved.update(&snapshot(10, 8, 2_601.0), &complete, 2, 2);
    assert!(unresolved.event.event_id.is_empty());
    assert!(!unresolved.event_confirmed);
}

#[test]
fn stale_cache_and_old_inflight_responses_cannot_bind_the_next_race() {
    let cached = split("old-event", 60.0, 400.0);
    for fresh in [split("new-event", 30.0, 500.0), cached.clone()] {
        let mut source = LmuTelemetrySource::new();
        // Request 7 was already running when this race began.
        assert!(source
            .driver_rank_race
            .update(&snapshot(10, 0, 0.0), &cached, 7, 6));
        assert!(!source.driver_rank_event_ready(10, 0));
        assert!(!source
            .driver_rank_race
            .update(&snapshot(10, 3, 10.0), &cached, 7, 7));
        assert!(!source.driver_rank_event_ready(10, 3));
        // Failed/unfinished requests do not confirm the old cache either.
        assert!(!source
            .driver_rank_race
            .update(&snapshot(10, 5, 20.0), &cached, 8, 7));
        assert!(!source.driver_rank_event_ready(10, 5));
        assert!(!source
            .driver_rank_race
            .update(&snapshot(10, 5, 30.0), &fresh, 9, 9));
        assert!(source.driver_rank_event_ready(10, 5));
        assert_eq!(
            source.driver_rank_event_context(10),
            DriverRankEventContext::from(&fresh)
        );
        assert!(!source.driver_rank_event_ready(10, 9));
    }

    let mut source = LmuTelemetrySource::new();
    source
        .driver_rank_race
        .update(&snapshot(10, 0, 0.0), &cached, 7, 6);
    source.driver_rank_race.update(
        &snapshot(10, 8, 2_600.0),
        &split("next-event", 30.0, 500.0),
        8,
        8,
    );
    assert!(!source.driver_rank_event_ready(10, 8));
}

#[test]
fn reconnecting_to_another_circuit_opens_a_new_race_but_same_circuit_does_not() {
    let mut source = LmuTelemetrySource::new();
    let mut original = snapshot(10, 5, 100.0);
    for (slot, byte) in original.track_name.iter_mut().zip(b"Monza") {
        *slot = *byte as c_char;
    }
    original.track_length = 5_793.0;
    source.update_driver_rank_session(&original);
    source.race_qualifying_positions.insert(1, 10);
    source.scored_finish_positions.insert(1, 7);
    source
        .driver_rank_prerace_scores
        .insert("player".into(), 480.0);

    source.current_session = None;
    source.update_session(10);
    original.session_elapsed_seconds = 200.0;
    original.track_length += 0.1;
    source.update_driver_rank_session(&original);
    assert_eq!(source.driver_rank_race_sequence, 1);
    assert_eq!(source.race_qualifying_positions[&1], 10);
    // Missing track data during reconnection must not erase known identity.
    source.update_driver_rank_session(&snapshot(10, 5, 201.0));
    assert_eq!(source.driver_rank_race_sequence, 1);

    let mut another = snapshot(10, 5, 300.0);
    for (slot, byte) in another.track_name.iter_mut().zip(b"Sebring") {
        *slot = *byte as c_char;
    }
    another.track_length = 6_019.0;
    source.current_session = None;
    source.update_session(10);
    source.update_driver_rank_session(&another);
    assert_eq!(source.driver_rank_race_sequence, 2);
    assert!(source.race_qualifying_positions.is_empty());
    assert!(source.scored_finish_positions.is_empty());
    assert!(source.driver_rank_prerace_scores.is_empty());
}

#[test]
fn bytes_after_the_track_name_terminator_do_not_restart_dr() {
    let mut source = LmuTelemetrySource::new();
    let mut current = snapshot(10, 5, 100.0);
    for (slot, byte) in current.track_name.iter_mut().zip(b"Monza\0") {
        *slot = *byte as c_char;
    }
    current.track_name[6..].fill(1);
    source.update_driver_rank_session(&current);
    source.race_qualifying_positions.insert(1, 10);
    source.scored_finish_positions.insert(1, 7);
    current.session_elapsed_seconds = 200.0;
    current.track_name[6..].fill(2);
    source.update_driver_rank_session(&current);
    assert_eq!(source.driver_rank_race_sequence, 1);
    assert_eq!(source.race_qualifying_positions[&1], 10);
    assert_eq!(source.scored_finish_positions[&1], 7);

    // A different visible name still changes the identity, even without a
    // track-length change or a regressing clock.
    current.track_name[0] = b'X' as c_char;
    source.update_driver_rank_session(&current);
    assert_eq!(source.driver_rank_race_sequence, 2);
}

#[test]
fn stopped_frames_cannot_create_a_race_or_replace_a_pending_finish() {
    let mut source = LmuTelemetrySource::new();
    let final_sample = diagnostic(-1.768_074_481_308_993_5, 7);
    source.update_session(10);
    source.update_driver_rank_session(&snapshot(10, 0, 0.0));
    validate(&mut source, &final_sample, 10, 5, 14);
    source.update_driver_rank_session(&snapshot(10, 8, 1_240.0));
    validate(&mut source, &final_sample, 10, 8, 14);
    source.race_qualifying_positions.insert(1, 10);
    source.scored_finish_positions.insert(1, 7);

    // Reproduce phase 9, a generic reset, then the alternating positions from
    // dr-estimate-1789121225252-9620.jsonl (lines 200-208).
    for (position, gain) in [(19, -13.846_505_853_858_01), (2, 1.264_605_257_253_099)] {
        source.current_session = None;
        source.update_session(10);
        source.update_driver_rank_session(&snapshot(10, 9, 0.0));
        assert!(validate(&mut source, &diagnostic(gain, position), 10, 9, 15).is_empty());
        assert_eq!(source.driver_rank_race_sequence, 1);
        assert_eq!(source.race_qualifying_positions[&1], 10);
        assert_eq!(source.scored_finish_positions[&1], 7);
        let pending = source.driver_rank_validation.as_ref().unwrap();
        assert_eq!(pending.final_estimated_gain, final_sample.estimated_gain);
        assert_eq!(pending.final_race_position, 7);
        assert_eq!(pending.before_refresh_revision, 14);
    }

    source.update_driver_rank_session(&snapshot(1, 0, 0.0));
    validate(&mut source, &final_sample, 1, 0, 14);
    assert!(source.driver_rank_validation.is_some());
    let events = validate(&mut source, &final_sample, 1, 0, 15);
    assert!(source.driver_rank_validation.is_none());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event"], "driver_rank_validation");
    assert_eq!(events[0]["status"], "settled");
    assert_eq!(events[0]["race_sequence"], 1);
    assert_eq!(events[0]["result"]["race_position"], 7);
    assert_eq!(
        events[0]["comparison"]["estimated_gain"],
        final_sample.estimated_gain.unwrap()
    );
}

#[test]
fn launching_in_stopped_phase_does_not_create_a_final() {
    let mut source = LmuTelemetrySource::new();
    source.update_session(10);
    source.update_driver_rank_session(&snapshot(10, 9, 0.0));
    assert!(validate(&mut source, &diagnostic(-13.8, 19), 10, 9, 14).is_empty());
    assert_eq!(source.driver_rank_race_sequence, 0);
    assert!(source.driver_rank_validation.is_none());
}

#[test]
fn generic_reset_at_checkered_flag_preserves_dr_but_accepts_position_corrections() {
    let mut source = LmuTelemetrySource::new();
    source.update_session(10);
    source.update_driver_rank_session(&snapshot(10, 5, 2_000.0));
    validate(&mut source, &diagnostic(-4.18, 11), 10, 5, 12);
    source.update_driver_rank_session(&snapshot(10, 8, 2_600.0));
    validate(&mut source, &diagnostic(-4.18, 11), 10, 8, 12);
    source.scored_finish_positions.insert(1, 11);
    source.current_session = None;
    source.update_session(10);
    source.update_driver_rank_session(&snapshot(10, 8, 0.0));
    assert_eq!(source.driver_rank_race_sequence, 1);
    assert_eq!(source.scored_finish_positions[&1], 11);

    validate(&mut source, &diagnostic(-2.5, 10), 10, 8, 12);
    let final_result = source.driver_rank_validation.as_ref().unwrap();
    assert_eq!(final_result.final_race_position, 10);
    assert_eq!(final_result.final_estimated_gain, Some(-2.5));
}

#[test]
fn a_real_restart_opens_one_new_race_and_clears_previous_race_inputs() {
    for stopped in [false, true] {
        let mut source = LmuTelemetrySource::new();
        source.update_driver_rank_session(&snapshot(10, 5, 300.0));
        source.scored_finish_positions.insert(1, 7);
        source.race_qualifying_positions.insert(1, 10);
        source
            .driver_rank_prerace_scores
            .insert("player".into(), 480.0);
        if stopped {
            source.update_driver_rank_session(&snapshot(10, 9, 0.0));
        }
        source.update_driver_rank_session(&snapshot(10, 0, 0.0));
        assert_eq!(source.driver_rank_race_sequence, 2);
        assert!(source.scored_finish_positions.is_empty());
        assert!(source.race_qualifying_positions.is_empty());
        assert!(source.driver_rank_prerace_scores.is_empty());
        source.update_driver_rank_session(&snapshot(10, 0, 0.1));
        assert_eq!(source.driver_rank_race_sequence, 2);
    }
}
