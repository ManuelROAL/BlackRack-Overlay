//! Driver rank estimation and the diagnostics that validate it.

use super::*;

impl LmuTelemetrySource {
    pub(super) fn class_rank(class_name: &str) -> u8 {
        let normalized = class_name.to_ascii_uppercase();
        if normalized.contains("HYPER") || normalized.contains("GTP") {
            0
        } else if normalized.contains("LMP2") {
            1
        } else if normalized.contains("LMGT3") || normalized.contains("GT3") {
            2
        } else {
            3
        }
    }

    pub(super) fn driver_rank_score(rank: &str, progress: f64) -> Option<f64> {
        if progress < 0.0 || !progress.is_finite() {
            return None;
        }
        let mut characters = rank.trim().chars();
        let level = characters.next()?.to_ascii_uppercase();
        let tier = characters.as_str().parse::<i32>().ok()?;
        let level_offset = match level {
            'B' => 0,
            'S' => 3,
            'G' => 6,
            'P' => 9,
            _ => return None,
        };
        Some(((level_offset + tier).max(0) as f64 * 100.0) + progress.clamp(0.0, 100.0))
    }

    pub(super) fn head_to_head(position: i32, opponent_position: i32) -> f64 {
        match position.cmp(&opponent_position) {
            std::cmp::Ordering::Less => 1.0,
            std::cmp::Ordering::Equal => 0.5,
            std::cmp::Ordering::Greater => 0.0,
        }
    }

    pub(super) fn driver_rank_expected(
        rating: f64,
        opponent_rating: f64,
        settings: DriverRankSettings,
    ) -> f64 {
        1.0 / (1.0
            + settings
                .logarithm
                .powf((opponent_rating - rating) / settings.distance))
    }

    pub(super) fn driver_rank_gain(
        race_result_total: f64,
        qualify_result_total: f64,
        opponent_count: u32,
        settings: DriverRankSettings,
    ) -> f64 {
        let gain_factor = settings.multiplier * settings.k / (2.0 * f64::from(opponent_count));
        gain_factor * (race_result_total + DRIVER_RANK_QUALIFY_WEIGHT * qualify_result_total)
    }

    pub(super) fn usable_raw_elo(value: Option<f64>) -> Option<f64> {
        value.filter(|elo| elo.is_finite() && *elo > 0.0)
    }

    pub(super) fn driver_rank_actual_gain(
        before_raw_elo: Option<f64>,
        after_raw_elo: Option<f64>,
        before_visual_score: Option<f64>,
        after_visual_score: Option<f64>,
    ) -> Option<(f64, &'static str)> {
        match (
            Self::usable_raw_elo(before_raw_elo),
            Self::usable_raw_elo(after_raw_elo),
        ) {
            (Some(before), Some(after)) => {
                Some(((after - before) / DRIVER_RANK_INTERNAL_SCALE, "raw_elo"))
            }
            _ => match (before_visual_score, after_visual_score) {
                (Some(before), Some(after)) if before.is_finite() && after.is_finite() => {
                    Some((after - before, "visual_score"))
                }
                _ => None,
            },
        }
    }

