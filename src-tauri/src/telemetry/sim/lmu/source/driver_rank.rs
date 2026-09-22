//! Driver rank estimation and the diagnostics that validate it.

use super::*;
use crate::telemetry::sim::lmu::event_split::SessionSplit;

#[cfg(test)]
mod tests;

/// Only the event inputs used by DR; do not retain a second profile roster.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct DriverRankEventContext {
    pub(super) event_id: String,
    pub(super) number: u32,
    pub(super) count: u32,
    pub(super) driver_rank_settings: DriverRankSettings,
}

impl From<&SessionSplit> for DriverRankEventContext {
    fn from(split: &SessionSplit) -> Self {
        Self {
            event_id: split.event_id.clone(),
            number: split.number,
            count: split.count,
            driver_rank_settings: split.driver_rank_settings,
        }
    }
}

/// DR survives generic telemetry resets and the stopped-session frames LMU
/// exposes after a race. Neither represents another rated result.
#[derive(Default)]
pub(super) struct DriverRankRaceState {
    session_type: Option<i32>,
    game_phase: Option<u32>,
    elapsed_seconds: Option<f64>,
    event: DriverRankEventContext,
    event_confirmed: bool,
    event_request_baseline: u64,
    track_name: Option<[c_char; 64]>,
    track_length: Option<f64>,
}

impl DriverRankRaceState {
    fn is_race(&self) -> bool {
        self.session_type
            .is_some_and(|session| (10..=13).contains(&session))
    }

    fn update(
        &mut self,
        snapshot: &LmuSnapshot,
        split: &SessionSplit,
        request_revision: u64,
        resolved_request_revision: u64,
    ) -> bool {
        if snapshot.game_phase >= 9 {
            self.game_phase = Some(snapshot.game_phase);
            return false;
        }

        let is_race = (10..=13).contains(&snapshot.session_type);
        // LMU's fixed-size C string may retain arbitrary bytes after its NUL.
        let mut track_name = snapshot.track_name;
        if let Some(end) = track_name.iter().position(|byte| *byte == 0) {
            track_name[end..].fill(0);
        }
        let known_track_name = track_name[0] != 0;
        let known_track_length = snapshot.track_length.is_finite() && snapshot.track_length > 1.0;
        let track_changed = (known_track_name
            && self.track_name.is_some_and(|name| name != track_name))
            || (known_track_length
                && self
                    .track_length
                    .is_some_and(|length| (length - snapshot.track_length).abs() > 1.0));
        let new_race = is_race
            && (self.session_type != Some(snapshot.session_type)
                || (snapshot.game_phase < 8
                    && (track_changed
                        || self.game_phase.is_some_and(|phase| phase >= 8)
                        || LmuTelemetrySource::session_elapsed_regressed(
                            self.elapsed_seconds,
                            snapshot.session_elapsed_seconds,
                        ))));
        if new_race {
            self.event = DriverRankEventContext::from(split);
            self.event_confirmed = false;
            self.event_request_baseline = request_revision;
        } else if is_race
            && !self.event_confirmed
            && snapshot.game_phase < 8
            && resolved_request_revision > self.event_request_baseline
            && !split.event_id.is_empty()
        {
            // A response to a request already in flight at race entry may still
            // describe the previous event. Require a request started afterwards,
            // including when a legitimate new heat keeps the same event ID.
            self.event = DriverRankEventContext::from(split);
            self.event_confirmed = true;
        } else if is_race
            && self.event_confirmed
            && self.event.event_id.eq_ignore_ascii_case(&split.event_id)
        {
            // Resolve late metadata for this race, but never adopt the event
            // the player has registered for while the old race is still visible.
            if split.number > 0 {
                self.event.number = split.number;
            }
            if split.count > 0 {
                self.event.count = split.count;
            }
            self.event.driver_rank_settings = split.driver_rank_settings;
        }
        if !is_race || snapshot.game_phase < 8 {
            if known_track_name {
                self.track_name = Some(track_name);
            }
            if known_track_length {
                self.track_length = Some(snapshot.track_length);
            }
        }
        self.session_type = Some(snapshot.session_type);
        self.game_phase = Some(snapshot.game_phase);
        self.elapsed_seconds = snapshot
            .session_elapsed_seconds
            .is_finite()
            .then_some(snapshot.session_elapsed_seconds);
        new_race
    }
}

impl LmuTelemetrySource {
    pub(super) fn update_driver_rank_session(&mut self, snapshot: &LmuSnapshot) {
        let was_race = self.driver_rank_race.is_race();
        let (requested, resolved) = self.session_split.request_revisions();
        if self
            .driver_rank_race
            .update(snapshot, self.session_split.value(), requested, resolved)
        {
            self.scored_finish_positions.clear();
            self.driver_rank_race_sequence = self.driver_rank_race_sequence.saturating_add(1);
            self.race_qualifying_positions.clear();
            if was_race {
                self.driver_rank_prerace_scores.clear();
            }
        }
    }

    pub(super) fn driver_rank_event_context(&self, session_type: i32) -> DriverRankEventContext {
        if (10..=13).contains(&session_type) {
            self.driver_rank_race.event.clone()
        } else {
            DriverRankEventContext::from(self.session_split.value())
        }
    }

    pub(super) fn driver_rank_event_ready(&self, session_type: i32, game_phase: u32) -> bool {
        game_phase < 9
            && (!(10..=13).contains(&session_type) || self.driver_rank_race.event_confirmed)
    }

