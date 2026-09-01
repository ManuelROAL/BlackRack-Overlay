//! The standings and relative models.

use super::*;

impl LmuTelemetrySource {
    pub(super) fn rest_finish_status(value: &str) -> Option<u32> {
        match value.trim().to_ascii_uppercase().as_str() {
            "FSTAT_FINISHED" | "FINISHED" => Some(1),
            "FSTAT_DNF" | "DNF" => Some(2),
            "FSTAT_DQ" | "FSTAT_DISQUALIFIED" | "DQ" | "DISQUALIFIED" => Some(3),
            _ => None,
        }
    }

    pub(super) fn update_standings_state(&mut self, snapshot: &LmuSnapshot) {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let mut raw_entries = snapshot.standings[..count]
            .iter()
            .filter(|entry| entry.position > 0)
            .collect::<Vec<_>>();
        raw_entries.sort_by_key(|entry| entry.position);

        let mut class_positions = HashMap::<[c_char; 32], i32>::new();
        for entry in raw_entries {
            self.ensure_vehicle_identity(entry);
            let current_energy = Self::virtual_energy_percent(entry.virtual_energy);
            self.car_histories
                .entry(entry.vehicle_id)
                .or_default()
                .update(entry, current_energy);

            let class_position = class_positions.entry(entry.vehicle_class).or_default();
            *class_position += 1;
            if (10..=13).contains(&snapshot.session_type) {
                if snapshot.game_phase <= 4 {
                    self.starting_positions
                        .insert(entry.vehicle_id, *class_position);
                } else {
                    self.starting_positions
                        .entry(entry.vehicle_id)
                        .or_insert(*class_position);
                }
            }
        }
    }