    pub(super) fn update_driver_rank_validation(&mut self, input: DriverRankValidationInput<'_>) {
        let DriverRankValidationInput {
            session_type,
            game_phase,
            event_id,
            split_number,
            sample,
            player_raw_elo,
            refresh_revision,
        } = input;
        if !crate::telemetry::dr_estimate_log::enabled() {
            return;
        }

        let is_race = (10..=13).contains(&session_type);
        if is_race {
            let Some(sample) = sample else {
                return;
            };
            if self
                .driver_rank_validation
                .as_ref()
                .is_none_or(|state| state.race_sequence != self.driver_rank_race_sequence)
            {
                if let Some(previous) = self.driver_rank_validation.take() {
                    crate::telemetry::dr_estimate_log::queue(serde_json::json!({
                        "event": "driver_rank_validation",
                        "log_schema_version": DRIVER_RANK_LOG_SCHEMA_VERSION,
                        "formula_version": DRIVER_RANK_FORMULA_VERSION,
                        "app_version": env!("CARGO_PKG_VERSION"),
                        "status": "unsettled",
                        "reason": "new_race_started_before_fresh_rank",
                        "race_sequence": previous.race_sequence,
                        "event_id": previous.event_id,
                        "split_number": previous.split_number,
                        "comparison": {
                            "estimated_gain": previous.final_estimated_gain,
                            "actual_gain": null,
                            "error": null,
                            "absolute_error": null,
                        },
                    }));
                }
                self.driver_rank_validation = Some(DriverRankValidationState {
                    race_sequence: self.driver_rank_race_sequence,
                    event_id: event_id.to_owned(),
                    split_number,
                    player_vehicle_id: sample.vehicle_id,
                    player_class: sample.vehicle_class.clone(),
                    before_raw_elo: Self::usable_raw_elo(player_raw_elo),
                    before_visual_score: sample.visual_score,
                    before_refresh_revision: refresh_revision,
                    final_estimated_gain: None,
                    final_race_position: 0,
                    final_qualifying_position: 0,
                    final_position_source: "unavailable",
                    final_logged_signature: None,
                });
            }

            let Some(state) = self.driver_rank_validation.as_mut() else {
                return;
            };
            if state.event_id.is_empty() && !event_id.is_empty() {
                state.event_id = event_id.to_owned();
            }
            if state.split_number == 0 && split_number > 0 {
                state.split_number = split_number;
            }
            if game_phase < 8 && refresh_revision >= state.before_refresh_revision {
                if let Some(raw_elo) = Self::usable_raw_elo(player_raw_elo) {
                    state.before_raw_elo = Some(raw_elo);
                }
                if sample.visual_score.is_some() {
                    state.before_visual_score = sample.visual_score;
                }
                state.before_refresh_revision = refresh_revision;
            }
            if game_phase < 8 || sample.estimated_gain.is_none() {
                return;
            }

            state.final_estimated_gain = sample.estimated_gain;
            state.final_race_position = sample.race_position;
            state.final_qualifying_position = sample.qualifying_position;
            state.final_position_source = sample.race_position_source;
            let signature = format!(
                "{:.3}|{}|{}|{}",
                sample.estimated_gain.unwrap_or_default(),
                sample.race_position,
                sample.qualifying_position,
                sample.race_position_source
            );
            if state.final_logged_signature.as_deref() == Some(signature.as_str()) {
                return;
            }
            state.final_logged_signature = Some(signature);
            crate::telemetry::dr_estimate_log::queue(serde_json::json!({
                "event": "driver_rank_race_final",
                "log_schema_version": DRIVER_RANK_LOG_SCHEMA_VERSION,
                "formula_version": DRIVER_RANK_FORMULA_VERSION,
                "app_version": env!("CARGO_PKG_VERSION"),
                "race_sequence": state.race_sequence,
                "event_id": state.event_id,
                "split_number": state.split_number,
                "player": {
                    "vehicle_id": state.player_vehicle_id,
                    "vehicle_class": state.player_class,
                    "before_raw_elo": state.before_raw_elo,
                    "before_visual_score": state.before_visual_score,
                    "before_refresh_revision": state.before_refresh_revision,
                },
                "result": {
                    "estimated_gain": state.final_estimated_gain,
                    "race_position": state.final_race_position,
                    "qualifying_position": state.final_qualifying_position,
                    "position_source": state.final_position_source,
                },
            }));
            return;
        }

        let Some(state) = self.driver_rank_validation.take() else {
            return;
        };
        let Some(estimated_gain) = state.final_estimated_gain else {
            self.driver_rank_validation = Some(state);
            return;
        };
        if refresh_revision <= state.before_refresh_revision {
            self.driver_rank_validation = Some(state);
            return;
        }

        let current_visual_score = sample.and_then(|value| value.visual_score);
        let after_raw_elo = Self::usable_raw_elo(player_raw_elo);
        let Some((actual_gain, actual_source)) = Self::driver_rank_actual_gain(
            state.before_raw_elo,
            after_raw_elo,
            state.before_visual_score,
            current_visual_score,
        ) else {
            self.driver_rank_validation = Some(state);
            return;
        };
        let error = actual_gain - estimated_gain;
        crate::telemetry::dr_estimate_log::queue(serde_json::json!({
            "event": "driver_rank_validation",
            "log_schema_version": DRIVER_RANK_LOG_SCHEMA_VERSION,
            "formula_version": DRIVER_RANK_FORMULA_VERSION,
            "app_version": env!("CARGO_PKG_VERSION"),
            "status": "settled",
            "race_sequence": state.race_sequence,
            "event_id": state.event_id,
            "split_number": state.split_number,
            "player": {
                "vehicle_id": state.player_vehicle_id,
                "vehicle_class": state.player_class,
                "before_raw_elo": state.before_raw_elo,
                "after_raw_elo": after_raw_elo,
                "before_visual_score": state.before_visual_score,
                "after_visual_score": current_visual_score,
                "before_refresh_revision": state.before_refresh_revision,
                "after_refresh_revision": refresh_revision,
            },
            "comparison": {
                "estimated_gain": estimated_gain,
                "actual_gain": actual_gain,
                "actual_source": actual_source,
                "error": error,
                "absolute_error": error.abs(),
            },
            "result": {
                "race_position": state.final_race_position,
                "qualifying_position": state.final_qualifying_position,
                "position_source": state.final_position_source,
            },
        }));
    }