    pub(super) fn class_rank(class_name: &str) -> u8 {
        let normalized = class_name.to_ascii_uppercase();
        if normalized.contains("HYPER") || normalized.contains("GTP") {
            0
        } else if normalized.contains("LMP2") {
            1
        } else if normalized.contains("LMGT3") || normalized.contains("GT3") {
            2
        } else if normalized.contains("GTE") {
            3
        } else {
            4
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
        if crate::telemetry::dr_estimate_log::enabled() {
            self.apply_driver_rank_validation(input, crate::telemetry::dr_estimate_log::queue);
        }
    }

    fn apply_driver_rank_validation(
        &mut self,
        input: DriverRankValidationInput<'_>,
        mut emit: impl FnMut(serde_json::Value),
    ) {
        let DriverRankValidationInput {
            session_type,
            game_phase,
            event_id,
            split_number,
            sample,
            player_raw_elo,
            refresh_revision,
        } = input;
        // Phase 9 can reset the clock and scramble the roster. Keep the final
        // prediction pending for a fresh profile in a later session.
        if game_phase >= 9 {
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
                    emit(serde_json::json!({
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
                    player_driver_name: sample.driver_name.clone(),
                    player_class: sample.vehicle_class.clone(),
                    before_raw_elo: Self::usable_raw_elo(player_raw_elo),
                    before_visual_score: sample.visual_score,
                    before_refresh_revision: refresh_revision,
                    final_estimated_gain: None,
                    final_race_position: 0,
                    final_qualifying_position: 0,
                    final_position_source: "unavailable",
                    final_logged_signature: None,
                    postrace_score_candidate: None,
                    postrace_score_source: None,
                    postrace_score_samples: 0,
                });
            }

            let Some(state) = self.driver_rank_validation.as_mut() else {
                return;
            };
            if sample.driver_name != state.player_driver_name {
                state.postrace_score_candidate = None;
                state.postrace_score_source = None;
                state.postrace_score_samples = 0;
                return;
            }
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
            emit(serde_json::json!({
                "event": "driver_rank_race_final",
                "log_schema_version": DRIVER_RANK_LOG_SCHEMA_VERSION,
                "formula_version": DRIVER_RANK_FORMULA_VERSION,
                "app_version": env!("CARGO_PKG_VERSION"),
                "race_sequence": state.race_sequence,
                "event_id": state.event_id,
                "split_number": state.split_number,
                "player": {
                    "vehicle_id": state.player_vehicle_id,
                    "driver_name": state.player_driver_name,
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

        let Some(mut state) = self.driver_rank_validation.take() else {
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

        let Some(sample) = sample else {
            state.postrace_score_candidate = None;
            state.postrace_score_source = None;
            state.postrace_score_samples = 0;
            self.driver_rank_validation = Some(state);
            return;
        };
        if sample.driver_name != state.player_driver_name {
            state.postrace_score_candidate = None;
            state.postrace_score_source = None;
            state.postrace_score_samples = 0;
            self.driver_rank_validation = Some(state);
            return;
        }
        let current_visual_score = sample.visual_score;
        let after_raw_elo = Self::usable_raw_elo(player_raw_elo);
        let Some((actual_gain, actual_source)) = Self::driver_rank_actual_gain(
            state.before_raw_elo,
            after_raw_elo,
            state.before_visual_score,
            current_visual_score,
        ) else {
            state.postrace_score_candidate = None;
            state.postrace_score_source = None;
            state.postrace_score_samples = 0;
            self.driver_rank_validation = Some(state);
            return;
        };
        let observed_score = match actual_source {
            "raw_elo" => after_raw_elo,
            _ => current_visual_score,
        };
        let Some(observed_score) = observed_score else {
            state.postrace_score_candidate = None;
            state.postrace_score_source = None;
            state.postrace_score_samples = 0;
            self.driver_rank_validation = Some(state);
            return;
        };
        if state.postrace_score_source == Some(actual_source)
            && state
                .postrace_score_candidate
                .is_some_and(|candidate| (candidate - observed_score).abs() <= 0.001)
        {
            state.postrace_score_samples = state.postrace_score_samples.saturating_add(1);
        } else {
            state.postrace_score_candidate = Some(observed_score);
            state.postrace_score_source = Some(actual_source);
            state.postrace_score_samples = 1;
        }
        if state.postrace_score_samples < 2 {
            self.driver_rank_validation = Some(state);
            return;
        }
        let error = actual_gain - estimated_gain;
        emit(serde_json::json!({
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
                "driver_name": state.player_driver_name,
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

    /// La parrilla de una carrera se forma una vez y ya no cambia. RaceControl
    /// puede devolver un orden de clasificación completo pero equivocado
    /// durante un ciclo, y eso reordenaba toda la parrilla y contaminaba la
    /// estimación de DR. Conservar la primera lectura autoritativa de cada
    /// coche descarta esas reescrituras posteriores.
    pub(super) fn latch_race_qualifying_positions(
        latched: &mut HashMap<i32, i32>,
        authoritative_class_positions: &HashMap<i32, i32>,
    ) {
        for (vehicle_id, position) in authoritative_class_positions {
            latched.entry(*vehicle_id).or_insert(*position);
        }
    }

    pub(super) fn class_positions_with_preferred_class_order(
        entries: &[StandingEntry],
        preferred_class_positions: &HashMap<i32, i32>,
        fallback_class_positions: &HashMap<i32, i32>,
    ) -> HashMap<i32, i32> {
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
                driver_name: entry.driver_name.clone(),
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
                        driver_name: entry.driver_name.clone(),
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
                        driver_name: entry.driver_name.clone(),
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
                    driver_name: entry.driver_name.clone(),
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
