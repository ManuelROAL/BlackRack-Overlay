//! Flag and rejoin inference for iRacing's shared-memory fields.

use crate::telemetry::{FlagWarning, RejoinWarning};

use super::{AuxiliaryVehicle, IracingTelemetrySource, SURFACE_OFF_TRACK};

const FLAG_CHECKERED: u32 = 0x0000_0001;
const FLAG_YELLOW: u32 = 0x0000_0008;
const FLAG_BLUE: u32 = 0x0000_0020;
const FLAG_CAUTION: u32 = 0x0000_4000 | 0x0000_8000;
const LOW_SPEED_KPH: f64 = 8.0 * 3.6;
const MAX_YELLOW_DISTANCE_METERS: f64 = 500.0;
const MAX_BLUE_DISTANCE_METERS: f64 = 2_000.0;
const MAX_TRAFFIC_GAP_SECONDS: f64 = 15.0;
const REJOIN_HOLD: std::time::Duration = std::time::Duration::from_secs(10);

pub(super) fn yellow_flag_active(session_flags: u32) -> bool {
    session_flags & (FLAG_YELLOW | FLAG_CAUTION) != 0
}

pub(super) fn flag_warning(
    vehicles: &[AuxiliaryVehicle],
    session_flags: u32,
    _session_state: i32,
    game_phase: u32,
    track_length: f64,
) -> FlagWarning {
    let Some(player) = player_vehicle(vehicles) else {
        return FlagWarning::default();
    };

    if session_flags & FLAG_CHECKERED != 0 || game_phase == 8 {
        return FlagWarning {
            kind: "checkered",
            active: true,
            distance_meters: 0.0,
            car_position: player.map.overall_position,
            vehicle_class: String::new(),
        };
    }

    if yellow_flag_active(session_flags) {
        if let Some((target, distance)) = inferred_yellow(vehicles, track_length) {
            return warning_for_vehicle("yellow", target, distance, vehicles);
        }
        // The official flag is still useful when no live speed/surface value
        // identifies a culprit. Do not invent car details in that case.
        return FlagWarning {
            kind: "yellow",
            active: true,
            ..FlagWarning::default()
        };
    }

    if session_flags & FLAG_BLUE != 0 {
        if let Some((target, distance)) = inferred_blue(vehicles, track_length) {
            return warning_for_vehicle("blue", target, distance, vehicles);
        }
        return FlagWarning {
            kind: "blue",
            active: true,
            ..FlagWarning::default()
        };
    }

    FlagWarning::default()
}

pub(super) fn inferred_yellow_culprit_id(
    vehicles: &[AuxiliaryVehicle],
    track_length: f64,
) -> Option<i32> {
    inferred_yellow(vehicles, track_length).map(|(vehicle, _)| vehicle.map.vehicle_id)
}

fn inferred_yellow(
    vehicles: &[AuxiliaryVehicle],
    track_length: f64,
) -> Option<(&AuxiliaryVehicle, f64)> {
    let player = player_vehicle(vehicles)?;
    vehicles
        .iter()
        .filter(|vehicle| {
            vehicle.map.vehicle_id != player.map.vehicle_id
                && !vehicle.map.in_garage
                && !vehicle.map.in_pits
                && (vehicle.track_surface == SURFACE_OFF_TRACK
                    || (vehicle.speed_available && vehicle.speed_kph < LOW_SPEED_KPH))
        })
        .filter_map(|vehicle| {
            let distance = signed_track_distance(
                player.map.lap_distance,
                vehicle.map.lap_distance,
                track_length,
            );
            (distance.is_finite() && distance.abs() <= MAX_YELLOW_DISTANCE_METERS)
                .then_some((vehicle, distance))
        })
        .min_by(|left, right| {
            left.1
                .abs()
                .total_cmp(&right.1.abs())
                .then_with(|| left.1.total_cmp(&right.1))
        })
}

fn inferred_blue(
    vehicles: &[AuxiliaryVehicle],
    track_length: f64,
) -> Option<(&AuxiliaryVehicle, f64)> {
    let player = player_vehicle(vehicles)?;
    let candidates = vehicles
        .iter()
        .filter(|vehicle| {
            vehicle.map.vehicle_id != player.map.vehicle_id
                && !vehicle.map.in_pits
                && !vehicle.map.in_garage
        })
        .filter_map(|vehicle| {
            let signed = signed_track_distance(
                player.map.lap_distance,
                vehicle.map.lap_distance,
                track_length,
            );
            let distance = -signed;
            if !(1.0..=MAX_BLUE_DISTANCE_METERS).contains(&distance) {
                return None;
            }
            let faster = vehicle.map.total_laps > player.map.total_laps
                || (vehicle.pace_available
                    && player.pace_available
                    && vehicle.pace_seconds < player.pace_seconds * 0.98);
            Some((vehicle, distance, faster))
        })
        .collect::<Vec<_>>();
    candidates
        .iter()
        .filter(|(_, _, faster)| *faster)
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .or_else(|| {
            candidates
                .iter()
                .min_by(|left, right| left.1.total_cmp(&right.1))
        })
        .map(|(vehicle, distance, _)| (*vehicle, *distance))
}

