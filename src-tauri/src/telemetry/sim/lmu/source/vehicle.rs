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

    /// The best cumulative sector ends among the cars sharing the player's
    /// class, the reference a live-timing screen paints purple on a multiclass
    /// grid. Scoring publishes one best per sector, so the three ends need not
    /// come from the same lap or even the same car.
    pub(super) fn class_best_sector_ends(snapshot: &LmuSnapshot) -> [f64; 3] {
        const MINIMUM_END: [f64; 3] = [5.0, 10.0, 20.0];
        const MAXIMUM_END: [f64; 3] = [300.0, 600.0, 900.0];
        let entries =
            &snapshot.standings[..snapshot.standings_count.min(MAX_VEHICLES as u32) as usize];
        let Some(player_class) = entries
            .iter()
            .find(|entry| entry.is_player != 0)
            .map(|entry| entry.vehicle_class)
        else {
            return [0.0; 3];
        };
        let mut best = [0.0; 3];
        for entry in entries
            .iter()
            .filter(|entry| entry.vehicle_class == player_class)
        {
            for index in 0..3 {
                let candidate = entry.best_sector_ends[index];
                if candidate >= MINIMUM_END[index]
                    && candidate <= MAXIMUM_END[index]
                    && (best[index] <= 0.0 || candidate < best[index])
                {
                    best[index] = candidate;
                }
            }
        }
        best
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

    /// `mCompoundType` indexes the compounds the car actually carries, so the
    /// garage list resolves it exactly. The soft/medium/hard/wet ladder below is
    /// only the fallback for a car whose list has not arrived: it mislabels
    /// every car that does not carry all four, such as an LMP2 running Medium
    /// and Wet, where index 0 is Medium rather than Soft.
    fn compound_letter(compound: u8, conditions: &[RestCompoundCondition]) -> String {
        if let Some(condition) = conditions.get(compound as usize) {
            if let Some(letter) = condition
                .compound_type
                .split(|character: char| !character.is_alphabetic())
                .rfind(|word| !word.is_empty())
                .and_then(|word| word.chars().next())
            {
                return letter.to_ascii_uppercase().to_string();
            }
        }
        match compound {
            0 => "S",
            1 => "M",
            2 => "H",
            3 => "W",
            _ => "?",
        }
        .to_owned()
    }

    /// The optimal temperature the game publishes for the compound on each
    /// wheel, or -1 while the garage list is missing.
    pub(super) fn tire_optimal_temperatures(
        wheel_types: &[u8; 4],
        conditions: &[RestCompoundCondition],
    ) -> [f64; 4] {
        std::array::from_fn(|index| {
            conditions
                .get(wheel_types[index] as usize)
                .map(|condition| condition.optimal_temperature)
                .filter(|optimal| optimal.is_finite() && *optimal > 0.0)
                .unwrap_or(-1.0)
        })
    }

    pub(super) fn tire_compound(
        wheel_types: &[u8; 4],
        conditions: &[RestCompoundCondition],
    ) -> String {
        let mut unique: Vec<String> = Vec::new();
        for &compound in wheel_types {
            let letter = Self::compound_letter(compound, conditions);
            if !unique.contains(&letter) {
                unique.push(letter);
            }
        }
        unique.join("/")
    }

    pub(super) fn tire_compounds(
        wheel_types: &[u8; 4],
        conditions: &[RestCompoundCondition],
    ) -> [String; 4] {
        std::array::from_fn(|index| Self::compound_letter(wheel_types[index], conditions))
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
        let tire_compound = Self::tire_compound(&entry.wheel_compounds, &[]);
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