    pub(super) fn scored_class_positions(
        entries: &[StandingEntry],
        scored_overall_positions: &HashMap<i32, i32>,
    ) -> HashMap<i32, i32> {
        let mut classes = HashMap::<&str, Vec<&StandingEntry>>::new();
        for entry in entries {
            classes
                .entry(entry.vehicle_class.as_str())
                .or_default()
                .push(entry);
        }

        let mut positions = HashMap::new();
        for mut class_entries in classes.into_values() {
            if !class_entries.iter().all(|entry| {
                scored_overall_positions
                    .get(&entry.vehicle_id)
                    .is_some_and(|position| *position > 0)
            }) {
                continue;
            }
            class_entries.sort_by_key(|entry| {
                (
                    scored_overall_positions[&entry.vehicle_id],
                    entry.vehicle_id,
                )
            });
            for (index, entry) in class_entries.into_iter().enumerate() {
                positions.insert(entry.vehicle_id, index as i32 + 1);
            }
        }
        positions
    }

    pub(super) fn class_positions_with_complete_preferred_order(
        entries: &[StandingEntry],
        preferred_overall_positions: &HashMap<i32, i32>,
        fallback_class_positions: &HashMap<i32, i32>,
    ) -> HashMap<i32, i32> {
        let preferred_class_positions =
            Self::scored_class_positions(entries, preferred_overall_positions);
        entries
            .iter()
            .map(|entry| {
                let position = preferred_class_positions
                    .get(&entry.vehicle_id)
                    .or_else(|| fallback_class_positions.get(&entry.vehicle_id))
                    .copied()
                    .unwrap_or(entry.position);
                (entry.vehicle_id, position)
            })
            .collect()
    }

