//! Assembles a telemetry frame from one shared-memory snapshot.

use super::*;

impl TelemetrySource for LmuTelemetrySource {
    fn descriptor(&self) -> SourceDescriptor {
        super::super::DESCRIPTOR
    }

    fn next_frame(&mut self, demand: TelemetryDemand) -> TelemetryFrame {
        let TelemetryDemand {
            include_standings,
            include_track_map,
            include_fuel_strategy,
            include_tire_life,
            include_flag_warning,
            include_rejoin_warning,
            include_rest_standings,
            include_rest_supplement,
            include_rest_weather,
        } = demand;
        const TRANSIENT_SNAPSHOT_HOLD: Duration = Duration::from_secs(2);
        const STANDINGS_STATE_INTERVAL: Duration = Duration::from_millis(200);

        let snapshot_started = Instant::now();
        let mut snapshot = LmuSnapshot::default();
        let spectator_vehicle_id = if crate::telemetry::team_mode() {
            self.team_vehicle_id().unwrap_or(-1)
        } else if crate::telemetry::spectator_mode() {
            self.spectator_vehicle_id().unwrap_or(-1)
        } else {
            -2
        };
        let result = if ffi_snapshot_layout_ok() {
            unsafe { lmu_read_snapshot(&mut snapshot, spectator_vehicle_id) }
        } else {
            0
        };
        let live_snapshot = result > 0 && snapshot.connected != 0;
        if live_snapshot {
            self.last_valid_snapshot = Some(snapshot);
            self.last_valid_snapshot_at = Some(Instant::now());
        } else if self.last_valid_snapshot_at.is_some_and(|received| {
            received.elapsed() <= TRANSIENT_SNAPSHOT_HOLD && self.last_valid_snapshot.is_some()
        }) {
            snapshot = self.last_valid_snapshot.expect("snapshot comprobado");
        }
        let snapshot_us = snapshot_started.elapsed().as_micros();
        let rest_started = Instant::now();
        self.local_rest.refresh(
            snapshot.connected != 0,
            snapshot.player_active != 0,
            include_rest_standings,
            include_rest_supplement,
            include_rest_weather,
            Self::weather_session_key(snapshot.session_type),
        );
        let rest_us = rest_started.elapsed().as_micros();

        if snapshot.connected == 0 {
            self.current_session = None;
            self.last_session_elapsed_seconds = None;
            self.local_rest.reset_session_history();
            self.last_lap = -1;
            self.fuel_at_lap_start = None;
            self.fuel_previous_sample = None;
            self.fuel_added_this_lap = 0.0;
            self.energy_at_lap_start = None;
            self.energy_previous_sample = None;
            self.energy_added_this_lap = 0.0;
            self.consumption_profiler.reset_lap();
            self.tire_wear_tracker.reset();
            self.rejoin_hold_frames = 0;
            self.last_standings_state_update = None;
            self.last_valid_standings.clear();
            self.last_valid_standings_at = None;
            self.last_valid_snapshot = None;
            self.last_valid_snapshot_at = None;
            self.player_lap_distance.reset();
            self.player_lap_times.reset();
            return TelemetryFrame::waiting_for_simulator(false);
        }

        if self.current_session == Some(snapshot.session_type)
            && Self::session_elapsed_regressed(
                self.last_session_elapsed_seconds,
                snapshot.session_elapsed_seconds,
            )
        {
            self.current_session = None;
        }
        let session_started = Instant::now();
        self.update_session(snapshot.session_type);
        self.last_session_elapsed_seconds = snapshot
            .session_elapsed_seconds
            .is_finite()
            .then_some(snapshot.session_elapsed_seconds);
        self.session_split.refresh();
        // Standings sigue el criterio preventivo de TinyPedal. El overlay de
        // banderas exige además una amarilla sectorial y proximidad.
        let standings_yellow_culprits =
            if include_standings || include_track_map || include_flag_warning {
                Self::slow_yellow_vehicles(&snapshot)
            } else {
                Default::default()
            };
        let session_us = session_started.elapsed().as_micros();
        let standings_state_due = include_standings
            || self
                .last_standings_state_update
                .is_none_or(|updated| updated.elapsed() >= STANDINGS_STATE_INTERVAL);
        let standings_state_us = if standings_state_due {
            let started = Instant::now();
            self.update_standings_state(&snapshot);
            self.update_fuel_race_pace(&snapshot);
            self.last_standings_state_update = Some(Instant::now());
            Some(started.elapsed().as_micros())
        } else {
            None
        };
        let standings_build_started = include_standings.then(Instant::now);
        let standings = if include_standings {
            let standings = self.standings(&snapshot, &standings_yellow_culprits);
            self.stable_standings(standings)
        } else {
            Vec::new()
        };
        let standings_build_us =
            standings_build_started.map(|started| started.elapsed().as_micros());

        if snapshot.player_active == 0 {
            self.last_lap = -1;
            self.fuel_at_lap_start = None;
            self.fuel_previous_sample = None;
            self.fuel_added_this_lap = 0.0;
            self.energy_at_lap_start = None;
            self.energy_previous_sample = None;
            self.energy_added_this_lap = 0.0;
            self.consumption_profiler.reset_lap();
            self.tire_wear_tracker.reset();
            self.player_lap_distance.reset();
            self.player_lap_times.reset();
            let mut frame = TelemetryFrame::waiting_for_simulator(true);
            frame.standings = standings;
            return frame;
        }

        let warnings_started = Instant::now();
        let flag_warning = if include_flag_warning {
            Self::flag_warning(&snapshot, &standings_yellow_culprits)
        } else {
            FlagWarning::default()
        };
        let rejoin_warning = if include_rejoin_warning {
            self.update_rejoin_warning(&snapshot)
        } else {
            RejoinWarning::default()
        };
        let warnings_us = warnings_started.elapsed().as_micros();
        let frame_started = Instant::now();

        // Clasificamos la vuelta terminada con los estados observados durante toda
        // la vuelta. Solo una vuelta válida, sin boxes y completamente en verde
        // puede modificar el consumo base de carrera.
        let lap_changed = snapshot.lap_number != self.last_lap;
        let reconstructed_last_lap_seconds = self.player_lap_times.update(
            snapshot.player_lap_start_elapsed_seconds,
            snapshot.current_lap_seconds,
        );
        let official_last_lap_seconds =
            CarHistory::normalize_official_lap(snapshot.last_lap_seconds);
        let displayed_last_lap_seconds = if official_last_lap_seconds > 0.0 {
            official_last_lap_seconds
        } else {
            reconstructed_last_lap_seconds
        };
        let last_lap_valid = CarHistory::completed_lap_result_is_valid(
            snapshot.last_lap_seconds,
            reconstructed_last_lap_seconds,
        );
        let in_pits = Self::player_in_pits(&snapshot);
        let player_tire_flat_spot_percent = self.tire_wear_tracker.update(
            snapshot.player_tire_remaining_by_wheel_percent,
            snapshot.player_tire_slip_ratio,
            snapshot.player_tire_sliding_fraction,
            snapshot.brake,
            in_pits,
        );
        let formation = (10..=13).contains(&snapshot.session_type) && snapshot.game_phase == 3;
        let completed_is_clean = lap_changed
            && self.last_lap >= 0
            && self.lap_was_valid
            && !CarHistory::official_lap_is_invalid(snapshot.last_lap_seconds)
            && !self.lap_visited_pits
            && !self.lap_was_formation
            && self.lap_was_green;
        self.tire_wear_tracker.observe_lap(
            snapshot.player_tire_remaining_by_wheel_percent,
            lap_changed,
            completed_is_clean,
        );
        self.update_player_lap_pace(&snapshot, completed_is_clean);
        if lap_changed {
            self.fuel_last_lap = None;
            self.energy_last_lap = None;
        }
        let (virtual_energy_percent, virtual_energy_per_lap, estimated_virtual_energy_laps) =
            self.update_energy_estimate(&snapshot, lap_changed, completed_is_clean);
        let (fuel_per_lap, estimated_fuel_laps) =
            self.update_fuel_estimate(&snapshot, lap_changed, completed_is_clean);
        self.update_qualifying_reference(&snapshot, lap_changed, completed_is_clean);
        let raw_lap_progress = if snapshot.track_length > 1.0 {
            (snapshot.player_lap_distance / snapshot.track_length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let player_validity_is_synchronized = snapshot.current_lap_seconds >= 2.0
            || raw_lap_progress * snapshot.track_length >= 300.0;
        let current_player_lap_valid =
            !player_validity_is_synchronized || snapshot.player_lap_valid != 0;
        let synchronized_progress =
            synchronized_lap_progress(raw_lap_progress, snapshot.current_lap_seconds, lap_changed);
        let lap_distance = self.player_lap_distance.update(PlayerLapDistanceSample {
            raw_distance: synchronized_progress * snapshot.track_length,
            current_lap_seconds: snapshot.current_lap_seconds,
            speed_kph: snapshot.speed_kph,
            gear: snapshot.gear,
            lap_number: snapshot.lap_number,
            track_length: snapshot.track_length,
            lap_changed,
        });
        let lap_progress = if snapshot.track_length > 1.0 {
            (lap_distance / snapshot.track_length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let fuel_used_current_lap = self
            .fuel_at_lap_start
            .map(|start| start + self.fuel_added_this_lap - snapshot.fuel_liters)
            .unwrap_or(0.0)
            .max(0.0);
        let energy_used_current_lap = self
            .energy_at_lap_start
            .map(|start| start + self.energy_added_this_lap - virtual_energy_percent)
            .unwrap_or(0.0)
            .max(0.0);
        let (vehicle_name, vehicle_livery_name) = Self::player_vehicle_names(&snapshot);
        let track_name = Self::string_from_chars(&snapshot.track_name);
        let (tc_active, abs_active) = Self::driver_assists(&snapshot);
        let (steering_angle_degrees, force_feedback) =
            Self::steering_and_force(&snapshot, self.local_rest.steering_range_degrees());
        let rest_pit_stop = self.local_rest.pit_stop().cloned();
        let rest_aero_damage = self.local_rest.aero_damage();
        let rest_suspension_damage = self.local_rest.suspension_damage();
        let rest_compound_conditions = self.local_rest.compound_conditions().to_vec();
        // The compound bolted on each axle comes named in the player's own
        // telemetry entry, which beats indexing the garage list by hand.
        let (player_front_compound, player_rear_compound) = snapshot
            .standings
            .iter()
            .find(|entry| entry.is_player != 0)
            .map(Self::axle_compound_names)
            .unwrap_or_default();
        let session_split = self.session_split.value().clone();
        let profile_estimate: ProfileEstimate = self.consumption_profiler.observe(
            &vehicle_name,
            &track_name,
            snapshot.lap_number,
            lap_progress,
            current_player_lap_valid && snapshot.game_phase == 5,
            in_pits,
            formation,
            fuel_used_current_lap,
            energy_used_current_lap,
            self.fuel_last_lap,
            self.energy_last_lap,
        );
        let fuel_per_lap = if fuel_per_lap > 0.0 {
            fuel_per_lap
        } else {
            profile_estimate.fuel_reference
        };
        let virtual_energy_per_lap = if virtual_energy_per_lap > 0.0 {
            virtual_energy_per_lap
        } else {
            profile_estimate.energy_reference
        };
        if lap_changed {
            self.last_lap = snapshot.lap_number;
            self.lap_visited_pits = in_pits;
            self.lap_was_formation = formation;
            self.lap_was_valid = true;
            self.lap_was_green = snapshot.game_phase == 5;
        } else {
            self.lap_visited_pits |= in_pits;
            self.lap_was_formation |= formation;
            self.lap_was_valid &= current_player_lap_valid;
            self.lap_was_green &= snapshot.game_phase == 5;
        }
        let lap_seconds = [
            self.lap_time_pace.unwrap_or(0.0),
            snapshot.last_lap_seconds,
            snapshot.best_lap_seconds,
            snapshot.current_lap_seconds,
        ]
        .into_iter()
        .find(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(0.0);
        // Match TinyPedal's fuel projection: use the smoothed clean player pace
        // rather than the noisier instantaneous estimated-lap field.
        let session_laps_remaining_estimated =
            Self::estimated_laps_remaining(&snapshot, lap_progress, lap_seconds);
        let session_lap_equivalents_remaining = session_laps_remaining_estimated;
        let session_laps_remaining = if session_lap_equivalents_remaining <= 0.0 {
            0.0
        } else {
            session_lap_equivalents_remaining + lap_progress
        };
        let session_total_laps_estimated =
            Self::total_laps_estimated(&snapshot, lap_progress, lap_seconds);
        let planned_fuel_per_lap = projected_consumption([
            fuel_per_lap,
            self.fuel_last_lap.unwrap_or(0.0),
            profile_estimate.fuel_reference,
            self.fuel_qualifying_lap.unwrap_or(0.0),
            0.0,
        ]);
        let virtual_energy_active = Self::uses_virtual_energy(&snapshot);
        let planned_energy_per_lap = projected_consumption([
            virtual_energy_per_lap,
            self.energy_last_lap.unwrap_or(0.0),
            profile_estimate.energy_reference,
            self.energy_qualifying_lap.unwrap_or(0.0),
            0.0,
        ]);
        let resource_autonomy = ResourceAutonomy::calculate(
            Some(snapshot.fuel_liters),
            planned_fuel_per_lap,
            Some(virtual_energy_percent),
            planned_energy_per_lap,
            virtual_energy_active,
        );
        let pit_traversal = self.pit_traversal_estimator.observe(
            &track_name,
            snapshot.track_length,
            PitSpeedSample {
                time: snapshot.session_elapsed_seconds,
                speed_ms: snapshot.speed_kph / 3.6,
                in_pits,
                limiter: snapshot.speed_limiter_active != 0,
                throttle: snapshot.throttle,
                brake: snapshot.brake,
            },
            snapshot.player_active != 0 && (include_track_map || include_rest_supplement),
        );
        let pit_traversal_seconds = pit_traversal.seconds.unwrap_or(0.0);
        let (active_amount, active_consumption, active_used, active_capacity) =
            if virtual_energy_active {
                (
                    virtual_energy_percent,
                    planned_energy_per_lap,
                    energy_used_current_lap,
                    100.0,
                )
            } else {
                (
                    snapshot.fuel_liters,
                    planned_fuel_per_lap,
                    fuel_used_current_lap,
                    snapshot.fuel_capacity_liters,
                )
            };
        let final_pit_seconds = estimated_final_pit_delay(
            active_amount,
            active_consumption,
            active_used,
            active_capacity,
            session_lap_equivalents_remaining,
            rest_pit_stop
                .as_ref()
                .map_or(0.0, |estimate| estimate.total),
            pit_traversal_seconds,
        );
        let session_extra_laps_estimated =
            Self::extra_laps_estimated(&snapshot, lap_progress, lap_seconds, final_pit_seconds);
        let fuel_full_stint_laps = (planned_fuel_per_lap > 0.0)
            .then_some(snapshot.fuel_capacity_liters / planned_fuel_per_lap);
        let energy_full_stint_laps =
            (planned_energy_per_lap > 0.0).then_some(100.0 / planned_energy_per_lap);
        let full_stint_laps = if virtual_energy_active {
            energy_full_stint_laps.map(|energy_laps| {
                fuel_full_stint_laps.map_or(energy_laps, |fuel_laps| energy_laps.min(fuel_laps))
            })
        } else {
            fuel_full_stint_laps
        };
        let tire_life_model = include_tire_life
            .then_some(full_stint_laps)
            .flatten()
            .and_then(|stint_laps| {
                self.tire_wear_tracker
                    .life_model(snapshot.player_tire_remaining_by_wheel_percent, stint_laps)
            });
        let (
            fuel_strategies,
            fuel_needed_liters,
            virtual_energy_needed_percent,
            virtual_energy_next_stint_percent,
            virtual_energy_stints_remaining,
        ) = if include_fuel_strategy {
            let fuel_race_laps =
                self.fuel_race_laps_remaining(&snapshot, lap_progress, lap_seconds);
            let fuel_margin = crate::telemetry::fuel_refuel_margin();
            let active_margin = if virtual_energy_active {
                crate::telemetry::energy_refill_margin()
            } else {
                fuel_margin
            };
            let calculate_plan = |input: ResourceStrategyInput, minimum_stops, margin| {
                calculate_lap_reference_strategy(input, lap_seconds, minimum_stops, margin)
            };
            let player_pit_stop_requested = Self::player_pit_stop_requested(&snapshot);
            let strategy_input =
                |current, capacity, consumption, pit_cycle, pit_out, laps_remaining| {
                    ResourceStrategyInput {
                        current,
                        capacity,
                        consumption,
                        laps_remaining,
                        lap_progress,
                        completed_laps: snapshot.player_total_laps,
                        pit_cycle_consumption: pit_cycle,
                        pit_out_consumption: pit_out,
                        pit_out_lap: profile_estimate.current_lap_started_in_pits,
                        pit_requested: player_pit_stop_requested,
                    }
                };
            let fuel_input = |consumption, laps_remaining| {
                strategy_input(
                    snapshot.fuel_liters,
                    snapshot.fuel_capacity_liters,
                    consumption,
                    0.0,
                    0.0,
                    laps_remaining,
                )
            };
            let energy_input = |consumption, laps_remaining| {
                strategy_input(
                    virtual_energy_percent,
                    100.0,
                    consumption,
                    0.0,
                    0.0,
                    laps_remaining,
                )
            };
            let calculate_primary = |laps_remaining| {
                let fuel_strategy = calculate_plan(
                    fuel_input(planned_fuel_per_lap, laps_remaining),
                    0,
                    fuel_margin,
                );
                let active_input = if virtual_energy_active {
                    energy_input(planned_energy_per_lap, laps_remaining)
                } else {
                    fuel_input(planned_fuel_per_lap, laps_remaining)
                };
                let parallel_minimum_stops = if virtual_energy_active {
                    fuel_strategy.map_or(0, |strategy| strategy.stops)
                } else {
                    0
                };
                let active_strategy =
                    calculate_plan(active_input, parallel_minimum_stops, active_margin);
                (fuel_strategy, active_strategy, parallel_minimum_stops)
            };
            let (fuel_strategy, active_strategy, parallel_minimum_stops) =
                calculate_primary(fuel_race_laps);
            let active_scenario = |consumption| {
                let input = if virtual_energy_active {
                    energy_input(consumption, fuel_race_laps)
                } else {
                    fuel_input(consumption, fuel_race_laps)
                };
                calculate_plan(input, parallel_minimum_stops, active_margin)
            };
            let target_consumption = if virtual_energy_active {
                virtual_energy_per_lap
            } else {
                fuel_per_lap
            };
            let target_input = if virtual_energy_active {
                energy_input(target_consumption, fuel_race_laps)
            } else {
                fuel_input(target_consumption, fuel_race_laps)
            };
            let qualifying_consumption = if virtual_energy_active {
                self.energy_qualifying_lap.unwrap_or(0.0)
            } else {
                self.fuel_qualifying_lap.unwrap_or(0.0)
            };
            let resource_service_seconds = rest_pit_stop.as_ref().map_or(0.0, |estimate| {
                if virtual_energy_active {
                    estimate.ve
                } else {
                    estimate.fuel
                }
            });
            let other_service_seconds = rest_pit_stop.as_ref().map_or(0.0, |estimate| {
                let parallel_resource = if virtual_energy_active {
                    estimate.fuel
                } else {
                    estimate.ve
                };
                parallel_resource
                    .max(estimate.tires)
                    .max(estimate.damage + estimate.brakes + estimate.brake_ducts)
                    .max(estimate.penalties)
                    .max(if estimate.driver_swap > 0.0 {
                        DRIVER_SWAP_SERVICE_SECONDS
                    } else {
                        0.0
                    })
            });
            let pit_entry_bias = crate::telemetry::track_map_model::learned_pit_entry_bias(
                &track_name,
                snapshot.track_length,
            );
            let consumption_into_lap = if virtual_energy_active {
                energy_used_current_lap
            } else {
                fuel_used_current_lap
            };
            let mut stint_targets = calculate_stint_targets(
                target_input,
                lap_seconds,
                parallel_minimum_stops,
                consumption_into_lap,
                pit_entry_bias,
                qualifying_consumption,
                self.qualifying_reference_time.unwrap_or(0.0),
                rest_pit_stop
                    .as_ref()
                    .map_or(0.0, |estimate| estimate.total),
                resource_service_seconds,
                other_service_seconds,
                pit_traversal_seconds,
            );
            for target in stint_targets.iter_mut().flatten() {
                if let Some(plan) = calculate_plan(
                    ResourceStrategyInput {
                        consumption: target.target_consumption,
                        ..target_input
                    },
                    parallel_minimum_stops,
                    active_margin,
                ) {
                    let stops_saved =
                        active_strategy.map_or(0, |active| active.stops.saturating_sub(plan.stops));
                    if target.stops_saved != stops_saved {
                        target.net_time_seconds = None;
                    }
                    target.stops_saved = stops_saved;
                }
            }
            let post_pit = ResourceAutonomy::calculate(
                self.local_rest.pit_refill_target(false),
                planned_fuel_per_lap,
                self.local_rest.pit_refill_target(true),
                planned_energy_per_lap,
                virtual_energy_active,
            );
            let (next_stint_load, _, _) = next_stint_autonomy(
                self.local_rest.pit_refill_target(virtual_energy_active),
                if virtual_energy_active {
                    planned_energy_per_lap
                } else {
                    planned_fuel_per_lap
                },
                lap_seconds,
            );
            let next_stint_laps = post_pit.range_laps;
            let next_stint_minutes = next_stint_laps
                .filter(|_| lap_seconds.is_finite() && lap_seconds > 0.0)
                .map(|laps| laps * lap_seconds / 60.0);
            let mut strategies = FuelStrategies {
                active: active_strategy,
                fuel: virtual_energy_active.then_some(fuel_strategy).flatten(),
                estimated: active_scenario(if virtual_energy_active {
                    planned_energy_per_lap
                } else {
                    planned_fuel_per_lap
                }),
                average: active_scenario(if virtual_energy_active {
                    virtual_energy_per_lap
                } else {
                    fuel_per_lap
                }),
                qualifying: active_scenario(if virtual_energy_active {
                    self.energy_qualifying_lap.unwrap_or(0.0)
                } else {
                    self.fuel_qualifying_lap.unwrap_or(0.0)
                }),
                last: active_scenario(if virtual_energy_active {
                    self.energy_last_lap.unwrap_or(0.0)
                } else {
                    self.fuel_last_lap.unwrap_or(0.0)
                }),
                stint_targets,
                next_stint_load,
                next_stint_laps,
                next_stint_minutes,
                ..FuelStrategies::default()
            }
            .with_qualifying_guidance();
            strategies.conservative_fill_active = false;
            strategies.conservative_next_fill = active_strategy.map_or(0.0, |plan| plan.next_fill);
            let needed_fuel = fuel_strategy
                .map(|strategy| {
                    snapshot.fuel_liters + strategy.total_additional - strategy.end_remaining
                })
                .unwrap_or(0.0);
            let energy_plan = if let Some(strategy) = if virtual_energy_active {
                active_strategy
            } else {
                None
            } {
                (
                    virtual_energy_percent + strategy.total_additional - strategy.end_remaining,
                    strategy.next_fill,
                    strategy.stops,
                )
            } else {
                (0.0, 0.0, 0)
            };
            (
                strategies,
                needed_fuel,
                energy_plan.0,
                energy_plan.1,
                energy_plan.2,
            )
        } else {
            (FuelStrategies::default(), 0.0, 0.0, 0.0, 0)
        };
        let track_map_vehicles = if include_track_map {
            snapshot.standings[..snapshot.standings_count.min(MAX_VEHICLES as u32) as usize]
                .iter()
                .filter(|entry| {
                    entry.vehicle_id > 0
                        && entry.world_x.is_finite()
                        && entry.world_y.is_finite()
                        && entry.in_garage == 0
                })
                .map(|entry| TrackMapVehicle {
                    vehicle_id: entry.vehicle_id,
                    overall_position: entry.position,
                    vehicle_class: Self::string_from_chars(&entry.vehicle_class),
                    world_x: entry.world_x,
                    world_y: entry.world_y,
                    lap_distance: entry.lap_distance.max(0.0),
                    total_laps: entry.total_laps,
                    in_pits: entry.in_pits != 0,
                    in_garage: entry.in_garage != 0,
                    causing_yellow: standings_yellow_culprits.contains(&entry.vehicle_id),
                    sector: entry.sector,
                    is_player: entry.is_player != 0,
                })
                .collect()
        } else {
            Vec::new()
        };
        let weather_forecast = if include_rest_weather {
            self.local_rest.weather_forecast().map(|session| {
                let mut nodes = session.forecast_nodes();
                let session_length =
                    if snapshot.session_end_seconds > snapshot.session_elapsed_seconds {
                        snapshot.session_end_seconds
                    } else {
                        self.local_rest.session_max_time_seconds()
                    };
                let progress = if session_length > 0.0 {
                    (snapshot.session_elapsed_seconds / session_length).clamp(0.0, 1.0)
                } else if snapshot.session_time_remaining > 0.0 {
                    (snapshot.session_elapsed_seconds
                        / (snapshot.session_elapsed_seconds + snapshot.session_time_remaining))
                        .clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let current_index = if nodes.is_empty() {
                    0
                } else {
                    ((progress / 0.2).floor().min((nodes.len() - 1) as f64)) as i32
                };
                let next_index = (current_index + 1).min(nodes.len() as i32);
                if session_length > 0.0 {
                    for (index, node) in nodes.iter_mut().enumerate() {
                        if index as i32 >= next_index {
                            let minutes = ((index as f64 * 0.2 * session_length
                                - snapshot.session_elapsed_seconds)
                                / 60.0)
                                .round();
                            node.minutes_from_now = Some(minutes.max(0.0) as i32);
                        }
                    }
                }
                crate::telemetry::WeatherForecastModel {
                    available: true,
                    session: Self::weather_session_key(snapshot.session_type).to_owned(),
                    current_index,
                    next_index,
                    nodes,
                }
            })
        } else {
            None
        };
        let current_humidity_percent = weather_forecast
            .as_ref()
            .and_then(|model| {
                usize::try_from(model.current_index)
                    .ok()
                    .and_then(|index| model.nodes.get(index))
            })
            .map(|node| node.humidity_percent)
            .unwrap_or(0.0);
        let rest_wind = weather_forecast.as_ref().and_then(|model| {
            usize::try_from(model.current_index).ok().and_then(|index| {
                self.local_rest
                    .weather_forecast()
                    .and_then(|session| session.wind_at(index))
            })
        });
        let (wind_speed_ms, wind_direction_degrees) = Self::resolve_wind(rest_wind);
        let wind_relative_direction_degrees = Self::relative_wind_direction_degrees(
            wind_direction_degrees,
            snapshot.player_orientation_right_z,
            snapshot.player_orientation_forward_z,
        );
        let player_grip_percent = Self::track_grip_percent(snapshot.track_grip_level);
        let track_rubber_percent = Self::track_rubber_percent(&snapshot);
        let track_grip_state = Self::track_surface_state(snapshot.track_wetness_percent);
        let frame = TelemetryFrame {
            source: super::super::DESCRIPTOR.id,
            source_name: super::super::DESCRIPTOR.display_name,
            capabilities: super::super::DESCRIPTOR.capabilities,
            performance_profile: "smooth",
            connected: true,
            spectator_mode: crate::telemetry::observer_mode(),
            player_active: true,
            game_in_foreground: snapshot.game_in_foreground != 0,
            game_in_realtime: snapshot.game_in_realtime != 0,
            player_in_garage: snapshot.player_in_garage != 0,
            session_type: snapshot.session_type,
            game_phase: snapshot.game_phase,
            session_max_laps: snapshot.max_laps,
            session_time_remaining: snapshot.session_time_remaining.max(0.0),
            session_elapsed_seconds: snapshot.session_elapsed_seconds.max(0.0),
            game_time_of_day_seconds: if snapshot.game_time_of_day_seconds.is_finite() {
                snapshot.game_time_of_day_seconds.rem_euclid(86_400.0)
            } else {
                0.0
            },
            session_max_time_seconds: self.local_rest.session_max_time_seconds(),
            leader_total_laps: snapshot.leader_total_laps,
            session_split_number: session_split.number,
            session_split_count: session_split.count,
            track_name,
            player_vehicle_name: vehicle_name,
            player_vehicle_livery_name: vehicle_livery_name,
            rest_weather_available: snapshot.ambient_temperature_c.is_finite()
                && snapshot.track_temperature_c.is_finite(),
            ambient_temperature_c: snapshot.ambient_temperature_c,
            track_temperature_c: snapshot.track_temperature_c,
            rain_percent: snapshot.rain_percent.clamp(0.0, 100.0),
            track_wetness_percent: snapshot.track_wetness_percent.clamp(0.0, 100.0),
            track_wetness_min_percent: snapshot.track_wetness_min_percent.clamp(0.0, 100.0),
            track_wetness_max_percent: snapshot.track_wetness_max_percent.clamp(0.0, 100.0),
            weather_forecast: weather_forecast.unwrap_or_default(),
            current_humidity_percent,
            wind_speed_ms,
            wind_direction_degrees,
            wind_relative_direction_degrees,
            player_grip_percent,
            track_rubber_percent,
            track_grip_state,
            cloud_coverage: Self::live_weather_icon(snapshot.cloud_coverage, snapshot.rain_percent),
            lap_number: snapshot.lap_number,
            player_sector: snapshot.player_sector,
            yellow_sectors: snapshot.yellow_sectors,
            player_total_laps: snapshot.player_total_laps,
            player_position: snapshot.player_position.max(0),
            player_class_position: snapshot.player_class_position.max(0),
            player_class_size: snapshot.player_class_size.max(0),
            player_lap_valid: current_player_lap_valid,
            player_in_pits: in_pits,
            speed_kph: snapshot.speed_kph.max(0.0),
            gear: snapshot.gear.clamp(-1, i8::MAX as i32) as i8,
            rpm: snapshot.rpm.max(0.0),
            max_rpm: snapshot.max_rpm.max(1.0),
            throttle: snapshot.throttle.clamp(0.0, 1.0),
            brake: snapshot.brake.clamp(0.0, 1.0),
            clutch: snapshot.clutch.clamp(0.0, 1.0),
            brake_bias_percent: snapshot.brake_bias_percent.clamp(0.0, 100.0),
            track_limits_steps: snapshot.track_limits_steps,
            track_limits_steps_per_penalty: snapshot.track_limits_steps_per_penalty,
            tc_active,
            abs_active,
            // Every published maximum sits at zero until the car is on track
            // with its own setup loaded, and that is also what a source without
            // electronics reports, so the maxima are the availability signal.
            car_electronics_available: snapshot.engine_map_max > 0
                || snapshot.traction_control_max > 0
                || snapshot.anti_lock_brakes_max > 0
                || snapshot.brake_migration_max > 0
                || snapshot.front_anti_roll_bar_max > 0
                || snapshot.rear_anti_roll_bar_max > 0,
            engine_map: snapshot.engine_map,
            engine_map_max: snapshot.engine_map_max,
            traction_control_level: snapshot.traction_control_level,
            traction_control_max: snapshot.traction_control_max,
            traction_control_slip: snapshot.traction_control_slip,
            traction_control_slip_max: snapshot.traction_control_slip_max,
            traction_control_cut: snapshot.traction_control_cut,
            traction_control_cut_max: snapshot.traction_control_cut_max,
            anti_lock_brakes_level: snapshot.anti_lock_brakes_level,
            anti_lock_brakes_max: snapshot.anti_lock_brakes_max,
            brake_migration: snapshot.brake_migration,
            brake_migration_max: snapshot.brake_migration_max,
            front_anti_roll_bar: snapshot.front_anti_roll_bar,
            front_anti_roll_bar_max: snapshot.front_anti_roll_bar_max,
            rear_anti_roll_bar: snapshot.rear_anti_roll_bar,
            rear_anti_roll_bar_max: snapshot.rear_anti_roll_bar_max,
            speed_limiter_active: snapshot.speed_limiter_active != 0,
            headlights_on: snapshot.headlights_on != 0,
            wiper_state: snapshot.wiper_state,
            // A GT or LMP2 car reports an unavailable motor and no charge, so
            // the panel can drop the whole hybrid block instead of drawing an
            // empty battery the driver would read as a flat one.
            hybrid_available: snapshot.hybrid_motor_state != 0
                || snapshot.battery_charge_percent > 0.0,
            battery_charge_percent: snapshot.battery_charge_percent.clamp(0.0, 100.0),
            hybrid_regen_kw: snapshot.hybrid_regen_kw,
            hybrid_motor_state: snapshot.hybrid_motor_state.min(3),
            hybrid_motor_temperature_c: snapshot.hybrid_motor_temperature_c,
            hybrid_motor_rpm: snapshot.hybrid_motor_rpm.max(0.0),
            lift_and_coast_progress: snapshot.lift_and_coast_progress.min(u8::MAX as u32) as u8,
            steering_angle_degrees,
            force_feedback,
            fuel_liters: snapshot.fuel_liters.max(0.0),
            fuel_added_this_lap: self.fuel_added_this_lap,
            fuel_capacity_liters: snapshot.fuel_capacity_liters.max(1.0),
            fuel_per_lap,
            fuel_last_lap: self.fuel_last_lap.unwrap_or(0.0),
            fuel_qualifying_lap: self.fuel_qualifying_lap.unwrap_or(0.0),
            fuel_reference_per_lap: profile_estimate.fuel_reference,
            fuel_projected_lap: profile_estimate.fuel_projected,
            fuel_pit_cycle_consumption: profile_estimate.fuel_pit_cycle_consumption,
            fuel_pit_out_consumption: profile_estimate.fuel_pit_out_consumption,
            fuel_ratio_assigned: if virtual_energy_active {
                self.local_rest.fuel_ratio_assigned()
            } else {
                0.0
            },
            fuel_ratio_average: if virtual_energy_active {
                fuel_energy_ratio(fuel_per_lap, virtual_energy_per_lap)
            } else {
                0.0
            },
            fuel_ratio_last: if virtual_energy_active {
                fuel_energy_ratio(
                    self.fuel_last_lap.unwrap_or(0.0),
                    self.energy_last_lap.unwrap_or(0.0),
                )
            } else {
                0.0
            },
            estimated_fuel_laps,
            resource_autonomy,
            pit_traversal_approximate: pit_traversal.approximate,
            session_laps_remaining,
            session_laps_remaining_estimated,
            session_lap_equivalents_remaining,
            session_total_laps_estimated,
            session_extra_laps_estimated,
            session_extra_laps_approximate: session_extra_laps_estimated.is_some()
                && final_pit_seconds > 0.0
                && pit_traversal.approximate,
            fuel_needed_liters,
            fuel_to_add_liters: if virtual_energy_active {
                fuel_strategies.fuel
            } else {
                fuel_strategies.active
            }
            .map_or(0.0, |plan| plan.total_additional),
            virtual_energy_active,
            virtual_energy_percent,
            virtual_energy_raw: snapshot.virtual_energy,
            virtual_energy_added_this_lap: self.energy_added_this_lap,
            virtual_energy_per_lap,
            virtual_energy_last_lap: self.energy_last_lap.unwrap_or(0.0),
            virtual_energy_qualifying_lap: self.energy_qualifying_lap.unwrap_or(0.0),
            virtual_energy_reference_per_lap: profile_estimate.energy_reference,
            virtual_energy_projected_lap: profile_estimate.energy_projected,
            virtual_energy_pit_cycle_consumption: profile_estimate.energy_pit_cycle_consumption,
            virtual_energy_pit_out_consumption: profile_estimate.energy_pit_out_consumption,
            player_pit_out_lap: profile_estimate.current_lap_started_in_pits,
            estimated_virtual_energy_laps,
            virtual_energy_needed_percent,
            virtual_energy_next_stint_percent,
            virtual_energy_stints_remaining,
            fuel_strategies,
            standings_model: Default::default(),
            relative_model: Default::default(),
            player_tire_remaining_percent: snapshot.player_tire_remaining_percent,
            player_damage_percent: snapshot.player_damage_percent.clamp(0.0, 100.0),
            player_aero_damage_percent: rest_aero_damage
                .map(|aero| aero * 100.0)
                .unwrap_or(-1.0)
                .clamp(-1.0, 100.0),
            player_suspension_damage_percent: suspension_damage_percent(
                rest_suspension_damage,
                snapshot.player_tire_detached,
            ),
            player_suspension_damage_by_wheel_percent: suspension_damage_by_wheel_percent(
                rest_suspension_damage,
                snapshot.player_tire_detached,
            ),
            player_body_damage_percent: snapshot
                .player_damage_severity
                .into_iter()
                .map(|severity| severity.min(2) as f64)
                .sum::<f64>()
                / 16.0
                * 100.0,
            player_damage_severity: snapshot.player_damage_severity,
            player_engine_overheating: snapshot.player_engine_overheating != 0,
            player_engine_oil_temperature_c: snapshot.player_engine_oil_temperature_c,
            player_engine_water_temperature_c: snapshot.player_engine_water_temperature_c,
            player_part_detached: snapshot.player_body_part_detached != 0
                || snapshot.player_tire_detached.contains(&1),
            player_rear_wing_detached: rear_wing_detached(snapshot.player_body_part_detached != 0),
            player_tire_temperature_c: snapshot.player_tire_temperature_c,
            player_tire_temperature_by_zone_c: snapshot.player_tire_temperature_by_zone_c,
            player_brake_temperature_c: snapshot.player_brake_temperature_c,
            player_tire_sliding_fraction: snapshot.player_tire_sliding_fraction,
            player_tire_remaining_by_wheel_percent: snapshot.player_tire_remaining_by_wheel_percent,
            tire_life_model,
            player_tire_flat_spot_percent,
            player_tire_compounds: Self::tire_compounds(
                &snapshot.player_tire_compounds,
                (&player_front_compound, &player_rear_compound),
            ),
            player_tire_optimal_temperature_c: Self::tire_optimal_temperatures(
                &snapshot.player_tire_compounds,
                (&player_front_compound, &player_rear_compound),
                &rest_compound_conditions,
            ),
            player_tire_flat: snapshot.player_tire_flat.map(|value| value != 0),
            player_tire_detached: snapshot.player_tire_detached.map(|value| value != 0),
            player_stint: snapshot
                .standings
                .iter()
                .find(|entry| entry.is_player != 0)
                .map(|entry| entry.pit_stops + 1)
                .unwrap_or(0),
            player_strategy_pit: in_pits,
            pit_stop_estimate_available: rest_pit_stop.is_some(),
            pit_stop_estimate_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| estimate.total.max(0.0))
                .unwrap_or(0.0),
            pit_stop_fuel_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| estimate.fuel.max(0.0))
                .unwrap_or(0.0),
            pit_stop_energy_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| estimate.ve.max(0.0))
                .unwrap_or(0.0),
            pit_stop_tire_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| estimate.tires.max(0.0))
                .unwrap_or(0.0),
            pit_stop_damage_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| (estimate.damage + estimate.brakes + estimate.brake_ducts).max(0.0))
                .unwrap_or(0.0),
            pit_stop_penalty_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| estimate.penalties.max(0.0))
                .unwrap_or(0.0),
            pit_stop_driver_swap_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| estimate.driver_swap.max(0.0))
                .unwrap_or(0.0),
            lap_progress: profile_estimate.lap_progress,
            track_length_meters: snapshot.track_length.max(0.0),
            track_map_vehicles,
            track_map_model: Default::default(),
            consumption_profile_samples: profile_estimate.samples,
            current_lap_seconds: snapshot.current_lap_seconds.max(0.0),
            current_sector1_seconds: snapshot.current_sector1_seconds.max(0.0),
            current_sector2_seconds: snapshot.current_sector2_seconds.max(0.0),
            player_best_sector_ends: snapshot.player_best_sector_ends,
            player_sector_states: ["pending"; 3],
            class_best_sector_ends: Self::class_best_sector_ends(&snapshot),
            last_lap_seconds: displayed_last_lap_seconds,
            last_lap_valid,
            best_lap_seconds: snapshot.best_lap_seconds.max(0.0),
            lap_delta_seconds: snapshot.lap_delta_seconds,
            delta_model: Default::default(),
            timing_model: Default::default(),
            stint_history_model: Default::default(),
            flag_warning,
            rejoin_warning,
            standings,
        };
        self.source_stage_performance.record(SourceStageSample {
            snapshot_us,
            rest_us,
            session_us,
            standings_state_us,
            standings_build_us,
            warnings_us,
            frame_us: frame_started.elapsed().as_micros(),
        });
        frame
    }
}
