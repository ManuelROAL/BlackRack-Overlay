use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const PROFILE_VERSION: u32 = 5;
const PROFILE_POINTS: usize = 101;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ResourceProfile {
    samples: u32,
    cumulative: Vec<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ProjectionCorrection {
    samples: u32,
    values: Vec<f64>,
}

impl ProjectionCorrection {
    fn at_progress(&self, progress: f64) -> f64 {
        if self.values.len() != PROFILE_POINTS {
            return 0.0;
        }
        let position = progress.clamp(0.0, 1.0) * (PROFILE_POINTS - 1) as f64;
        let lower = position.floor() as usize;
        let upper = position.ceil() as usize;
        let value = if lower == upper {
            self.values[lower]
        } else {
            let fraction = position - lower as f64;
            self.values[lower] + (self.values[upper] - self.values[lower]) * fraction
        };

        // Antes del 15 % la referencia completa es más estable. Entre el 15 y
        // el 25 % incorporamos progresivamente el error aprendido de esta pista.
        let confidence = ((progress - 0.15) / 0.10).clamp(0.0, 1.0);
        value * confidence
    }

    fn update(&mut self, profile: &ResourceProfile, completed_lap: &[f64]) {
        if profile.cumulative.len() != PROFILE_POINTS || completed_lap.len() != PROFILE_POINTS {
            return;
        }
        let completed_total = completed_lap.last().copied().unwrap_or(0.0);
        if !completed_total.is_finite() || completed_total <= 0.0 {
            return;
        }

        let next_sample = self.samples.saturating_add(1);
        let weight = 1.0 / next_sample.min(8) as f64;
        if self.values.len() != PROFILE_POINTS {
            self.values = vec![0.0; PROFILE_POINTS];
        }
        let limit = (completed_total * 0.15).max(0.01);
        for (index, correction) in self.values.iter_mut().enumerate() {
            let progress = index as f64 / (PROFILE_POINTS - 1) as f64;
            let projected = profile.total() + completed_lap[index] - profile.at_progress(progress);
            let residual = (completed_total - projected).clamp(-limit, limit);
            *correction += (residual - *correction) * weight;
        }
        self.samples = next_sample;
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ResourceAverage {
    samples: u32,
    value: f64,
}

impl ResourceAverage {
    fn update(&mut self, value: f64) {
        if !value.is_finite() || value <= 0.0 {
            return;
        }
        let next_sample = self.samples.saturating_add(1);
        let weight = 1.0 / next_sample.min(8) as f64;
        self.value += (value - self.value) * weight;
        self.samples = next_sample;
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct LapConsumptionAverage {
    fuel: ResourceAverage,
    energy: ResourceAverage,
}

impl ResourceProfile {
    fn total(&self) -> f64 {
        self.cumulative.last().copied().unwrap_or(0.0)
    }

    fn at_progress(&self, progress: f64) -> f64 {
        if self.cumulative.len() != PROFILE_POINTS {
            return 0.0;
        }
        let position = progress.clamp(0.0, 1.0) * (PROFILE_POINTS - 1) as f64;
        let lower = position.floor() as usize;
        let upper = position.ceil() as usize;
        if lower == upper {
            return self.cumulative[lower];
        }
        let fraction = position - lower as f64;
        self.cumulative[lower] + (self.cumulative[upper] - self.cumulative[lower]) * fraction
    }

    fn update(&mut self, lap: &[f64]) {
        if lap.len() != PROFILE_POINTS {
            return;
        }
        let next_sample = self.samples.saturating_add(1);
        let weight = 1.0 / next_sample.min(8) as f64;
        if self.cumulative.len() != PROFILE_POINTS {
            self.cumulative = lap.to_vec();
        } else {
            for (stored, current) in self.cumulative.iter_mut().zip(lap) {
                *stored += (*current - *stored) * weight;
            }
        }
        self.samples = next_sample;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredProfiles {
    version: u32,
    vehicle: String,
    track: String,
    fuel: ResourceProfile,
    energy: ResourceProfile,
    #[serde(default)]
    fuel_projection_correction: ProjectionCorrection,
    #[serde(default)]
    energy_projection_correction: ProjectionCorrection,
    #[serde(default)]
    pit_in: LapConsumptionAverage,
    #[serde(default)]
    pit_out: LapConsumptionAverage,
}

impl StoredProfiles {
    fn empty(vehicle: String, track: String) -> Self {
        Self {
            version: PROFILE_VERSION,
            vehicle,
            track,
            fuel: ResourceProfile::default(),
            energy: ResourceProfile::default(),
            fuel_projection_correction: ProjectionCorrection::default(),
            energy_projection_correction: ProjectionCorrection::default(),
            pit_in: LapConsumptionAverage::default(),
            pit_out: LapConsumptionAverage::default(),
        }
    }
}

#[derive(Clone, Debug)]
struct LapTrace {
    lap_number: i32,
    clean: bool,
    formation: bool,
    started_in_pits: bool,
    visited_pits: bool,
    fuel: Vec<Option<f64>>,
    energy: Vec<Option<f64>>,
}

impl LapTrace {
    fn new(lap_number: i32, clean: bool, formation: bool, in_pits: bool) -> Self {
        Self {
            lap_number,
            clean,
            formation,
            started_in_pits: in_pits,
            visited_pits: in_pits,
            fuel: vec![None; PROFILE_POINTS],
            energy: vec![None; PROFILE_POINTS],
        }
    }

    fn record(&mut self, progress: f64, fuel_used: f64, energy_used: f64) {
        let index = (progress.clamp(0.0, 1.0) * (PROFILE_POINTS - 1) as f64).floor() as usize;
        Self::record_resource(&mut self.fuel, index, fuel_used);
        Self::record_resource(&mut self.energy, index, energy_used);
    }

    fn record_resource(trace: &mut [Option<f64>], index: usize, used: f64) {
        if used.is_finite() && used >= 0.0 {
            trace[index] = Some(used);
        }
    }

    fn completed_resource(trace: &[Option<f64>], total: f64) -> Option<Vec<f64>> {
        if !total.is_finite() || total <= 0.0 {
            return None;
        }

        let mut anchors = Vec::with_capacity(PROFILE_POINTS);
        anchors.push((0usize, 0.0));
        for (index, value) in trace.iter().enumerate().skip(1).take(PROFILE_POINTS - 2) {
            if let Some(value) = value {
                anchors.push((index, value.clamp(0.0, total)));
            }
        }
        anchors.push((PROFILE_POINTS - 1, total));
        anchors.sort_by_key(|(index, _)| *index);
        anchors.dedup_by_key(|(index, _)| *index);

        let mut result = vec![0.0; PROFILE_POINTS];
        for pair in anchors.windows(2) {
            let (start_index, start_value) = pair[0];
            let (end_index, end_value) = pair[1];
            let width = end_index.saturating_sub(start_index);
            if width == 0 {
                result[start_index] = end_value;
                continue;
            }
            for (offset, output) in result[start_index..=end_index].iter_mut().enumerate() {
                let fraction = offset as f64 / width as f64;
                *output = start_value + (end_value - start_value) * fraction;
            }
        }

        // El consumo acumulado nunca puede disminuir aunque haya ruido de lectura.
        let mut maximum: f64 = 0.0;
        for value in &mut result {
            maximum = maximum.max(*value);
            *value = maximum.min(total);
        }
        Some(result)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProfileEstimate {
    pub lap_progress: f64,
    pub fuel_reference: f64,
    pub fuel_projected: f64,
    pub energy_reference: f64,
    pub energy_projected: f64,
    pub fuel_pit_cycle_consumption: f64,
    pub energy_pit_cycle_consumption: f64,
    pub fuel_pit_out_consumption: f64,
    pub energy_pit_out_consumption: f64,
    pub current_lap_started_in_pits: bool,
    pub samples: u32,
}

pub struct ConsumptionProfiler {
    directory: Option<PathBuf>,
    identity: Option<(String, String)>,
    storage_path: Option<PathBuf>,
    profiles: StoredProfiles,
    trace: Option<LapTrace>,
    awaiting_pit_out: bool,
}

impl ConsumptionProfiler {
    pub fn new(directory: Option<PathBuf>) -> Self {
        Self {
            directory,
            identity: None,
            storage_path: None,
            profiles: StoredProfiles::empty(String::new(), String::new()),
            trace: None,
            awaiting_pit_out: false,
        }
    }

    pub fn reset_lap(&mut self) {
        self.trace = None;
        self.awaiting_pit_out = false;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn observe(
        &mut self,
        vehicle: &str,
        track: &str,
        lap_number: i32,
        lap_progress: f64,
        lap_is_valid: bool,
        in_pits: bool,
        formation: bool,
        fuel_used: f64,
        energy_used: f64,
        completed_fuel: Option<f64>,
        completed_energy: Option<f64>,
    ) -> ProfileEstimate {
        self.select_identity(vehicle, track);

        let changed_lap = self
            .trace
            .as_ref()
            .map(|trace| trace.lap_number != lap_number)
            .unwrap_or(false);
        if changed_lap {
            self.finish_lap(completed_fuel, completed_energy);
            self.trace = None;
        }

        // Telemetría y scoring pueden actualizarse con un frame de diferencia.
        // Al cambiar mLapNumber, mLapDist conserva a veces el 99,8 % de la vuelta
        // anterior. Ese frame pertenece ya a la vuelta nueva y debe comenzar en 0.
        let lap_progress = if changed_lap && lap_progress > 0.9 {
            0.0
        } else {
            lap_progress
        };

        let clean_now = lap_is_valid && !in_pits;
        let trace = self
            .trace
            .get_or_insert_with(|| LapTrace::new(lap_number, clean_now, formation, in_pits));
        trace.clean &= clean_now;
        trace.formation |= formation;
        trace.visited_pits |= in_pits;
        trace.record(lap_progress, fuel_used, energy_used);

        let fuel_reference = self.profiles.fuel.total();
        let energy_reference = self.profiles.energy.total();
        let complete_pit_cycle =
            self.profiles.pit_in.fuel.samples > 0 && self.profiles.pit_out.fuel.samples > 0;
        let complete_energy_pit_cycle =
            self.profiles.pit_in.energy.samples > 0 && self.profiles.pit_out.energy.samples > 0;
        ProfileEstimate {
            lap_progress: lap_progress.clamp(0.0, 1.0),
            fuel_reference,
            fuel_projected: Self::project(
                &self.profiles.fuel,
                &self.profiles.fuel_projection_correction,
                lap_progress,
                fuel_used,
            ),
            energy_reference,
            energy_projected: Self::project(
                &self.profiles.energy,
                &self.profiles.energy_projection_correction,
                lap_progress,
                energy_used,
            ),
            fuel_pit_cycle_consumption: if complete_pit_cycle {
                self.profiles.pit_in.fuel.value + self.profiles.pit_out.fuel.value
            } else {
                0.0
            },
            energy_pit_cycle_consumption: if complete_energy_pit_cycle {
                self.profiles.pit_in.energy.value + self.profiles.pit_out.energy.value
            } else {
                0.0
            },
            fuel_pit_out_consumption: self.profiles.pit_out.fuel.value,
            energy_pit_out_consumption: self.profiles.pit_out.energy.value,
            current_lap_started_in_pits: trace.started_in_pits,
            samples: self.profiles.fuel.samples.max(self.profiles.energy.samples),
        }
    }

    fn project(
        profile: &ResourceProfile,
        correction: &ProjectionCorrection,
        progress: f64,
        used: f64,
    ) -> f64 {
        let total = profile.total();
        if total <= 0.0 || !used.is_finite() || used < 0.0 {
            return 0.0;
        }
        let projected = (total + used - profile.at_progress(progress)
            + correction.at_progress(progress))
        .max(used)
        .max(0.0);
        // Nunca se debe sustituir una referencia válida por el valor casi cero
        // producido por un frame desincronizado en la línea de meta.
        if projected < total * 0.5 {
            total
        } else {
            projected
        }
    }

    fn finish_lap(&mut self, completed_fuel: Option<f64>, completed_energy: Option<f64>) {
        let Some(trace) = self.trace.as_ref() else {
            return;
        };
        if trace.formation {
            return;
        }

        let mut changed = false;
        if trace.started_in_pits && self.awaiting_pit_out {
            let target = &mut self.profiles.pit_out;
            if let Some(total) = completed_fuel {
                target.fuel.update(total);
                changed = true;
            }
            if let Some(total) = completed_energy {
                target.energy.update(total);
                changed = true;
            }
            self.awaiting_pit_out = false;
        } else if trace.visited_pits && !trace.started_in_pits {
            let target = &mut self.profiles.pit_in;
            let mut learned_pit_in = false;
            if let Some(total) = completed_fuel {
                target.fuel.update(total);
                changed = true;
                learned_pit_in = true;
            }
            if let Some(total) = completed_energy {
                target.energy.update(total);
                changed = true;
                learned_pit_in = true;
            }
            self.awaiting_pit_out = learned_pit_in;
        } else if trace.clean {
            self.awaiting_pit_out = false;
            if let Some(total) = completed_fuel {
                if let Some(lap) = LapTrace::completed_resource(&trace.fuel, total) {
                    let had_reference = self.profiles.fuel.samples > 0;
                    self.profiles.fuel.update(&lap);
                    if had_reference {
                        self.profiles
                            .fuel_projection_correction
                            .update(&self.profiles.fuel, &lap);
                    }
                    changed = true;
                }
            }
            if let Some(total) = completed_energy {
                if let Some(lap) = LapTrace::completed_resource(&trace.energy, total) {
                    let had_reference = self.profiles.energy.samples > 0;
                    self.profiles.energy.update(&lap);
                    if had_reference {
                        self.profiles
                            .energy_projection_correction
                            .update(&self.profiles.energy, &lap);
                    }
                    changed = true;
                }
            }
        }
        if changed {
            self.save();
        }
    }

    fn select_identity(&mut self, vehicle: &str, track: &str) {
        let next = (vehicle.trim().to_owned(), track.trim().to_owned());
        if self.identity.as_ref() == Some(&next) {
            return;
        }

        self.identity = Some(next.clone());
        self.trace = None;
        self.awaiting_pit_out = false;
        self.storage_path = self
            .directory
            .as_ref()
            .map(|directory| directory.join(Self::filename(&next.0, &next.1)));
        self.profiles = self
            .storage_path
            .as_deref()
            .and_then(Self::load)
            .filter(|stored| {
                (1..=PROFILE_VERSION).contains(&stored.version)
                    && stored.vehicle == next.0
                    && stored.track == next.1
            })
            .unwrap_or_else(|| StoredProfiles::empty(next.0, next.1));
        if self.profiles.version < 3 {
            // Las versiones anteriores no diferenciaban una salida de garaje de
            // una vuelta de salida posterior a una parada real.
            self.profiles.pit_in = LapConsumptionAverage::default();
            self.profiles.pit_out = LapConsumptionAverage::default();
        }
        self.profiles.version = PROFILE_VERSION;
    }

    fn filename(vehicle: &str, track: &str) -> String {
        fn clean(value: &str) -> String {
            let text = value
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() {
                        character.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect::<String>();
            text.split('-')
                .filter(|part| !part.is_empty())
                .take(8)
                .collect::<Vec<_>>()
                .join("-")
        }
        let mut hash = 0xcbf29ce484222325u64;
        for byte in vehicle.bytes().chain([0]).chain(track.bytes()) {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("{}--{}--{:016x}.json", clean(vehicle), clean(track), hash)
    }

    fn load(path: &Path) -> Option<StoredProfiles> {
        let bytes = fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn save(&self) {
        let Some(path) = self.storage_path.as_deref() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&self.profiles) {
            let _ = fs::write(path, bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ConsumptionProfiler;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn learns_clean_lap_and_projects_current_consumption_by_distance() {
        let mut profiler = ConsumptionProfiler::new(None);
        profiler.observe(
            "GT3", "Spa", 1, 0.0, true, false, false, 0.0, 0.0, None, None,
        );
        profiler.observe(
            "GT3", "Spa", 1, 0.5, true, false, false, 1.5, 2.0, None, None,
        );
        let learned = profiler.observe(
            "GT3",
            "Spa",
            2,
            0.0,
            true,
            false,
            false,
            0.0,
            0.0,
            Some(3.0),
            Some(4.0),
        );
        assert!((learned.fuel_reference - 3.0).abs() < 0.001);
        assert!((learned.energy_reference - 4.0).abs() < 0.001);

        let projected = profiler.observe(
            "GT3", "Spa", 2, 0.5, true, false, false, 2.0, 2.5, None, None,
        );
        assert!((projected.fuel_projected - 3.5).abs() < 0.001);
        assert!((projected.energy_projected - 4.5).abs() < 0.001);
    }

    #[test]
    fn learns_and_applies_projection_residual_after_the_first_quarter() {
        let mut profiler = ConsumptionProfiler::new(None);

        profiler.observe(
            "LMP2", "Daytona", 1, 0.5, true, false, false, 1.5, 0.0, None, None,
        );
        profiler.observe(
            "LMP2",
            "Daytona",
            2,
            0.0,
            true,
            false,
            false,
            0.0,
            0.0,
            Some(3.0),
            None,
        );

        // La segunda vuelta consume 0,5 L más de lo que proyectaría el perfil
        // anterior. Al actualizar también el perfil, quedan 0,25 L de residuo.
        profiler.observe(
            "LMP2", "Daytona", 2, 0.5, true, false, false, 2.0, 0.0, None, None,
        );
        profiler.observe(
            "LMP2",
            "Daytona",
            3,
            0.0,
            true,
            false,
            false,
            0.0,
            0.0,
            Some(4.0),
            None,
        );

        let early = profiler.observe(
            "LMP2", "Daytona", 3, 0.10, true, false, false, 0.4, 0.0, None, None,
        );
        let projected = profiler.observe(
            "LMP2", "Daytona", 3, 0.5, true, false, false, 2.0, 0.0, None, None,
        );

        assert!(early.fuel_projected < 4.0);
        assert!((projected.fuel_projected - 4.0).abs() < 0.001);
    }

    #[test]
    fn pit_lap_does_not_replace_clean_reference_profile() {
        let mut profiler = ConsumptionProfiler::new(None);
        profiler.observe(
            "Hypercar", "Le Mans", 1, 0.5, true, false, false, 1.0, 1.5, None, None,
        );
        profiler.observe(
            "Hypercar",
            "Le Mans",
            2,
            0.0,
            true,
            false,
            false,
            0.0,
            0.0,
            Some(2.0),
            Some(3.0),
        );
        profiler.observe(
            "Hypercar", "Le Mans", 2, 0.5, true, true, false, 0.5, 0.8, None, None,
        );
        let after_pit = profiler.observe(
            "Hypercar",
            "Le Mans",
            3,
            0.0,
            true,
            false,
            false,
            0.0,
            0.0,
            Some(8.0),
            Some(12.0),
        );
        assert!((after_pit.fuel_reference - 2.0).abs() < 0.001);
        assert!((after_pit.energy_reference - 3.0).abs() < 0.001);
        assert_eq!(after_pit.samples, 1);
    }

    #[test]
    fn learns_a_complete_pit_cycle_without_polluting_clean_consumption() {
        let mut profiler = ConsumptionProfiler::new(None);
        profiler.observe(
            "Hypercar", "Le Mans", 1, 0.5, true, false, false, 5.0, 4.0, None, None,
        );
        profiler.observe(
            "Hypercar",
            "Le Mans",
            2,
            0.0,
            true,
            false,
            false,
            0.0,
            0.0,
            Some(10.0),
            Some(8.0),
        );

        // Entrada: la vuelta empieza en pista y visita el carril de boxes.
        profiler.observe(
            "Hypercar", "Le Mans", 2, 0.8, true, true, false, 5.0, 4.0, None, None,
        );
        profiler.observe(
            "Hypercar",
            "Le Mans",
            3,
            0.0,
            true,
            true,
            false,
            0.0,
            0.0,
            Some(6.0),
            Some(5.0),
        );

        // Salida: la siguiente vuelta comienza en boxes y termina en pista.
        profiler.observe(
            "Hypercar", "Le Mans", 3, 0.4, true, false, false, 3.0, 2.5, None, None,
        );
        let estimate = profiler.observe(
            "Hypercar",
            "Le Mans",
            4,
            0.0,
            true,
            false,
            false,
            0.0,
            0.0,
            Some(8.0),
            Some(7.0),
        );

        assert!((estimate.fuel_reference - 10.0).abs() < 0.001);
        assert!((estimate.energy_reference - 8.0).abs() < 0.001);
        assert!((estimate.fuel_pit_cycle_consumption - 14.0).abs() < 0.001);
        assert!((estimate.energy_pit_cycle_consumption - 12.0).abs() < 0.001);
        assert_eq!(estimate.samples, 1);
    }

    #[test]
    fn garage_exit_is_not_learned_as_a_race_pit_out_lap() {
        let mut profiler = ConsumptionProfiler::new(None);
        profiler.observe(
            "LMP2", "Daytona", 1, 0.0, true, true, false, 0.0, 0.0, None, None,
        );
        let estimate = profiler.observe(
            "LMP2",
            "Daytona",
            2,
            0.0,
            true,
            false,
            false,
            0.0,
            0.0,
            Some(2.1),
            None,
        );

        assert_eq!(estimate.fuel_pit_out_consumption, 0.0);
        assert_eq!(estimate.fuel_pit_cycle_consumption, 0.0);
    }

    #[test]
    fn lap_change_with_stale_distance_does_not_emit_a_zero_projection() {
        let mut profiler = ConsumptionProfiler::new(None);
        profiler.observe(
            "LMP2", "Daytona", 1, 0.5, true, false, false, 1.4, 0.0, None, None,
        );
        profiler.observe(
            "LMP2",
            "Daytona",
            2,
            0.0,
            true,
            false,
            false,
            0.0,
            0.0,
            Some(2.8),
            None,
        );

        let estimate = profiler.observe(
            "LMP2",
            "Daytona",
            3,
            0.998,
            true,
            false,
            false,
            0.001,
            0.0,
            Some(2.8),
            None,
        );

        assert_eq!(estimate.lap_progress, 0.0);
        assert!(estimate.fuel_projected > 2.0);
    }

    #[test]
    fn profile_is_restored_for_the_same_vehicle_and_track() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "lmu-overlay-consumption-profile-{}-{unique}",
            std::process::id()
        ));

        {
            let mut profiler = ConsumptionProfiler::new(Some(directory.clone()));
            profiler.observe(
                "GT3", "Portimao", 1, 0.5, true, false, false, 1.0, 1.5, None, None,
            );
            profiler.observe(
                "GT3",
                "Portimao",
                2,
                0.0,
                true,
                false,
                false,
                0.0,
                0.0,
                Some(2.0),
                Some(3.0),
            );
        }

        let mut restored = ConsumptionProfiler::new(Some(directory.clone()));
        let estimate = restored.observe(
            "GT3", "Portimao", 10, 0.0, true, false, false, 0.0, 0.0, None, None,
        );
        assert!((estimate.fuel_reference - 2.0).abs() < 0.001);
        assert!((estimate.energy_reference - 3.0).abs() < 0.001);
        assert_eq!(estimate.samples, 1);

        let _ = fs::remove_dir_all(directory);
    }
}
