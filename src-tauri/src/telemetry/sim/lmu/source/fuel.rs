//! Fuel and virtual energy consumption estimates.

use super::*;

impl LmuTelemetrySource {
    pub(super) fn update_clean_average(
        average: Option<f64>,
        samples: &mut u32,
        consumed: f64,
    ) -> Option<f64> {
        if !consumed.is_finite() || consumed <= 0.0 {
            return average;
        }
        *samples = samples.saturating_add(1);
        let weight = 1.0 / (*samples).min(8) as f64;
        Some(average.map_or(consumed, |value| value + (consumed - value) * weight))
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
