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

    pub(super) fn player_crossings_until_finish(
        snapshot: &LmuSnapshot,
        finish_delay: f64,
        player_lap: f64,
    ) -> Option<f64> {
        let next_crossing = Self::time_to_next_crossing(player_lap, snapshot.player_time_into_lap)?;

        if finish_delay <= next_crossing {
            return Some(1.0);
        }

        let crossings_after_next = ((finish_delay - next_crossing - 0.001) / player_lap)
            .ceil()
            .max(0.0);
        Some(1.0 + crossings_after_next)
    }

    pub(super) fn player_class_leader(snapshot: &LmuSnapshot) -> Option<&LmuStandingEntry> {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let standings = &snapshot.standings[..count];
        let player = standings.iter().find(|entry| entry.is_player != 0)?;
        standings
            .iter()
            .filter(|entry| entry.position > 0 && entry.vehicle_class == player.vehicle_class)
            .min_by_key(|entry| entry.position)
    }

    pub(super) fn standing_reference_lap(entry: &LmuStandingEntry) -> Option<f64> {
        [
            entry.estimated_lap_time,
            entry.last_lap_seconds,
            entry.best_lap_seconds,
        ]
        .into_iter()
        .find(|value| value.is_finite() && *value > 0.0)
    }

    pub(super) fn total_laps_estimated(snapshot: &LmuSnapshot) -> f64 {
        if snapshot.game_phase >= 8 {
            if let Some(class_leader) = Self::player_class_leader(snapshot) {
                return if class_leader.finish_status != 0 {
                    class_leader.total_laps.max(0) as f64
                } else {
                    class_leader.total_laps.saturating_add(1).max(0) as f64
                };
            }
            return if Self::player_finished(snapshot) {
                snapshot.player_total_laps.max(0) as f64
            } else {
                snapshot.player_total_laps.saturating_add(1).max(0) as f64
            };
        }
        if snapshot.max_laps > 0 && snapshot.max_laps < 10_000 {
            return snapshot.max_laps as f64;
        }

        let Some(finish_delay) = Self::leader_finish_delay(snapshot) else {
            return 0.0;
        };
        if finish_delay <= 0.0 {
            return 0.0;
        }

        let (completed_laps, lap_time, time_into_lap) =
            if let Some(class_leader) = Self::player_class_leader(snapshot) {
                if class_leader.finish_status != 0 {
                    return class_leader.total_laps.max(0) as f64;
                }
                let Some(lap_time) = Self::standing_reference_lap(class_leader) else {
                    return 0.0;
                };
                (
                    class_leader.total_laps,
                    lap_time,
                    class_leader.time_into_lap,
                )
            } else {
                (
                    snapshot.leader_total_laps,
                    snapshot.leader_lap_time,
                    snapshot.leader_time_into_lap,
                )
            };
        let Some(next_crossing) = Self::time_to_next_crossing(lap_time, time_into_lap) else {
            return 0.0;
        };
        let crossings = if finish_delay <= next_crossing {
            1.0
        } else {
            1.0 + ((finish_delay - next_crossing - 0.001) / lap_time)
                .ceil()
                .max(0.0)
        };

        completed_laps.max(0) as f64 + crossings
    }

    pub(super) fn laps_remaining_after_delay(
        snapshot: &LmuSnapshot,
        player_lap: f64,
        delay_seconds: f64,
    ) -> f64 {
        if snapshot.game_phase >= 8 {
            return if Self::player_finished(snapshot) {
                0.0
            } else {
                1.0
            };
        }
        if let Some(finish_delay) = Self::leader_finish_delay(snapshot) {
            let available_time = (finish_delay - delay_seconds.max(0.0)).max(0.0);
            if let Some(crossings) =
                Self::player_crossings_until_finish(snapshot, available_time, player_lap)
            {
                return crossings;
            }
        }

        // Fallback para sesiones sin datos válidos del líder.
        if snapshot.max_laps > 0 && snapshot.max_laps < 10_000 {
            return (snapshot.max_laps - snapshot.player_total_laps).max(0) as f64;
        }
        let available_time = (snapshot.session_time_remaining - delay_seconds.max(0.0)).max(0.0);
        if snapshot.session_time_remaining > 0.0 && player_lap > 0.0 {
            return (available_time / player_lap).ceil() + 1.0;
        }
        0.0
    }

    pub(super) fn laps_remaining(snapshot: &LmuSnapshot, player_lap: f64) -> f64 {
        Self::laps_remaining_after_delay(snapshot, player_lap, 0.0)
    }
}
