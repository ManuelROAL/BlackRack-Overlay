//! The flag and rejoin warning models.

use super::*;

impl LmuTelemetrySource {
    pub(super) fn flag_warning(
        snapshot: &LmuSnapshot,
        yellow_culprits: &HashSet<i32>,
    ) -> FlagWarning {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let raw = &snapshot.standings[..count];
        let Some(player) = raw.iter().find(|entry| entry.is_player != 0) else {
            return FlagWarning::default();
        };

        // La bandera a cuadros pertenece al estado de sesión/coche, no a
        // mFlag (el SDK documenta ese campo solamente para verde y azul).
        // Debe prevalecer sobre cualquier amarilla o azul todavía activa.
        if player.finish_status == 1 || snapshot.game_phase == 8 {
            return FlagWarning {
                kind: "checkered",
                active: true,
                distance_meters: 0.0,
                car_position: player.position,
                vehicle_class: String::new(),
            };
        }

        // TinyPedal solo enseña el aviso si LMU confirma una amarilla sectorial.
        // Busca primero un coche lento hasta 500 m por delante y, si no existe,
        // uno hasta 50 m por detrás.
        if snapshot.yellow_sectors != 0 {
            let ahead = raw
                .iter()
                .filter(|entry| yellow_culprits.contains(&entry.vehicle_id))
                .map(|entry| {
                    (
                        entry.vehicle_id,
                        Self::track_distance(
                            player.lap_distance,
                            entry.lap_distance,
                            snapshot.track_length,
                        ),
                    )
                })
                .filter(|(_, distance)| *distance <= 500.0)
                .min_by(|left, right| left.1.total_cmp(&right.1));

            if let Some((vehicle_id, distance)) = ahead {
                return Self::warning_for_car("yellow", vehicle_id, distance, snapshot);
            }

            let behind = raw
                .iter()
                .filter(|entry| yellow_culprits.contains(&entry.vehicle_id))
                .map(|entry| {
                    (
                        entry.vehicle_id,
                        Self::track_distance(
                            entry.lap_distance,
                            player.lap_distance,
                            snapshot.track_length,
                        ),
                    )
                })
                .filter(|(_, distance)| *distance <= 50.0)
                .min_by(|left, right| left.1.total_cmp(&right.1));

            if let Some((vehicle_id, distance)) = behind {
                // Igual que TinyPedal: positivo indica delante y negativo detrás.
                return Self::warning_for_car("yellow", vehicle_id, -distance, snapshot);
            }
        }

        if player.flag != 6 {
            return FlagWarning::default();
        }

        // mFlag confirma que el jugador recibe azul, pero no identifica al coche.
        // Elegimos el coche más cercano por detrás; priorizamos al que lleva una
        // vuelta de ventaja o tiene ritmo claramente superior.
        let mut candidates = raw
            .iter()
            .filter(|entry| {
                entry.vehicle_id != player.vehicle_id
                    && entry.in_pits == 0
                    && entry.in_garage == 0
                    && entry.finish_status == 0
            })
            .filter_map(|entry| {
                let distance = Self::track_distance(
                    entry.lap_distance,
                    player.lap_distance,
                    snapshot.track_length,
                );
                (distance > 1.0 && distance <= 2_000.0).then_some((entry, distance))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.1.total_cmp(&right.1));
        let is_plausible_blue_car = |entry: &LmuStandingEntry| {
            entry.total_laps > player.total_laps
                || (entry.best_lap_seconds > 0.0
                    && player.best_lap_seconds > 0.0
                    && entry.best_lap_seconds < player.best_lap_seconds * 0.98)
        };
        let target = candidates
            .iter()
            .find(|(entry, _)| is_plausible_blue_car(entry))
            .or_else(|| candidates.first());

        target.map_or_else(FlagWarning::default, |(entry, distance)| {
            Self::warning_for_car("blue", entry.vehicle_id, *distance, snapshot)
        })
    }

    pub(super) fn warning_for_car(
        kind: &'static str,
        vehicle_id: i32,
        distance_meters: f64,
        snapshot: &LmuSnapshot,
    ) -> FlagWarning {
        let (car_position, vehicle_class) = Self::warning_car_details(snapshot, vehicle_id);
        FlagWarning {
            kind,
            active: true,
            distance_meters: if distance_meters.is_finite() {
                distance_meters
            } else {
                0.0
            },
            car_position,
            vehicle_class,
        }
    }

    pub(super) fn warning_car_details(snapshot: &LmuSnapshot, vehicle_id: i32) -> (i32, String) {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let raw = &snapshot.standings[..count];
        let Some(target) = raw.iter().find(|entry| entry.vehicle_id == vehicle_id) else {
            return (0, String::new());
        };
        let class_position = raw
            .iter()
            .filter(|entry| {
                entry.position > 0
                    && entry.position <= target.position
                    && entry.vehicle_class == target.vehicle_class
            })
            .count() as i32;
        (
            class_position,
            Self::string_from_chars(&target.vehicle_class),
        )
    }

    pub(super) fn rejoin_safety(distance: f64, time_to_arrival: f64) -> &'static str {
        if !time_to_arrival.is_finite() {
            return "safe";
        }
        if distance <= 100.0 || time_to_arrival <= 6.0 {
            "danger"
        } else if distance <= 250.0 || time_to_arrival <= 12.0 {
            "caution"
        } else {
            "safe"
        }
    }

