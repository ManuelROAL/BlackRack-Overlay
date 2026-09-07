//! Fuel and virtual energy consumption estimates.

use super::*;

#[derive(Default)]
pub(super) struct ClassPace {
    crossings: HashMap<i32, i32>,
    laps: VecDeque<f64>,
    pace: Option<f64>,
}

impl ClassPace {
    fn observe(&mut self, entry: &LmuStandingEntry, green: bool) {
        let changed =
            self.crossings.insert(entry.vehicle_id, entry.total_laps) != Some(entry.total_laps);
        if changed
            && green
            && entry.total_laps >= 2
            && entry.in_pits == 0
            && entry.last_lap_seconds.is_finite()
            && entry.last_lap_seconds > 0.0
        {
            self.laps.push_back(entry.last_lap_seconds);
            while self.laps.len() > 5 {
                self.laps.pop_front();
            }
            let fastest = self.laps.iter().copied().fold(f64::INFINITY, f64::min);
            let accepted: Vec<_> = self
                .laps
                .iter()
                .filter(|lap| **lap < fastest + 2.0)
                .collect();
            self.pace = Some(accepted.iter().map(|lap| **lap).sum::<f64>() / accepted.len() as f64);
        }
        if self.pace.is_none() {
            self.pace = [
                entry.best_lap_seconds,
                entry.last_lap_seconds,
                entry.estimated_lap_time,
            ]
            .into_iter()
            .find(|value| value.is_finite() && *value > 0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_pace_counts_each_crossing_once_and_excludes_slow_laps() {
        let mut pace = ClassPace::default();
        let mut entry = LmuStandingEntry {
            vehicle_id: 1,
            total_laps: 2,
            last_lap_seconds: 100.0,
            ..Default::default()
        };
        pace.observe(&entry, true);
        pace.observe(&entry, true);
        assert_eq!(pace.laps.len(), 1);
        entry.total_laps = 3;
        entry.last_lap_seconds = 101.0;
        pace.observe(&entry, true);
        entry.total_laps = 4;
        entry.last_lap_seconds = 110.0;
        pace.observe(&entry, true);
        assert_eq!(pace.pace, Some(100.5));
        entry.vehicle_id = 2;
        entry.last_lap_seconds = 100.0;
        pace.observe(&entry, true);
        entry.vehicle_id = 1;
        pace.observe(&entry, true);
        assert_eq!(pace.laps.len(), 4);
    }

    #[test]
    fn fuel_distance_uses_the_overall_finish_and_class_pace() {
        let mut source = LmuTelemetrySource::new();
        source.fuel_class_pace.pace = Some(120.0);
        source.fuel_leader_pace.pace = Some(100.0);
        let mut snapshot = LmuSnapshot {
            game_phase: 5,
            session_time_remaining: 250.0,
            leader_time_into_lap: 50.0,
            ..Default::default()
        };
        assert!((source.fuel_race_laps_remaining(&snapshot, 0.5, 150.0) - 2.5).abs() < 1e-9);
        snapshot.session_time_remaining = 0.0;
        snapshot.session_end_seconds = 1000.0;
        snapshot.leader_time_into_lap = 10.0;
        assert!((source.fuel_race_laps_remaining(&snapshot, 0.5, 150.0) - 1.5).abs() < 1e-9);
        snapshot.session_time_remaining = 250.0;
        snapshot.leader_time_into_lap = 50.0;
        snapshot.max_laps = 10;
        snapshot.leader_total_laps = 9;
        assert!((source.fuel_race_laps_remaining(&snapshot, 0.5, 150.0) - 0.5).abs() < 1e-9);
        source.fuel_class_pace = ClassPace::default();
        assert_eq!(
            source.fuel_race_laps_remaining(&snapshot, 0.5, 150.0),
            LmuTelemetrySource::estimated_laps_remaining(&snapshot, 0.5, 150.0)
        );
    }
}

impl LmuTelemetrySource {
    pub(super) fn update_fuel_race_pace(&mut self, snapshot: &LmuSnapshot) {
        let entries = &snapshot.standings[..(snapshot.standings_count as usize).min(MAX_VEHICLES)];
        if let Some(leader) = entries
            .iter()
            .filter(|entry| entry.position > 0)
            .min_by_key(|entry| entry.position)
        {
            self.fuel_leader_pace
                .observe(leader, snapshot.game_phase == 5);
        }
        if let Some(leader) = entries
            .iter()
            .filter(|entry| {
                entry.position > 0 && entry.vehicle_class_id == snapshot.vehicle_class_id
            })
            .min_by_key(|entry| entry.position)
        {
            self.fuel_class_pace
                .observe(leader, snapshot.game_phase == 5);
        }
    }

    /// Fuel follows the class pace and the overall leader's finish horizon.
    /// Shared timing headers keep their existing player-distance projection.
    pub(super) fn fuel_race_laps_remaining(
        &self,
        snapshot: &LmuSnapshot,
        progress: f64,
        fallback_pace: f64,
    ) -> f64 {
        let fallback = Self::estimated_laps_remaining(snapshot, progress, fallback_pace);
        if Self::player_finished(snapshot) || snapshot.game_phase >= 8 {
            return fallback;
        }
        let (Some(pace), Some(leader_pace)) =
            (self.fuel_class_pace.pace, self.fuel_leader_pace.pace)
        else {
            return fallback;
        };
        let Some(next) = Self::time_to_next_crossing(leader_pace, snapshot.leader_time_into_lap)
        else {
            return fallback;
        };
        let timed = if snapshot.session_time_remaining > 0.0 || snapshot.session_end_seconds > 0.0 {
            next + ((snapshot.session_time_remaining - next).max(0.0) / leader_pace).ceil()
                * leader_pace
        } else {
            f64::INFINITY
        };
        let lap_limited = if snapshot.max_laps > 0 && snapshot.max_laps < 10_000 {
            let crossings = (snapshot.max_laps - snapshot.leader_total_laps).max(0);
            if crossings == 0 {
                0.0
            } else {
                next + f64::from(crossings - 1) * leader_pace
            }
        } else {
            f64::INFINITY
        };
        let horizon = timed.min(lap_limited);
        if !horizon.is_finite() {
            return fallback;
        }
        let progress = progress.clamp(0.0, 1.0);
        (horizon / pace + progress - 1e-9).ceil().max(1.0) - progress
    }

    pub(super) fn update_clean_average(
        average: Option<f64>,
        samples: &mut LapConsumptionWindow,
        consumed: f64,
    ) -> Option<f64> {
        if !consumed.is_finite() || consumed <= 0.0 {
            return average;
        }
        samples.push(consumed)
    }

    pub(super) fn update_fuel_estimate(
        &mut self,
        snapshot: &LmuSnapshot,
        lap_changed: bool,
        completed_is_clean: bool,
    ) -> (f64, f64) {
        if Self::player_in_pits(snapshot) {
            if let Some(previous) = self.fuel_previous_sample {
                let added = snapshot.fuel_liters - previous;
                if added.is_finite() && added > 0.0 {
                    self.fuel_added_this_lap += added;
                }
            }
        }
        self.fuel_previous_sample = Some(snapshot.fuel_liters);

        if lap_changed {
            if let Some(previous_start) = self.fuel_at_lap_start {
                let consumed = previous_start + self.fuel_added_this_lap - snapshot.fuel_liters;
                if (0.1..30.0).contains(&consumed) {
                    self.fuel_last_lap = Some(consumed);
                    if completed_is_clean {
                        self.fuel_per_lap = Self::update_clean_average(
                            self.fuel_per_lap,
                            &mut self.fuel_clean_laps,
                            consumed,
                        );
                    }
                }
            }
            self.fuel_at_lap_start = Some(snapshot.fuel_liters);
            self.fuel_added_this_lap = 0.0;
        }

        let fuel_per_lap = self.fuel_per_lap.unwrap_or(0.0);
        let estimated_laps = self
            .fuel_per_lap
            .filter(|consumption| *consumption > 0.0)
            .map(|consumption| snapshot.fuel_liters / consumption)
            .unwrap_or(0.0);
        (fuel_per_lap, estimated_laps)
    }

    pub(super) fn uses_virtual_energy(snapshot: &LmuSnapshot) -> bool {
        // Valores de IP_VehicleClass del SDK oficial: Hypercar = 0, GT3 = 6.
        matches!(snapshot.vehicle_class_id, 0 | 6)
    }

    pub(super) fn virtual_energy_percent(raw: f64) -> f64 {
        if !raw.is_finite() || raw <= 0.0 {
            return 0.0;
        }

        // El SDK ha expuesto este valor normalizado en distintas iteraciones.
        // Admitimos tanto fracción [0, 1] como porcentaje [0, 100].
        if raw <= 1.25 {
            (raw * 100.0).clamp(0.0, 100.0)
        } else {
            raw.clamp(0.0, 100.0)
        }
    }

    pub(super) fn update_energy_estimate(
        &mut self,
        snapshot: &LmuSnapshot,
        lap_changed: bool,
        completed_is_clean: bool,
    ) -> (f64, f64, f64) {
        let current = Self::virtual_energy_percent(snapshot.virtual_energy);
        if !Self::uses_virtual_energy(snapshot) {
            self.energy_at_lap_start = None;
            self.energy_previous_sample = None;
            self.energy_added_this_lap = 0.0;
            self.energy_per_lap = None;
            return (current, 0.0, 0.0);
        }

        if Self::player_in_pits(snapshot) {
            if let Some(previous) = self.energy_previous_sample {
                let added = current - previous;
                if added.is_finite() && added > 0.0 {
                    self.energy_added_this_lap += added;
                }
            }
        }
        self.energy_previous_sample = Some(current);

        if lap_changed {
            if let Some(previous_start) = self.energy_at_lap_start {
                let consumed = previous_start + self.energy_added_this_lap - current;
                if (0.05..100.0).contains(&consumed) {
                    self.energy_last_lap = Some(consumed);
                    if completed_is_clean {
                        self.energy_per_lap = Self::update_clean_average(
                            self.energy_per_lap,
                            &mut self.energy_clean_laps,
                            consumed,
                        );
                    }
                }
            }
            // LMU puede limitar la EV inicial en carreras cortas. La referencia
            // de la vuelta siempre es la lectura real, nunca un 100 % teórico.
            self.energy_at_lap_start = Some(current);
            self.energy_added_this_lap = 0.0;
        }

        let energy_per_lap = self.energy_per_lap.unwrap_or(0.0);
        let estimated_laps = self
            .energy_per_lap
            .filter(|consumption| *consumption > 0.0)
            .map(|consumption| current / consumption)
            .unwrap_or(0.0);
        (current, energy_per_lap, estimated_laps)
    }
}