    pub(super) fn standings(
        &mut self,
        snapshot: &LmuSnapshot,
        yellow_culprits: &HashSet<i32>,
    ) -> Vec<StandingEntry> {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let mut raw_entries = snapshot.standings[..count]
            .iter()
            .filter(|entry| entry.position > 0)
            .collect::<Vec<_>>();
        raw_entries.sort_by_key(|entry| entry.position);

        // Mantiene el método autocontenido para las pruebas y para cualquier
        // refresco forzado. En el ciclo normal son comparaciones de arrays ya
        // cacheados, sin volver a convertir cadenas.
        for entry in &raw_entries {
            self.ensure_vehicle_identity(entry);
        }
        let driver_names = raw_entries
            .iter()
            .filter_map(|entry| self.vehicle_identities.get(&entry.vehicle_id))
            .map(|identity| identity.driver_name.as_str())
            .collect::<Vec<_>>();
        self.driver_ranks.refresh(&driver_names);

        let player_class = raw_entries
            .iter()
            .find(|entry| entry.is_player != 0)
            .and_then(|entry| self.vehicle_identities.get(&entry.vehicle_id))
            .map(|identity| identity.vehicle_class.clone());
        let now = Instant::now();
        let log_gap_sample = crate::telemetry::analysis_logging_generation().is_some()
            && self
                .last_gap_log_at
                .is_none_or(|previous| now.duration_since(previous) >= Duration::from_secs(1));
        if log_gap_sample {
            self.last_gap_log_at = Some(now);
        }
        let log_driver_rank_sample = crate::telemetry::dr_estimate_log::enabled();
        let mut gap_sample = Vec::new();

        let mut fastest_by_class = HashMap::<String, f64>::new();
        for entry in &raw_entries {
            if entry.best_lap_seconds <= 0.0 {
                continue;
            }
            let identity = self
                .vehicle_identities
                .get(&entry.vehicle_id)
                .expect("identidad de vehículo inicializada");
            fastest_by_class
                .entry(identity.vehicle_class.clone())
                .and_modify(|best| *best = best.min(entry.best_lap_seconds))
                .or_insert(entry.best_lap_seconds);
        }

        let mut class_positions = HashMap::<String, i32>::new();
        let mut class_leaders = HashMap::<String, &LmuStandingEntry>::new();
        let mut previous_in_class = HashMap::<String, &LmuStandingEntry>::new();
        let mut driver_rank_scores = HashMap::<i32, f64>::new();
        let mut driver_qualifying_overall_positions = HashMap::<i32, i32>::new();
        let mut scored_overall_positions = HashMap::<i32, i32>::new();
        let mut player_driver_elo = None;
        let mut player_profile_revision = 0;
        let player_entry = raw_entries
            .iter()
            .find(|entry| entry.is_player != 0)
            .copied();
        let mut entries = Vec::with_capacity(raw_entries.len());

        for entry in raw_entries {
            let identity = self
                .vehicle_identities
                .get(&entry.vehicle_id)
                .expect("identidad de vehículo inicializada");
            let vehicle_class = identity.vehicle_class.clone();
            let rest = self
                .local_rest
                .standing(entry.vehicle_id, &identity.driver_name)
                .cloned();
            if snapshot.game_phase == 8 {
                if let Some(position) = rest
                    .as_ref()
                    .filter(|standing| standing.server_scored)
                    .map(|standing| standing.position)
                    .filter(|position| *position > 0)
                {
                    scored_overall_positions.insert(entry.vehicle_id, position);
                }
            }
            let class_position = class_positions.entry(vehicle_class.clone()).or_default();
            *class_position += 1;
            let class_position = *class_position;

            let current_energy = Self::virtual_energy_percent(entry.virtual_energy);
            let historical_lap_times = self
                .local_rest
                .history(entry.vehicle_id, &identity.driver_name)
                .map(|history| {
                    history
                        .iter()
                        .filter_map(|lap| {
                            (lap.total_laps > 0 && lap.lap_time.is_finite() && lap.lap_time > 0.0)
                                .then_some((lap.total_laps, lap.lap_time))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let history = self.car_histories.entry(entry.vehicle_id).or_default();
            history.seed_recent_lap_times(entry.total_laps, &historical_lap_times);
            let average_lap_seconds = history.average_lap_time();
            let average_energy_usage = history.average_energy_usage();
            let is_out_lap = history.is_out_lap();
            let pit_stop_lap = history.pit_stop_lap();
            let pit_stop_time_seconds = history.pit_stop_time_seconds();

            let starting_position = self
                .local_rest
                .starting_class_position(
                    entry.vehicle_id,
                    &identity.driver_name,
                    &identity.vehicle_class,
                )
                .or_else(|| self.starting_positions.get(&entry.vehicle_id).copied())
                .unwrap_or(class_position);
            let initial_class_count = self
                .local_rest
                .initial_class_count(&identity.vehicle_class)
                .max(class_position as usize);

            let leader = *class_leaders.entry(vehicle_class.clone()).or_insert(entry);
            let (laps_behind_leader, time_behind_leader) =
                Self::class_relative_gap(leader, entry, snapshot.track_length);
            let previous = previous_in_class
                .insert(vehicle_class.clone(), entry)
                .unwrap_or(entry);
            let (laps_behind_next, interval) = if class_position > 1 {
                Self::class_relative_gap(previous, entry, snapshot.track_length)
            } else {
                (0, 0.0)
            };

            if log_gap_sample && player_class.as_deref() == Some(vehicle_class.as_str()) {
                let leader_progress =
                    leader.total_laps as f64 + leader.lap_distance / snapshot.track_length.max(1.0);
                let entry_progress =
                    entry.total_laps as f64 + entry.lap_distance / snapshot.track_length.max(1.0);
                gap_sample.push(serde_json::json!({
                    "vehicle_id": entry.vehicle_id,
                    "driver": identity.driver_name.as_str(),
                    "overall_position": entry.position,
                    "class_position": class_position,
                    "is_player": entry.is_player != 0,
                    "total_laps": entry.total_laps,
                    "lap_distance": entry.lap_distance,
                    "time_into_lap": entry.time_into_lap,
                    "estimated_lap_time": entry.estimated_lap_time,
                    "raw_laps_behind_overall_leader": entry.laps_behind_leader,
                    "raw_time_behind_overall_leader": entry.time_behind_leader,
                    "raw_time_behind_overall_next": entry.interval,
                    "class_leader_vehicle_id": leader.vehicle_id,
                    "class_leader_total_laps": leader.total_laps,
                    "class_leader_lap_distance": leader.lap_distance,
                    "class_leader_time_into_lap": leader.time_into_lap,
                    "class_leader_estimated_lap_time": leader.estimated_lap_time,
                    "class_progress_diff": (leader_progress - entry_progress),
                    "gap_laps": laps_behind_leader,
                    "gap_seconds": time_behind_leader,
                    "interval_laps": laps_behind_next,
                    "interval_seconds": interval,
                }));
            }

            let virtual_energy_active = matches!(entry.vehicle_class_id, 0 | 6);
            let fastest_lap = fastest_by_class.get(&vehicle_class).copied().unwrap_or(0.0);
            let mut ranks = self
                .driver_ranks
                .lookup(&identity.driver_name)
                .unwrap_or_default();
            if let Some(event_profile) = self.session_split.value().profile(&identity.driver_name) {
                if ranks.nationality.is_empty() || ranks.nationality == "XX" {
                    ranks.nationality = event_profile.nationality.clone();
                }
                if ranks.badge.is_empty() {
                    ranks.badge = event_profile.badge.clone();
                }
            }
            if entry.is_player != 0 && ranks.driver_elo.is_finite() && ranks.driver_elo >= 0.0 {
                player_driver_elo = Some(ranks.driver_elo);
                player_profile_revision = self.driver_ranks.profile_revision(&identity.driver_name);
            }
            if let Some(score) = Self::driver_rank_score(&ranks.driver, ranks.driver_progress) {
                let driver_key = normalized_name(&identity.driver_name);
                let estimate_score = if (10..=13).contains(&snapshot.session_type) {
                    *self
                        .driver_rank_prerace_scores
                        .entry(driver_key)
                        .or_insert(score)
                } else {
                    self.driver_rank_prerace_scores.insert(driver_key, score);
                    score
                };
                driver_rank_scores.insert(entry.vehicle_id, estimate_score);
            }
            if let Some(qualification) = rest
                .as_ref()
                .map(|standing| standing.qualification)
                .filter(|qualification| *qualification > 0)
            {
                driver_qualifying_overall_positions.insert(entry.vehicle_id, qualification);
            }
            let (relative_ahead_seconds, relative_behind_seconds) = player_entry
                .map(|player| Self::relative_gaps_seconds(player, entry))
                .unwrap_or((0.0, 0.0));
            let relative_gap_seconds = if relative_ahead_seconds.abs() <= relative_behind_seconds {
                relative_ahead_seconds
            } else {
                relative_behind_seconds
            };
            let laps_relative_to_player = player_entry
                .map(|player| Self::laps_relative_to_player(player, entry))
                .unwrap_or(0);

            entries.push(StandingEntry {
                vehicle_id: entry.vehicle_id,
                overall_position: entry.position,
                position: class_position,
                position_change: starting_position - class_position,
                car_number: rest
                    .as_ref()
                    .map(|standing| standing.car_number.trim())
                    .filter(|number| !number.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| identity.fallback_car_number.clone()),
                driver_name: identity.driver_name.clone(),
                driver_rank: ranks.driver,
                driver_rank_progress: ranks.driver_progress,
                estimated_driver_rank_gain: 0.0,
                estimated_driver_rank_gain_available: false,
                safety_rank: ranks.safety,
                safety_rank_progress: ranks.safety_progress,
                nationality: ranks.nationality,
                driver_badge: ranks.badge,
                team_name: identity.team_name.clone(),
                vehicle_name: identity.vehicle_name.clone(),
                vehicle_class,
                initial_class_count,
                laps_relative_to_player,
                total_laps: entry.total_laps,
                laps_behind_leader,
                laps_behind_next,
                time_behind_leader,
                interval,
                relative_gap_seconds,
                relative_ahead_seconds,
                relative_behind_seconds,
                best_lap_seconds: entry.best_lap_seconds.max(0.0),
                last_lap_seconds: history.last_lap_seconds(entry.last_lap_seconds),
                average_lap_seconds,
                virtual_energy_active,
                virtual_energy_percent: if virtual_energy_active {
                    rest.as_ref()
                        .map(|standing| standing.ve_fraction * 100.0)
                        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= 100.0)
                        .unwrap_or(current_energy)
                } else {
                    0.0
                },
                virtual_energy_per_lap: if matches!(entry.vehicle_class_id, 0 | 6) {
                    average_energy_usage
                } else {
                    0.0
                },
                damage_percent: entry.damage_percent.clamp(0.0, 100.0),
                track_limits_steps: (entry.track_limits_available != 0)
                    .then_some(entry.track_limits_steps),
                // REST can overcount this value in team races. Shared memory's
                // per-vehicle mNumPitstops is the authoritative completed-stop
                // counter and also drives the pit-cycle confirmation above.
                pit_stops: entry.pit_stops,
                pit_stop_requested: entry.pit_state == 1
                    || rest.as_ref().is_some_and(|standing| {
                        matches!(
                            standing.pit_state.trim().to_ascii_uppercase().as_str(),
                            "REQUEST" | "REQUESTED"
                        )
                    }),
                pit_stop_lap,
                pit_stop_time_seconds,
                tire_compound: identity.tire_compound.clone(),
                tire_compounds: Self::tire_compounds(&entry.wheel_compounds),
                flag: entry.flag,
                causing_yellow: yellow_culprits.contains(&entry.vehicle_id),
                has_fastest_lap: fastest_lap > 0.0
                    && (entry.best_lap_seconds - fastest_lap).abs() <= 0.001,
                in_pits: entry.in_pits != 0
                    || rest.as_ref().is_some_and(|standing| standing.pitting),
                in_garage: entry.in_garage != 0
                    || rest
                        .as_ref()
                        .is_some_and(|standing| standing.in_garage_stall),
                is_out_lap,
                last_lap_valid: history.is_last_lap_valid(),
                penalty_count: entry.penalties,
                finish_status: rest
                    .as_ref()
                    .and_then(|standing| Self::rest_finish_status(&standing.finish_status))
                    .unwrap_or(entry.finish_status),
                is_player: entry.is_player != 0,
            });
        }

        let driver_qualifying_positions = Self::class_positions_with_complete_preferred_order(
            &entries,
            &driver_qualifying_overall_positions,
            &self.starting_positions,
        );
        for entry in &mut entries {
            let starting_position = driver_qualifying_positions
                .get(&entry.vehicle_id)
                .copied()
                .unwrap_or(entry.position);
            entry.position_change = starting_position - entry.position;
        }

        let latest_scored_positions =
            Self::scored_class_positions(&entries, &scored_overall_positions);
        self.scored_finish_positions.extend(latest_scored_positions);
        let driver_rank_diagnostic = Self::update_driver_rank_estimates(
            &mut entries,
            &driver_rank_scores,
            &driver_qualifying_positions,
            &self.scored_finish_positions,
            snapshot.session_type,
            self.session_split.value().driver_rank_settings,
        );

        let authenticated_player_elo = self.driver_ranks.authenticated_player_elo();
        let authenticated_elo_revision = self.driver_ranks.authenticated_player_elo_revision();
        let (validation_player_elo, player_rank_refresh_revision) = if authenticated_player_elo
            .is_some()
            && authenticated_elo_revision > player_profile_revision
        {
            (authenticated_player_elo, authenticated_elo_revision)
        } else {
            (player_driver_elo, player_profile_revision)
        };
        if player_driver_elo.is_none() {
            player_driver_elo = authenticated_player_elo;
        }
        if player_driver_elo.is_none() {
            player_driver_elo = self.session_split.value().player_driver_elo;
        }

        if log_driver_rank_sample {
            let split = self.session_split.value();
            if let Some(sample) = driver_rank_diagnostic.as_ref() {
                if (0..=8).contains(&snapshot.session_type) {
                    crate::telemetry::dr_estimate_log::queue(serde_json::json!({
                        "event": "driver_rank_current_sample",
                        "log_schema_version": DRIVER_RANK_LOG_SCHEMA_VERSION,
                        "formula_version": DRIVER_RANK_FORMULA_VERSION,
                        "app_version": env!("CARGO_PKG_VERSION"),
                        "race_sequence": self.driver_rank_race_sequence,
                        "event_id": split.event_id,
                        "split_number": split.number,
                        "split_count": split.count,
                        "session_type": snapshot.session_type,
                        "status": sample.status,
                        "player": {
                            "vehicle_id": sample.vehicle_id,
                            "vehicle_class": sample.vehicle_class,
                            "driver_rank": sample.driver_rank,
                            "driver_rank_progress": sample.driver_rank_progress,
                            "raw_elo": player_driver_elo,
                            "visual_score": sample.visual_score,
                            "internal_score": sample.visual_score.map(|score| score * DRIVER_RANK_INTERNAL_SCALE),
                        },
                    }));
                } else {
                    crate::telemetry::dr_estimate_log::queue(serde_json::json!({
                        "event": "driver_rank_estimate_sample",
                        "log_schema_version": DRIVER_RANK_LOG_SCHEMA_VERSION,
                        "formula_version": DRIVER_RANK_FORMULA_VERSION,
                        "app_version": env!("CARGO_PKG_VERSION"),
                        "race_sequence": self.driver_rank_race_sequence,
                        "event_id": split.event_id,
                        "split_number": split.number,
                        "split_count": split.count,
                        "session_type": snapshot.session_type,
                        "game_phase": snapshot.game_phase,
                        "status": sample.status,
                        "player": {
                            "vehicle_id": sample.vehicle_id,
                            "vehicle_class": sample.vehicle_class,
                            "driver_rank": sample.driver_rank,
                            "driver_rank_progress": sample.driver_rank_progress,
                            "raw_elo": player_driver_elo,
                            "visual_score": sample.visual_score,
                            "internal_score": sample.visual_score.map(|score| score * DRIVER_RANK_INTERNAL_SCALE),
                            "race_position": sample.race_position,
                            "live_race_position": sample.live_race_position,
                            "race_position_source": sample.race_position_source,
                            "qualifying_position": sample.qualifying_position,
                        },
                        "coverage": {
                            "same_class_rivals": sample.same_class_rivals,
                            "rated_opponents": sample.rated_opponents,
                        },
                        "calculation": {
                            "race_result_total": sample.race_result_total,
                            "qualifying_result_total": sample.qualifying_result_total,
                            "qualifying_weight": DRIVER_RANK_QUALIFY_WEIGHT,
                            "gain_factor": sample.gain_factor,
                            "estimated_gain": sample.estimated_gain,
                            "opponents": sample.opponents.iter().map(|opponent| serde_json::json!({
                                "vehicle_id": opponent.vehicle_id,
                                "visual_score": opponent.visual_score,
                                "internal_score": opponent.internal_score,
                                "race_position": opponent.race_position,
                                "qualifying_position": opponent.qualifying_position,
                                "expected": opponent.expected,
                                "race_result": opponent.race_result,
                                "qualifying_result": opponent.qualifying_result,
                            })).collect::<Vec<_>>(),
                        },
                        "settings": {
                            "multiplier": split.driver_rank_settings.multiplier,
                            "k": split.driver_rank_settings.k,
                            "distance": split.driver_rank_settings.distance,
                            "logarithm": split.driver_rank_settings.logarithm,
                        },
                    }));
                }
            }
        }

        let split = self.session_split.value().clone();
        self.update_driver_rank_validation(DriverRankValidationInput {
            session_type: snapshot.session_type,
            game_phase: snapshot.game_phase,
            event_id: &split.event_id,
            split_number: split.number,
            sample: driver_rank_diagnostic.as_ref(),
            player_raw_elo: validation_player_elo,
            refresh_revision: player_rank_refresh_revision,
        });

        if !gap_sample.is_empty() {
            crate::telemetry::queue_analysis_event(serde_json::json!({
                "event": "standings_gap_sample",
                "session_type": snapshot.session_type,
                "game_phase": snapshot.game_phase,
                "track_length": snapshot.track_length,
                "player_class": player_class.unwrap_or_default(),
                "cars": gap_sample,
            }));
        }

        entries.sort_by(|left, right| {
            Self::class_rank(&left.vehicle_class)
                .cmp(&Self::class_rank(&right.vehicle_class))
                .then_with(|| left.vehicle_class.cmp(&right.vehicle_class))
                .then_with(|| left.position.cmp(&right.position))
        });
        entries
    }

    pub(super) fn stable_standings(&mut self, standings: Vec<StandingEntry>) -> Vec<StandingEntry> {
        const TRANSIENT_EMPTY_HOLD: Duration = Duration::from_secs(2);

        if !standings.is_empty() {
            self.last_valid_standings.clone_from(&standings);
            self.last_valid_standings_at = Some(Instant::now());
            return standings;
        }

        if self.last_valid_standings_at.is_some_and(|received| {
            received.elapsed() <= TRANSIENT_EMPTY_HOLD && !self.last_valid_standings.is_empty()
        }) {
            return self.last_valid_standings.clone();
        }

        self.last_valid_standings.clear();
        self.last_valid_standings_at = None;
        standings
    }

    /// TinyPedal considera que un coche está provocando (o a punto de provocar)
    /// una amarilla cuando circula a menos de 8 m/s. En standings el indicador
    /// es deliberadamente preventivo: no depende de que LMU ya haya activado la
    /// bandera sectorial.
    pub(super) fn slow_yellow_vehicles(snapshot: &LmuSnapshot) -> HashSet<i32> {
        const YELLOW_SPEED_THRESHOLD_KPH: f64 = 8.0 * 3.6;
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);

        snapshot.standings[..count]
            .iter()
            .filter(|entry| {
                entry.vehicle_id != 0
                    && entry.in_pits == 0
                    && entry.in_garage == 0
                    && entry.pit_state < 2
                    && entry.speed_kph.is_finite()
                    && entry.speed_kph < YELLOW_SPEED_THRESHOLD_KPH
            })
            .map(|entry| entry.vehicle_id)
            .collect()
    }

    pub(super) fn class_relative_gap(
        ahead: &LmuStandingEntry,
        behind: &LmuStandingEntry,
        track_length: f64,
    ) -> (i32, f64) {
        if !track_length.is_finite() || track_length <= 0.0 {
            return (0, 0.0);
        }
        let ahead_progress = ahead.total_laps as f64 + ahead.lap_distance / track_length;
        let behind_progress = behind.total_laps as f64 + behind.lap_distance / track_length;
        let lap_diff = ahead_progress - behind_progress;
        if lap_diff >= 1.0 {
            return (lap_diff as i32, 0.0);
        }
        let mut time_gap = ahead.time_into_lap - behind.time_into_lap;
        if time_gap < 0.0 && lap_diff > 0.0 {
            time_gap += behind.estimated_lap_time.max(1.0);
        }
        (0, time_gap.abs().max(0.0))
    }

    pub(super) fn relative_gaps_seconds(
        player: &LmuStandingEntry,
        entry: &LmuStandingEntry,
    ) -> (f64, f64) {
        let lap_time = player.estimated_lap_time;
        if player.vehicle_id == entry.vehicle_id
            || entry.in_garage != 0
            || !lap_time.is_finite()
            || lap_time <= 1.0
            || !player.time_into_lap.is_finite()
            || !entry.time_into_lap.is_finite()
        {
            return (0.0, 0.0);
        }

        // TinyPedal-style circular timing: every opponent has one possible
        // occurrence ahead and another behind. The displayed convention is
        // negative ahead and positive behind.
        let ahead = (entry.time_into_lap - player.time_into_lap).rem_euclid(lap_time);
        if ahead <= f64::EPSILON {
            return (0.0, 0.0);
        }
        (-ahead, lap_time - ahead)
    }

    pub(super) fn laps_relative_to_player(
        player: &LmuStandingEntry,
        entry: &LmuStandingEntry,
    ) -> i32 {
        let lap_time = player.estimated_lap_time;
        if player.vehicle_id == entry.vehicle_id
            || !lap_time.is_finite()
            || lap_time <= 1.0
            || !player.time_into_lap.is_finite()
            || !entry.time_into_lap.is_finite()
        {
            return 0;
        }

        // Completed laps alone briefly differ when only one car has crossed the
        // timing line. Adding the continuous phase difference removes that false
        // lap before rounding to the actual race-lap relationship.
        let completed_delta = f64::from(entry.total_laps - player.total_laps);
        let phase_delta = (entry.time_into_lap - player.time_into_lap) / lap_time;
        (completed_delta + phase_delta).round() as i32
    }

    pub(super) fn track_distance(from: f64, to: f64, track_length: f64) -> f64 {
        if !from.is_finite() || !to.is_finite() || track_length <= 1.0 {
            return 0.0;
        }
        (to - from).rem_euclid(track_length)
    }
}