    pub(super) fn update_rejoin_warning(&mut self, snapshot: &LmuSnapshot) -> RejoinWarning {
        // Coincide con los valores por defecto del aviso Traffic de TinyPedal:
        // 8 m/s de velocidad baja, 15 s de separación y 10 s de persistencia.
        const LOW_SPEED_KPH: f64 = 8.0 * 3.6;
        const MAX_TRAFFIC_GAP_SECONDS: f64 = 15.0;
        const HOLD_FRAMES: u16 = 200;
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let raw = &snapshot.standings[..count];
        let Some(player) = raw.iter().find(|entry| entry.is_player != 0) else {
            self.rejoin_hold_frames = 0;
            return RejoinWarning::default();
        };

        if player.in_pits != 0 {
            self.rejoin_reason = "pit_exit";
            self.rejoin_hold_frames = HOLD_FRAMES;
        } else if snapshot.player_offroad_wheels >= 4
            || (snapshot.speed_kph.is_finite() && snapshot.speed_kph < LOW_SPEED_KPH)
        {
            // Conserva el motivo de salida de boxes mientras continúa su ventana;
            // fuera de ella, la velocidad baja/off-road inicia un rejoin normal.
            if self.rejoin_hold_frames == 0 || self.rejoin_reason != "pit_exit" {
                self.rejoin_reason = "rejoin";
            }
            self.rejoin_hold_frames = HOLD_FRAMES;
        } else if self.rejoin_hold_frames > 0 {
            self.rejoin_hold_frames -= 1;
        }

        if self.rejoin_hold_frames == 0 {
            return RejoinWarning::default();
        }

        let rear_car = raw
            .iter()
            .filter(|entry| {
                entry.vehicle_id != player.vehicle_id
                    && entry.in_pits == 0
                    && entry.in_garage == 0
                    && entry.finish_status == 0
            })
            .filter_map(|entry| {
                let traffic_gap = Self::relative_gaps_seconds(player, entry).1;
                if !(0.05..MAX_TRAFFIC_GAP_SECONDS).contains(&traffic_gap) {
                    return None;
                }
                let distance = Self::track_distance(
                    entry.lap_distance,
                    player.lap_distance,
                    snapshot.track_length,
                );
                if distance <= 1.0 {
                    return None;
                }
                let closing_speed_ms = ((entry.speed_kph - player.speed_kph) / 3.6).max(0.0);
                let time_to_arrival = if closing_speed_ms > 0.5 {
                    distance / closing_speed_ms
                } else {
                    f64::INFINITY
                };
                Some((entry, distance, traffic_gap, time_to_arrival))
            })
            // Prioriza el coche que llegará antes. Si ninguno se acerca, usa la
            // separación temporal más corta, que es el criterio de TinyPedal.
            .min_by(|left, right| {
                left.3
                    .total_cmp(&right.3)
                    .then_with(|| left.2.total_cmp(&right.2))
            });

        let Some((rear, distance, _, time_to_arrival)) = rear_car else {
            return RejoinWarning::default();
        };

        let safety = Self::rejoin_safety(distance, time_to_arrival);
        let (car_position, vehicle_class) = Self::warning_car_details(snapshot, rear.vehicle_id);

        RejoinWarning {
            active: true,
            reason: self.rejoin_reason,
            safety,
            rear_car_available: true,
            distance_meters: distance,
            time_to_arrival_seconds: if time_to_arrival.is_finite() {
                time_to_arrival
            } else {
                0.0
            },
            car_position,
            vehicle_class,
        }
    }
}