fn warning_for_vehicle(
    kind: &'static str,
    vehicle: &AuxiliaryVehicle,
    distance: f64,
    vehicles: &[AuxiliaryVehicle],
) -> FlagWarning {
    let (car_position, vehicle_class) = class_details(vehicle, vehicles);
    FlagWarning {
        kind,
        active: true,
        distance_meters: if distance.is_finite() { distance } else { 0.0 },
        car_position,
        vehicle_class,
    }
}

fn class_details(vehicle: &AuxiliaryVehicle, vehicles: &[AuxiliaryVehicle]) -> (i32, String) {
    let position = vehicle.map.overall_position;
    let class_position = (position > 0)
        .then(|| {
            vehicles
                .iter()
                .filter(|candidate| {
                    candidate.map.vehicle_class == vehicle.map.vehicle_class
                        && candidate.map.overall_position > 0
                        && candidate.map.overall_position <= position
                })
                .count() as i32
        })
        .unwrap_or(0);
    (class_position, vehicle.map.vehicle_class.clone())
}

fn player_vehicle(vehicles: &[AuxiliaryVehicle]) -> Option<&AuxiliaryVehicle> {
    vehicles.iter().find(|vehicle| vehicle.map.is_player)
}

fn track_distance(from: f64, to: f64, track_length: f64) -> f64 {
    if !track_length.is_finite() || track_length <= 0.0 {
        return f64::NAN;
    }
    (to - from).rem_euclid(track_length)
}

fn signed_track_distance(from: f64, to: f64, track_length: f64) -> f64 {
    let forward = track_distance(from, to, track_length);
    if forward <= track_length * 0.5 {
        forward
    } else {
        forward - track_length
    }
}

