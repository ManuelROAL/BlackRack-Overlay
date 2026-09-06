//! Session phase, lap pace and the remaining-laps projections built on them.

use super::*;

impl LmuTelemetrySource {
    pub(super) fn is_qualifying(session_type: i32) -> bool {
        (5..=8).contains(&session_type)
    }

    pub(super) fn clear_qualifying_reference(&mut self) {
        self.fuel_qualifying_lap = None;
        self.energy_qualifying_lap = None;
        self.qualifying_reference_time = None;
    }

    pub(super) fn update_session(&mut self, session_type: i32) {
        if self.current_session == Some(session_type) {
            return;
        }

        let previous_session = self.current_session.replace(session_type);
        if (10..=13).contains(&session_type)
            && previous_session.is_none_or(|previous| !(10..=13).contains(&previous))
        {
            self.driver_rank_race_sequence = self.driver_rank_race_sequence.saturating_add(1);
            self.race_qualifying_positions.clear();
        }
        self.last_lap = -1;
        self.fuel_at_lap_start = None;
        self.fuel_previous_sample = None;
        self.fuel_added_this_lap = 0.0;
        self.fuel_per_lap = None;
        self.fuel_clean_laps = 0;
        self.fuel_last_lap = None;
        self.energy_at_lap_start = None;
        self.energy_previous_sample = None;
        self.energy_added_this_lap = 0.0;
        self.energy_per_lap = None;
        self.energy_clean_laps = 0;
        self.energy_last_lap = None;
        self.lap_time_pace = None;
        self.lap_visited_pits = false;
        self.lap_was_formation = false;
        self.lap_was_valid = true;
        self.lap_was_green = true;
        self.tire_wear_tracker.reset();
        self.consumption_profiler.reset_lap();
        self.car_histories.clear();
        self.starting_positions.clear();
        self.scored_finish_positions.clear();
        if previous_session.is_some() {
            self.local_rest.reset_session_history();
        }
        self.vehicle_identities.clear();
        self.driver_ranks.begin_session();
        self.rejoin_hold_frames = 0;
        self.last_gap_log_at = None;
        self.last_standings_state_update = None;
        self.last_valid_standings.clear();
        self.last_valid_standings_at = None;
        self.player_lap_times.reset();

        let entered_qualifying = Self::is_qualifying(session_type)
            && previous_session.is_none_or(|previous| !Self::is_qualifying(previous));
        let entered_practice = (0..=4).contains(&session_type);
        if entered_qualifying || entered_practice {
            self.clear_qualifying_reference();
            self.driver_rank_prerace_scores.clear();
        }
    }

    pub(super) fn player_in_pits(snapshot: &LmuSnapshot) -> bool {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        snapshot.standings[..count]
            .iter()
            .any(|entry| entry.is_player != 0 && entry.in_pits != 0)
    }

    pub(super) fn player_pit_stop_requested(snapshot: &LmuSnapshot) -> bool {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        snapshot.standings[..count]
            .iter()
            .any(|entry| entry.is_player != 0 && entry.pit_state == 1)
    }

    pub(super) fn update_qualifying_reference(
        &mut self,
        snapshot: &LmuSnapshot,
        lap_changed: bool,
        completed_is_clean: bool,
    ) {
        if !lap_changed
            || !completed_is_clean
            || !Self::is_qualifying(snapshot.session_type)
            || snapshot.last_lap_seconds <= 0.0
            || (self.fuel_last_lap.is_none() && self.energy_last_lap.is_none())
        {
            return;
        }

        // mBestLapTime es la referencia oficial de la sesión y no se actualiza
        // con una vuelta invalidada. Admitimos una pequeña tolerancia de lectura.
        if snapshot.best_lap_seconds > 0.0
            && (snapshot.last_lap_seconds - snapshot.best_lap_seconds).abs() > 0.05
        {
            return;
        }

        let should_update = self
            .qualifying_reference_time
            .map(|reference| snapshot.last_lap_seconds < reference)
            .unwrap_or(true);

        if should_update {
            self.qualifying_reference_time = Some(snapshot.last_lap_seconds);
            if let Some(consumption) = self.fuel_last_lap {
                self.fuel_qualifying_lap = Some(consumption);
            }
            if let Some(consumption) = self.energy_last_lap {
                self.energy_qualifying_lap = Some(consumption);
            }
        }
    }

    pub(super) fn time_to_next_crossing(lap_time: f64, time_into_lap: f64) -> Option<f64> {
        if !lap_time.is_finite() || lap_time <= 0.0 {
            return None;
        }
        let elapsed = time_into_lap.clamp(0.0, lap_time);
        Some((lap_time - elapsed).max(0.001))
    }

