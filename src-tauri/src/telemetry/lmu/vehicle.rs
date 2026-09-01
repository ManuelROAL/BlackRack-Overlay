//! Vehicle identity: the spectated car, its names, number and tyre compounds.

use super::*;

impl LmuTelemetrySource {
    pub(super) fn spectator_vehicle_id(&self) -> Option<i32> {
        let focused = self.local_rest.focused_standing()?;
        let focused_name = normalized_name(&focused.driver_name);
        let snapshot = self.last_valid_snapshot.as_ref()?;
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        snapshot.standings[..count]
            .iter()
            .find(|entry| {
                normalized_name(&Self::string_from_chars(&entry.driver_name)) == focused_name
            })
            .map(|entry| entry.vehicle_id)
    }

    pub(super) fn team_vehicle_id(&self) -> Option<i32> {
        let (driver_names, team_name, vehicle_name) = self.local_rest.team_reference()?;
        let normalized_drivers = driver_names
            .iter()
            .map(|name| normalized_driver_identity(name))
            .collect::<HashSet<_>>();
        let normalized_team = normalized_name(team_name);
        let normalized_vehicle = normalized_name(vehicle_name);
        let snapshot = self.last_valid_snapshot.as_ref()?;
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let standings = &snapshot.standings[..count];
        standings
            .iter()
            .find(|entry| {
                normalized_drivers.contains(&normalized_driver_identity(&Self::string_from_chars(
                    &entry.driver_name,
                )))
            })
            .or_else(|| {
                (!normalized_team.is_empty()).then(|| {
                    standings.iter().find(|entry| {
                        normalized_name(&Self::string_from_chars(&entry.team_name))
                            == normalized_team
                    })
                })?
            })
            .or_else(|| {
                (!normalized_vehicle.is_empty()).then(|| {
                    standings.iter().find(|entry| {
                        normalized_name(&Self::string_from_chars(&entry.vehicle_name))
                            == normalized_vehicle
                    })
                })?
            })
            .map(|entry| entry.vehicle_id)
    }

    pub(super) fn driver_assists(snapshot: &LmuSnapshot) -> (bool, bool) {
        (snapshot.tc_active != 0, snapshot.abs_active != 0)
    }

    pub(super) fn steering_and_force(
        snapshot: &LmuSnapshot,
        rest_range_degrees: Option<f64>,
    ) -> (f64, f64) {
        let steering = if snapshot.steering.is_finite() {
            snapshot.steering.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let range = rest_range_degrees
            .filter(|range| range.is_finite() && *range > 0.0)
            .unwrap_or_else(|| {
                if snapshot.steering_range_degrees.is_finite() {
                    snapshot.steering_range_degrees.max(0.0)
                } else {
                    0.0
                }
            });
        let force = if snapshot.force_feedback.is_finite() {
            snapshot.force_feedback.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        (steering * range * 0.5, force)
    }

    pub(super) fn string_from_chars(chars: &[c_char]) -> String {
        let bytes = chars
            .iter()
            .take_while(|value| **value != 0)
            .map(|value| *value as u8)
            .collect::<Vec<_>>();
        String::from_utf8_lossy(&bytes).trim().to_owned()
    }

    pub(super) fn player_vehicle_names(snapshot: &LmuSnapshot) -> (String, String) {
        let livery_name = Self::string_from_chars(&snapshot.vehicle_name);
        let model = Self::string_from_chars(&snapshot.vehicle_model);
        let persistent_name = if model.is_empty() {
            livery_name.clone()
        } else {
            model
        };
        (persistent_name, livery_name)
    }

    pub(super) fn car_number(vehicle_name: &str, vehicle_filename: &str) -> String {
        if let Some(number) = vehicle_name
            .split_once('#')
            .and_then(|(_, suffix)| {
                suffix
                    .split(|character: char| !character.is_ascii_digit())
                    .next()
            })
            .filter(|number| !number.is_empty())
        {
            return number.to_owned();
        }

        vehicle_filename
            .split(|character: char| !character.is_ascii_digit())
            .rfind(|part| !part.is_empty())
            .filter(|part| part.len() <= 3)
            .unwrap_or("--")
            .to_owned()
    }

    pub(super) fn tire_compound(wheel_types: &[u8; 4]) -> String {
        let letters: Vec<&str> = wheel_types
            .iter()
            .map(|&compound| match compound {
                0 => "S",
                1 => "M",
                2 => "H",
                3 => "W",
                _ => "?",
            })
            .collect();
        let mut unique: Vec<&str> = Vec::new();
        for letter in letters {
            if !unique.contains(&letter) {
                unique.push(letter);
            }
        }
        if unique.len() == 1 {
            unique[0].to_string()
        } else {
            unique.join("/")
        }
    }

    pub(super) fn tire_compounds(wheel_types: &[u8; 4]) -> [String; 4] {
        std::array::from_fn(|index| match wheel_types[index] {
            0 => "S".into(),
            1 => "M".into(),
            2 => "H".into(),
            3 => "W".into(),
            _ => "?".into(),
        })
    }

    pub(super) fn ensure_vehicle_identity(&mut self, entry: &LmuStandingEntry) {
        if let Some(identity) = self.vehicle_identities.get(&entry.vehicle_id) {
            if identity.matches(entry) {
                return;
            }
        }

        let driver_name = Self::string_from_chars(&entry.driver_name);
        let vehicle_class = Self::string_from_chars(&entry.vehicle_class);
        let scoring_vehicle_name = Self::string_from_chars(&entry.vehicle_name);
        let vehicle_filename = Self::string_from_chars(&entry.vehicle_filename);
        let vehicle_model = Self::string_from_chars(&entry.vehicle_model);
        let raw_team_name = Self::string_from_chars(&entry.team_name);
        let tire_compound = Self::tire_compound(&entry.wheel_compounds);
        let identity = VehicleIdentity {
            driver_name_raw: entry.driver_name,
            vehicle_class_raw: entry.vehicle_class,
            team_name_raw: entry.team_name,
            vehicle_name_raw: entry.vehicle_name,
            vehicle_filename_raw: entry.vehicle_filename,
            vehicle_model_raw: entry.vehicle_model,
            tire_compound_raw: entry.tire_compound,
            rear_tire_compound_raw: entry.rear_tire_compound,
            driver_name,
            vehicle_class,
            team_name: if raw_team_name.is_empty() {
                scoring_vehicle_name.clone()
            } else {
                raw_team_name
            },
            vehicle_name: if vehicle_model.is_empty() {
                scoring_vehicle_name.clone()
            } else {
                vehicle_model
            },
            fallback_car_number: Self::car_number(&scoring_vehicle_name, &vehicle_filename),
            tire_compound,
        };
        self.vehicle_identities.insert(entry.vehicle_id, identity);
    }
}