impl IracingTelemetrySource {
    pub(super) fn update_rejoin_warning(
        &mut self,
        vehicles: &[AuxiliaryVehicle],
        speed_kph: f64,
        player_surface: i32,
        on_pit_road: bool,
        track_length: f64,
    ) -> RejoinWarning {
        let Some(player) = player_vehicle(vehicles) else {
            self.rejoin_hold_until = None;
            return RejoinWarning::default();
        };
        if player.map.in_garage {
            self.rejoin_hold_until = None;
            return RejoinWarning::default();
        }

        let now = std::time::Instant::now();
        let recovering = on_pit_road
            || player_surface == SURFACE_OFF_TRACK
            || (speed_kph.is_finite() && speed_kph < LOW_SPEED_KPH);
        if recovering {
            if on_pit_road {
                self.rejoin_reason = "pit_exit";
            } else if self.rejoin_hold_until.is_none() || self.rejoin_reason != "pit_exit" {
                self.rejoin_reason = "rejoin";
            }
            self.rejoin_hold_until = Some(now + REJOIN_HOLD);
        } else if self.rejoin_hold_until.is_some_and(|until| now >= until) {
            self.rejoin_hold_until = None;
        }

        if self.rejoin_hold_until.is_none() {
            return RejoinWarning::default();
        }

        let player_pace = player
            .pace_available
            .then_some(player.pace_seconds)
            .filter(|pace| pace.is_finite() && *pace > 1.0)
            .unwrap_or(100.0);
        let mut candidates = vehicles
            .iter()
            .filter(|vehicle| {
                vehicle.map.vehicle_id != player.map.vehicle_id
                    && !vehicle.map.in_pits
                    && !vehicle.map.in_garage
            })
            .filter_map(|vehicle| {
                let signed = signed_track_distance(
                    player.map.lap_distance,
                    vehicle.map.lap_distance,
                    track_length,
                );
                let distance = -signed;
                if !(1.0..=track_length * 0.5).contains(&distance) {
                    return None;
                }
                let traffic_gap = (distance / track_length * player_pace).max(0.0);
                if !(0.05..MAX_TRAFFIC_GAP_SECONDS).contains(&traffic_gap) {
                    return None;
                }
                let closing_speed_ms = vehicle
                    .speed_available
                    .then_some((vehicle.speed_kph - speed_kph) / 3.6)
                    .filter(|closing| closing.is_finite() && *closing > 0.5);
                let time_to_arrival =
                    closing_speed_ms.map_or(f64::INFINITY, |closing| distance / closing);
                Some((vehicle, distance, traffic_gap, time_to_arrival))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            left.3
                .total_cmp(&right.3)
                .then_with(|| left.2.total_cmp(&right.2))
        });
        let Some((rear, distance, _, time_to_arrival)) = candidates.into_iter().next() else {
            return RejoinWarning {
                active: true,
                reason: self.rejoin_reason,
                safety: "safe",
                rear_car_available: false,
                ..RejoinWarning::default()
            };
        };

        let safety = rejoin_safety(distance, time_to_arrival);
        let (car_position, vehicle_class) = class_details(rear, vehicles);
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

fn rejoin_safety(distance: f64, time_to_arrival: f64) -> &'static str {
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

#[cfg(test)]
mod tests {
    use crate::telemetry::TrackMapVehicle;

    use super::{
        flag_warning, rejoin_safety, signed_track_distance, track_distance, AuxiliaryVehicle,
        IracingTelemetrySource, FLAG_BLUE, FLAG_CHECKERED, FLAG_YELLOW,
    };

    fn vehicle(
        id: i32,
        position: i32,
        class: &str,
        distance: f64,
        player: bool,
        speed_kph: f64,
        speed_available: bool,
    ) -> AuxiliaryVehicle {
        AuxiliaryVehicle {
            map: TrackMapVehicle {
                vehicle_id: id,
                overall_position: position,
                vehicle_class: class.into(),
                world_x: 0.0,
                world_y: 0.0,
                world_position_available: false,
                lap_distance: distance,
                total_laps: 5,
                in_pits: false,
                in_garage: false,
                causing_yellow: false,
                sector: 1,
                is_player: player,
            },
            speed_kph,
            speed_available,
            pace_seconds: 100.0,
            pace_available: true,
            progress: 5.0 + distance / 10_000.0,
            track_surface: 1,
        }
    }

    #[test]
    fn wraps_track_distances_and_keeps_direction() {
        assert_eq!(track_distance(9_900.0, 100.0, 10_000.0), 200.0);
        assert_eq!(signed_track_distance(100.0, 9_900.0, 10_000.0), -200.0);
        assert_eq!(signed_track_distance(100.0, 500.0, 10_000.0), 400.0);
    }

    #[test]
    fn rejoin_risk_uses_distance_and_arrival_time() {
        assert_eq!(rejoin_safety(80.0, 20.0), "danger");
        assert_eq!(rejoin_safety(500.0, 5.0), "danger");
        assert_eq!(rejoin_safety(180.0, 20.0), "caution");
        assert_eq!(rejoin_safety(500.0, 9.0), "caution");
        assert_eq!(rejoin_safety(500.0, 18.0), "safe");
        assert_eq!(rejoin_safety(50.0, f64::INFINITY), "safe");
    }

    #[test]
    fn flag_priority_is_checkered_then_yellow_then_blue() {
        let vehicles = vec![vehicle(1, 1, "GT3", 1_000.0, true, 100.0, true)];
        assert_eq!(
            flag_warning(
                &vehicles,
                FLAG_CHECKERED | FLAG_YELLOW | FLAG_BLUE,
                4,
                5,
                10_000.0
            )
            .kind,
            "checkered"
        );
    }

    #[test]
    fn yellow_can_identify_a_nearby_slow_car_and_class_position() {
        let vehicles = vec![
            vehicle(1, 1, "GT3", 1_000.0, true, 100.0, true),
            vehicle(2, 3, "GT3", 1_200.0, false, 10.0, true),
        ];
        let warning = flag_warning(&vehicles, FLAG_YELLOW, 4, 5, 10_000.0);
        assert_eq!(warning.kind, "yellow");
        assert_eq!(warning.distance_meters, 200.0);
        assert_eq!(warning.car_position, 2);
        assert_eq!(warning.vehicle_class, "GT3");
    }

    #[test]
    fn missing_rival_speed_never_invents_a_rejoin_closing_time() {
        let vehicles = vec![
            vehicle(1, 1, "GT3", 1_000.0, true, 20.0, true),
            vehicle(2, 2, "GT3", 900.0, false, 0.0, false),
        ];
        let mut source = IracingTelemetrySource::new();
        let warning = source.update_rejoin_warning(&vehicles, 20.0, 0, true, 10_000.0);
        assert!(warning.active);
        assert!(warning.rear_car_available);
        assert_eq!(warning.safety, "safe");
        assert_eq!(warning.time_to_arrival_seconds, 0.0);
    }
}