    pub(super) fn leader_finish_delay(snapshot: &LmuSnapshot) -> Option<f64> {
        let leader_lap = snapshot.leader_lap_time;
        let next_crossing = Self::time_to_next_crossing(leader_lap, snapshot.leader_time_into_lap)?;

        if snapshot.max_laps > 0 && snapshot.max_laps < 10_000 {
            let crossings = (snapshot.max_laps - snapshot.leader_total_laps).max(0);
            if crossings == 0 {
                return Some(0.0);
            }
            return Some(next_crossing + (crossings - 1) as f64 * leader_lap);
        }

        let clock = snapshot.session_time_remaining.max(0.0);
        if clock <= next_crossing {
            return Some(next_crossing);
        }

        let full_laps_after_next = ((clock - next_crossing) / leader_lap).ceil();
        Some(next_crossing + full_laps_after_next * leader_lap)
    }

    pub(super) fn initial_player_lap_pace(snapshot: &LmuSnapshot) -> Option<f64> {
        [
            snapshot.best_lap_seconds,
            snapshot.last_lap_seconds,
            snapshot.estimated_lap_time,
        ]
        .into_iter()
        .find(|value| value.is_finite() && *value > 0.0)
    }

    pub(super) fn update_player_lap_pace(
        &mut self,
        snapshot: &LmuSnapshot,
        completed_is_clean: bool,
    ) {
        if self.lap_time_pace.is_none() {
            self.lap_time_pace = Self::initial_player_lap_pace(snapshot);
        }
        if !completed_is_clean || snapshot.last_lap_seconds <= 0.0 {
            return;
        }

        let completed = snapshot.last_lap_seconds;
        self.lap_time_pace = Some(match self.lap_time_pace {
            None => completed,
            Some(previous) if completed < previous => completed,
            // TinyPedal usa seis muestras (factor EMA 2/7) y limita a cinco
            // segundos el aumento producido por una sola vuelta válida.
            Some(previous) => (previous + (2.0 / 7.0) * (completed - previous)).min(previous + 5.0),
        });
    }

    pub(super) fn estimated_laps_remaining(
        snapshot: &LmuSnapshot,
        lap_progress: f64,
        lap_pace: f64,
    ) -> f64 {
        if Self::player_finished(snapshot) {
            return 0.0;
        }

        let progress = lap_progress.clamp(0.0, 1.0);
        if snapshot.game_phase >= 8 {
            return 1.0 - progress;
        }
        let finite_lap_target = snapshot.max_laps > 0 && snapshot.max_laps < 10_000;
        let target_finishes_first = finite_lap_target
            && (snapshot.session_time_remaining <= 0.0
                || Self::leader_finish_delay(snapshot)
                    .is_none_or(|delay| delay <= snapshot.session_time_remaining));
        if target_finishes_first {
            return (f64::from((snapshot.max_laps - snapshot.player_total_laps).max(0)) - progress)
                .max(0.0);
        }

        if !lap_pace.is_finite() || lap_pace <= 0.0 {
            return 0.0;
        }
        let laps_at_timer = if snapshot.session_time_remaining > 0.0 {
            snapshot.session_time_remaining / lap_pace + progress
        } else {
            progress
        };
        (laps_at_timer.ceil() - progress).max(0.0)
    }

    pub(super) fn player_finished(snapshot: &LmuSnapshot) -> bool {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        snapshot.standings[..count]
            .iter()
            .any(|entry| entry.is_player != 0 && entry.finish_status != 0)
    }

    pub(super) fn total_laps_estimated(snapshot: &LmuSnapshot, progress: f64, pace: f64) -> f64 {
        if Self::player_finished(snapshot) {
            return snapshot.player_total_laps.max(0) as f64;
        }
        if snapshot.game_phase >= 8 {
            return snapshot.player_total_laps.saturating_add(1).max(0) as f64;
        }
        let remaining = Self::estimated_laps_remaining(snapshot, progress, pace);
        if remaining <= 0.0 {
            if snapshot.max_laps > 0
                && snapshot.max_laps < 10_000
                && snapshot.player_total_laps >= snapshot.max_laps
            {
                return snapshot.max_laps as f64;
            }
            return 0.0;
        }
        snapshot.player_total_laps.max(0) as f64 + progress.clamp(0.0, 1.0) + remaining
    }

    /// Informational only: never feed this correction back into resource requirements.
    pub(super) fn extra_laps_estimated(
        snapshot: &LmuSnapshot,
        progress: f64,
        pace: f64,
        pit_seconds: f64,
    ) -> Option<i32> {
        if (snapshot.max_laps > 0 && snapshot.max_laps < 10_000)
            || snapshot.game_phase >= 8
            || Self::player_finished(snapshot)
            || !pace.is_finite()
            || pace <= 0.0
        {
            return None;
        }
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let leader_finished = snapshot.standings[..count]
            .iter()
            .any(|entry| entry.position == 1 && entry.finish_status != 0);
        if snapshot.player_position == 1 || leader_finished {
            return Some(0);
        }
        let leader_finish = Self::leader_finish_delay(snapshot)?;
        let progress = progress.clamp(0.0, 1.0);
        let clock = snapshot.session_time_remaining.max(0.0);
        let crossings = |seconds: f64| (seconds.max(0.0) / pace + progress).ceil();
        let base = crossings(clock);
        let leader_gain = crossings(leader_finish) - base;
        let pit_gain = crossings(clock - pit_seconds.max(0.0)) - base;
        Some((leader_gain + pit_gain) as i32)
    }
}