    pub(super) fn update_driver_rank_estimates(
        entries: &mut [StandingEntry],
        rank_scores: &HashMap<i32, f64>,
        qualifying_positions: &HashMap<i32, i32>,
        scored_class_positions: &HashMap<i32, i32>,
        session_type: i32,
        settings: DriverRankSettings,
    ) -> Option<DriverRankEstimateDiagnostic> {
        if (0..=8).contains(&session_type) {
            let entry = entries.iter().find(|entry| entry.is_player)?;
            let visual_score = rank_scores.get(&entry.vehicle_id).copied();
            return Some(DriverRankEstimateDiagnostic {
                status: if visual_score.is_some() {
                    "current_rank"
                } else {
                    "player_rank_unavailable"
                },
                vehicle_id: entry.vehicle_id,
                vehicle_class: entry.vehicle_class.clone(),
                driver_rank: entry.driver_rank.clone(),
                driver_rank_progress: entry.driver_rank_progress,
                visual_score,
                race_position: 0,
                live_race_position: 0,
                race_position_source: "not_applicable",
                qualifying_position: 0,
                same_class_rivals: 0,
                rated_opponents: 0,
                race_result_total: 0.0,
                qualifying_result_total: 0.0,
                gain_factor: 0.0,
                estimated_gain: None,
                opponents: Vec::new(),
            });
        }
        if !(10..=13).contains(&session_type) {
            return None;
        }

        // RaceControl no publica el cambio en directo. La puntuación visual se
        // convierte a la escala interna usada por el estimador y se aplican los
        // parámetros de DR configurados para el evento.
        let mut gains = Vec::<(usize, f64)>::new();
        let mut player_diagnostic = None;
        for (index, entry) in entries.iter().enumerate() {
            let race_position = scored_class_positions
                .get(&entry.vehicle_id)
                .copied()
                .unwrap_or(entry.position);
            let race_position_source = if scored_class_positions.contains_key(&entry.vehicle_id) {
                "rest_server_scored"
            } else {
                "shared_memory_live"
            };
            let starting_position = qualifying_positions
                .get(&entry.vehicle_id)
                .copied()
                .unwrap_or(entry.position + entry.position_change);
            let same_class_rivals = entries
                .iter()
                .enumerate()
                .filter(|(other_index, opponent)| {
                    *other_index != index && opponent.vehicle_class == entry.vehicle_class
                })
                .count() as u32;
            let rated_opponents = entries
                .iter()
                .enumerate()
                .filter(|(other_index, opponent)| {
                    *other_index != index
                        && opponent.vehicle_class == entry.vehicle_class
                        && rank_scores.contains_key(&opponent.vehicle_id)
                })
                .count() as u32;
            let Some(rank_score) = rank_scores.get(&entry.vehicle_id).copied() else {
                if entry.is_player {
                    player_diagnostic = Some(DriverRankEstimateDiagnostic {
                        status: "player_rank_unavailable",
                        vehicle_id: entry.vehicle_id,
                        vehicle_class: entry.vehicle_class.clone(),
                        driver_rank: entry.driver_rank.clone(),
                        driver_rank_progress: entry.driver_rank_progress,
                        visual_score: None,
                        race_position,
                        live_race_position: entry.position,
                        race_position_source,
                        qualifying_position: starting_position,
                        same_class_rivals,
                        rated_opponents,
                        race_result_total: 0.0,
                        qualifying_result_total: 0.0,
                        gain_factor: 0.0,
                        estimated_gain: None,
                        opponents: Vec::new(),
                    });
                }
                continue;
            };
            let mut race_result_total = 0.0;
            let mut qualify_result_total = 0.0;
            let mut opponent_count = 0_u32;
            let mut opponent_diagnostics = Vec::new();
            for (other_index, opponent) in entries.iter().enumerate() {
                if other_index == index || opponent.vehicle_class != entry.vehicle_class {
                    continue;
                }
                let Some(opponent_rank_score) = rank_scores.get(&opponent.vehicle_id).copied()
                else {
                    continue;
                };
                let rating = rank_score * DRIVER_RANK_INTERNAL_SCALE;
                let opponent_rating = opponent_rank_score * DRIVER_RANK_INTERNAL_SCALE;
                let expected = Self::driver_rank_expected(rating, opponent_rating, settings);
                let opponent_race_position = scored_class_positions
                    .get(&opponent.vehicle_id)
                    .copied()
                    .unwrap_or(opponent.position);
                let race_result = Self::head_to_head(race_position, opponent_race_position);
                let opponent_start = qualifying_positions
                    .get(&opponent.vehicle_id)
                    .copied()
                    .unwrap_or(opponent.position + opponent.position_change);
                let qualify_result = Self::head_to_head(starting_position, opponent_start);
                race_result_total += race_result - expected;
                qualify_result_total += qualify_result - expected;
                opponent_count += 1;
                if entry.is_player {
                    opponent_diagnostics.push(DriverRankOpponentDiagnostic {
                        vehicle_id: opponent.vehicle_id,
                        visual_score: opponent_rank_score,
                        internal_score: opponent_rating,
                        race_position: opponent_race_position,
                        qualifying_position: opponent_start,
                        expected,
                        race_result,
                        qualifying_result: qualify_result,
                    });
                }
            }
            if opponent_count == 0 {
                if entry.is_player {
                    player_diagnostic = Some(DriverRankEstimateDiagnostic {
                        status: "opponent_ranks_unavailable",
                        vehicle_id: entry.vehicle_id,
                        vehicle_class: entry.vehicle_class.clone(),
                        driver_rank: entry.driver_rank.clone(),
                        driver_rank_progress: entry.driver_rank_progress,
                        visual_score: Some(rank_score),
                        race_position,
                        live_race_position: entry.position,
                        race_position_source,
                        qualifying_position: starting_position,
                        same_class_rivals,
                        rated_opponents,
                        race_result_total,
                        qualifying_result_total: qualify_result_total,
                        gain_factor: 0.0,
                        estimated_gain: None,
                        opponents: opponent_diagnostics,
                    });
                }
                continue;
            }
            let gain_factor = settings.multiplier * settings.k / (2.0 * f64::from(opponent_count));
            let gain = Self::driver_rank_gain(
                race_result_total,
                qualify_result_total,
                opponent_count,
                settings,
            );
            if entry.is_player {
                player_diagnostic = Some(DriverRankEstimateDiagnostic {
                    status: "estimated",
                    vehicle_id: entry.vehicle_id,
                    vehicle_class: entry.vehicle_class.clone(),
                    driver_rank: entry.driver_rank.clone(),
                    driver_rank_progress: entry.driver_rank_progress,
                    visual_score: Some(rank_score),
                    race_position,
                    live_race_position: entry.position,
                    race_position_source,
                    qualifying_position: starting_position,
                    same_class_rivals,
                    rated_opponents,
                    race_result_total,
                    qualifying_result_total: qualify_result_total,
                    gain_factor,
                    estimated_gain: Some(gain),
                    opponents: opponent_diagnostics,
                });
            }
            gains.push((index, gain));
        }

        for (index, gain) in gains {
            entries[index].estimated_driver_rank_gain = gain;
            entries[index].estimated_driver_rank_gain_available = true;
        }
        player_diagnostic
    }
}
