#define NOMINMAX

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <optional>
#include <utility>

#include "SharedMemoryInterface.hpp"

constexpr size_t MAX_VEHICLES = 104;

struct LmuStandingEntry {
    int32_t vehicle_id;
    int32_t position;
    int32_t total_laps;
    int32_t laps_behind_leader;
    uint32_t is_player;
    uint32_t in_pits;
    uint32_t in_garage;
    uint32_t lap_invalidated;
    uint32_t flag;
    uint32_t pit_state;
    uint32_t pit_stops;
    uint32_t penalties;
    uint32_t finish_status;
    uint32_t vehicle_class_id;
    int32_t sector;
    uint32_t individual_phase;
    uint64_t steam_id;
    double time_behind_leader;
    double interval;
    int32_t laps_behind_next;
    double time_into_lap;
    double estimated_lap_time;
    double best_lap_seconds;
    double last_lap_seconds;
    double best_sector_ends[3];
    double lap_start_elapsed_seconds;
    double elapsed_seconds;
    double virtual_energy;
    double damage_percent;
    uint32_t track_limits_steps;
    uint32_t track_limits_available;
    double speed_kph;
    double path_lateral;
    double track_edge;
    double lap_distance;
    double world_x;
    double world_y;
    char driver_name[32];
    char vehicle_class[32];
    char team_name[24];
    char vehicle_name[64];
    char vehicle_filename[32];
    char vehicle_model[30];
    char tire_compound[18];
    char rear_tire_compound[18];
    uint8_t wheel_compounds[4];
};

struct LmuSnapshot {
    uint32_t connected;
    uint32_t player_active;
    uint32_t game_in_foreground;
    uint32_t game_in_realtime;
    uint32_t player_in_garage;
    uint32_t player_offroad_wheels;
    uint32_t standings_count;
    int32_t lap_number;
    int32_t player_sector;
    int32_t gear;
    int32_t player_total_laps;
    int32_t max_laps;
    int32_t session_type;
    uint32_t game_phase;
    uint32_t yellow_sectors;
    int32_t leader_total_laps;
    double speed_kph;
    double rpm;
    double max_rpm;
    double throttle;
    double brake;
    double clutch;
    double brake_bias_percent;
    uint32_t track_limits_steps;
    uint32_t track_limits_steps_per_penalty;
    uint32_t tc_active;
    uint32_t abs_active;
    uint32_t lift_and_coast_progress;
    double steering;
    double steering_range_degrees;
    double force_feedback;
    double fuel_liters;
    double fuel_capacity_liters;
    double virtual_energy;
    uint32_t vehicle_class_id;
    uint32_t player_lap_valid;
    double current_lap_seconds;
    double player_lap_start_elapsed_seconds;
    double current_sector1_seconds;
    double current_sector2_seconds;
    double player_best_sector_ends[3];
    double best_lap_seconds;
    double lap_delta_seconds;
    double session_time_remaining;
    double session_elapsed_seconds;
    double game_time_of_day_seconds;
    double session_end_seconds;
    double estimated_lap_time;
    double last_lap_seconds;
    double leader_lap_time;
    double leader_time_into_lap;
    double player_time_into_lap;
    double player_lap_distance;
    double player_world_x;
    double player_world_y;
    int32_t player_vehicle_id;
    double track_length;
    double ambient_temperature_c;
    double track_temperature_c;
    double rain_percent;
    double track_wetness_percent;
    double track_wetness_min_percent;
    double track_wetness_max_percent;
    uint8_t track_grip_level;
    uint8_t cloud_coverage;
    double wind_x;
    double wind_y;
    double wind_z;
    double player_orientation_right_z;
    double player_orientation_forward_z;
    double player_tire_remaining_percent;
    double player_damage_percent;
    double player_engine_oil_temperature_c;
    double player_engine_water_temperature_c;
    double player_tire_temperature_c[4];
    double player_tire_temperature_by_zone_c[4][3];
    double player_brake_temperature_c[4];
    double player_tire_remaining_by_wheel_percent[4];
    double player_tire_slip_ratio[4];
    double player_tire_sliding_fraction[4];
    uint8_t player_engine_overheating;
    uint32_t player_body_part_detached;
    uint8_t player_tire_compounds[4];
    uint8_t player_tire_flat[4];
    uint8_t player_tire_detached[4];
    uint8_t player_damage_severity[8];
    double battery_charge_percent;
    double hybrid_regen_kw;
    double hybrid_motor_temperature_c;
    double hybrid_motor_rpm;
    int32_t player_position;
    int32_t player_class_position;
    int32_t player_class_size;
    uint8_t hybrid_motor_state;
    uint8_t engine_map;
    uint8_t engine_map_max;
    uint8_t traction_control_level;
    uint8_t traction_control_max;
    uint8_t traction_control_slip;
    uint8_t traction_control_slip_max;
    uint8_t traction_control_cut;
    uint8_t traction_control_cut_max;
    uint8_t anti_lock_brakes_level;
    uint8_t anti_lock_brakes_max;
    uint8_t brake_migration;
    uint8_t brake_migration_max;
    uint8_t front_anti_roll_bar;
    uint8_t front_anti_roll_bar_max;
    uint8_t rear_anti_roll_bar;
    uint8_t rear_anti_roll_bar_max;
    uint8_t speed_limiter_active;
    uint8_t headlights_on;
    uint8_t wiper_state;
    char vehicle_name[64];
    char vehicle_model[30];
    char track_name[64];
    LmuStandingEntry standings[MAX_VEHICLES];
};

namespace {
HANDLE map_handle = nullptr;
SharedMemoryLayout* shared_memory = nullptr;
std::optional<SharedMemoryLock> shared_memory_lock;
SharedMemoryObjectOut copied_memory{};

// The SDK helper copies counts and stream sizes supplied by the game without
// checking them against the fixed-size arrays in SharedMemoryObjectOut. A
// transient or incompatible shared-memory frame must not be allowed to write
// past copied_memory, because that would corrupt the process long before the
// resulting failure is reported by Windows as STATUS_HEAP_CORRUPTION.
void copy_shared_memory_bounded(SharedMemoryObjectOut& destination,
                                const SharedMemoryObjectOut& source) {
    std::memcpy(&destination.generic, &source.generic, sizeof(SharedMemoryGeneric));

    if (source.generic.events[SME_UPDATE_SCORING]) {
        std::memcpy(&destination.scoring.scoringInfo,
                    &source.scoring.scoringInfo,
                    sizeof(ScoringInfoV01));

        const auto requested_vehicle_count = source.scoring.scoringInfo.mNumVehicles;
        const size_t vehicle_count = requested_vehicle_count <= 0
            ? 0
            : std::min(static_cast<size_t>(requested_vehicle_count), MAX_VEHICLES);
        std::memcpy(destination.scoring.vehScoringInfo,
                    source.scoring.vehScoringInfo,
                    vehicle_count * sizeof(VehicleScoringInfoV01));
        destination.scoring.scoringInfo.mNumVehicles =
            static_cast<decltype(destination.scoring.scoringInfo.mNumVehicles)>(vehicle_count);

        const size_t stream_capacity = sizeof(destination.scoring.scoringStream);
        const size_t stream_size = std::min(
            source.scoring.scoringStreamSize,
            stream_capacity - 1);
        std::memcpy(destination.scoring.scoringStream,
                    source.scoring.scoringStream,
                    stream_size);
        destination.scoring.scoringStreamSize = stream_size;
        destination.scoring.scoringStream[stream_size] = '\0';
        destination.scoring.scoringInfo.mVehicle = &destination.scoring.vehScoringInfo[0];
        destination.scoring.scoringInfo.mResultsStream = &destination.scoring.scoringStream[0];
    }

    if (source.generic.events[SME_UPDATE_TELEMETRY]) {
        const size_t vehicle_count = std::min(
            static_cast<size_t>(source.telemetry.activeVehicles),
            MAX_VEHICLES);
        destination.telemetry.activeVehicles = static_cast<uint8_t>(vehicle_count);
        destination.telemetry.playerHasVehicle = source.telemetry.playerHasVehicle;
        destination.telemetry.playerVehicleIdx = source.telemetry.playerVehicleIdx;
        std::memcpy(destination.telemetry.telemInfo,
                    source.telemetry.telemInfo,
                    vehicle_count * sizeof(TelemInfoV01));
    }

    if (source.generic.events[SME_ENTER]
        || source.generic.events[SME_EXIT]
        || source.generic.events[SME_SET_ENVIRONMENT]) {
        std::memcpy(&destination.paths,
                    &source.paths,
                    sizeof(SharedMemoryPathData));
    }
}

void close_reader() {
    if (shared_memory) {
        UnmapViewOfFile(shared_memory);
        shared_memory = nullptr;
    }
    if (map_handle) {
        CloseHandle(map_handle);
        map_handle = nullptr;
    }
    shared_memory_lock.reset();
    std::memset(&copied_memory, 0, sizeof(copied_memory));
}

bool open_reader() {
    if (shared_memory) {
        return true;
    }

    auto lock = SharedMemoryLock::MakeSharedMemoryLock();
    if (!lock.has_value()) {
        return false;
    }

    map_handle = OpenFileMappingW(FILE_MAP_READ, FALSE, L"LMU_Data");
    if (!map_handle) {
        return false;
    }

    shared_memory = static_cast<SharedMemoryLayout*>(MapViewOfFile(
        map_handle, FILE_MAP_READ, 0, 0, sizeof(SharedMemoryLayout)));
    if (!shared_memory) {
        CloseHandle(map_handle);
        map_handle = nullptr;
        return false;
    }

    shared_memory_lock = std::move(lock);
    return true;
}
}  // namespace

extern "C" size_t lmu_snapshot_size() {
    return sizeof(LmuSnapshot);
}

extern "C" int lmu_read_snapshot(LmuSnapshot* output, int32_t spectator_vehicle_id) {
    if (!output) {
        return -1;
    }
    std::memset(output, 0, sizeof(*output));

    if (!open_reader()) {
        return 0;
    }

    if (!shared_memory_lock->Lock(10)) {
        return 0;
    }
    copy_shared_memory_bounded(copied_memory, shared_memory->data);
    shared_memory_lock->Unlock();

    HWND game_window = copied_memory.generic.appInfo.mAppWindow;
    if (!game_window || !IsWindow(game_window)) {
        close_reader();
        return 0;
    }

    output->connected = 1;
    const HWND foreground_window = GetForegroundWindow();
    output->game_in_foreground =
        foreground_window &&
        GetAncestor(foreground_window, GA_ROOT) == GetAncestor(game_window, GA_ROOT)
        ? 1u
        : 0u;
    const auto& scoring = copied_memory.scoring;
    output->game_in_realtime = scoring.scoringInfo.mInRealtime ? 1u : 0u;
    const auto& telemetry = copied_memory.telemetry;
    const int vehicle_count = std::clamp(scoring.scoringInfo.mNumVehicles, 0L, 104L);
    output->standings_count = static_cast<uint32_t>(vehicle_count);
    output->max_laps = static_cast<int32_t>(scoring.scoringInfo.mMaxLaps);
    output->session_type = static_cast<int32_t>(scoring.scoringInfo.mSession);
    output->game_phase = static_cast<uint32_t>(scoring.scoringInfo.mGamePhase);
    // mSector usa 0=sector 3, 1=sector 1 y 2=sector 2. El bitmask se
    // normaliza al mismo valor para que Rust pueda comparar ambos sin
    // depender del orden interno de mSectorFlag.
    for (int flag_index = 0; flag_index < 3; ++flag_index) {
        if (scoring.scoringInfo.mSectorFlag[flag_index] == 1) {
            const int scoring_sector = (flag_index + 1) % 3;
            output->yellow_sectors |= 1u << scoring_sector;
        }
    }
    output->session_time_remaining = scoring.scoringInfo.mSessionTimeRemaining;
    output->session_elapsed_seconds = scoring.scoringInfo.mCurrentET;
    output->game_time_of_day_seconds = scoring.scoringInfo.mTimeOfDay;
    output->session_end_seconds = scoring.scoringInfo.mEndET;
    output->track_limits_steps_per_penalty = static_cast<uint32_t>(scoring.scoringInfo.mTrackLimitsStepsPerPenalty);
    output->track_length = scoring.scoringInfo.mLapDist;
    output->ambient_temperature_c = scoring.scoringInfo.mAmbientTemp;
    output->track_temperature_c = scoring.scoringInfo.mTrackTemp;
    output->rain_percent = scoring.scoringInfo.mRaining * 100.0;
    output->track_wetness_percent = scoring.scoringInfo.mAvgPathWetness * 100.0;
    output->track_wetness_min_percent = scoring.scoringInfo.mMinPathWetness * 100.0;
    output->track_wetness_max_percent = scoring.scoringInfo.mMaxPathWetness * 100.0;
    output->track_grip_level = scoring.scoringInfo.mTrackGripLevel;
    output->cloud_coverage = scoring.scoringInfo.mCloudCoverage;
    output->wind_x = scoring.scoringInfo.mWind.x;
    output->wind_y = scoring.scoringInfo.mWind.y;
    output->wind_z = scoring.scoringInfo.mWind.z;
    output->player_tire_remaining_percent = -1.0;
    output->player_engine_oil_temperature_c = -1.0;
    output->player_engine_water_temperature_c = -1.0;
    std::fill_n(output->player_tire_temperature_c, 4, -1.0);
    std::fill_n(&output->player_tire_temperature_by_zone_c[0][0], 12, -1.0);
    std::fill_n(output->player_brake_temperature_c, 4, -1.0);
    std::fill_n(output->player_tire_remaining_by_wheel_percent, 4, -1.0);
    std::fill_n(output->player_tire_slip_ratio, 4, 0.0);
    std::fill_n(output->player_tire_sliding_fraction, 4, 0.0);

    const unsigned long active_vehicles =
        std::min<unsigned long>(telemetry.activeVehicles, MAX_VEHICLES);
    const TelemInfoV01* selected_vehicle = nullptr;
    if (spectator_vehicle_id >= 0) {
        for (unsigned long index = 0; index < active_vehicles; ++index) {
            if (telemetry.telemInfo[index].mID == spectator_vehicle_id) {
                selected_vehicle = &telemetry.telemInfo[index];
                break;
            }
        }
    } else if (telemetry.playerHasVehicle && telemetry.playerVehicleIdx < active_vehicles) {
        selected_vehicle = &telemetry.telemInfo[telemetry.playerVehicleIdx];
    } else if (spectator_vehicle_id == -1) {
        const VehicleScoringInfoV01* player_entry = nullptr;
        for (int index = 0; index < vehicle_count; ++index) {
            if (scoring.vehScoringInfo[index].mIsPlayer) {
                player_entry = &scoring.vehScoringInfo[index];
                break;
            }
        }
        if (player_entry) {
            for (unsigned long index = 0; index < active_vehicles; ++index) {
                if (telemetry.telemInfo[index].mID == player_entry->mID) {
                    selected_vehicle = &telemetry.telemInfo[index];
                    break;
                }
            }
        }
    }
    const long selected_vehicle_id = selected_vehicle ? selected_vehicle->mID : -1;

    for (int index = 0; index < vehicle_count; ++index) {
        const VehicleScoringInfoV01& source = scoring.vehScoringInfo[index];
        LmuStandingEntry& destination = output->standings[index];
        destination.vehicle_id = static_cast<int32_t>(source.mID);
        destination.position = static_cast<int32_t>(source.mPlace);
        destination.total_laps = static_cast<int32_t>(source.mTotalLaps);
        destination.laps_behind_leader = static_cast<int32_t>(source.mLapsBehindLeader);
        // Once a telemetry car is selected it is the only reference, including
        // in observer modes. The local car may remain marked mIsPlayer in its
        // garage; admitting both would overwrite the followed car's state.
        destination.is_player = (selected_vehicle_id >= 0
            ? source.mID == selected_vehicle_id
            : source.mIsPlayer) ? 1u : 0u;
        destination.in_pits = source.mInPits ? 1u : 0u;
        destination.in_garage = source.mInGarageStall ? 1u : 0u;
        destination.flag = static_cast<uint32_t>(source.mFlag);
        destination.pit_state = static_cast<uint32_t>(source.mPitState);
        destination.pit_stops = static_cast<uint32_t>(std::max<short>(source.mNumPitstops, 0));
        destination.penalties = static_cast<uint32_t>(std::max<short>(source.mNumPenalties, 0));
        destination.finish_status = static_cast<uint32_t>(std::max<signed char>(source.mFinishStatus, 0));
        destination.sector = static_cast<int32_t>(source.mSector);
        destination.individual_phase = static_cast<uint32_t>(source.mIndividualPhase);
        destination.steam_id = static_cast<uint64_t>(source.mSteamID);
        destination.time_behind_leader = source.mTimeBehindLeader;
        destination.interval = source.mTimeBehindNext;
        destination.laps_behind_next = static_cast<int32_t>(source.mLapsBehindNext);
        destination.time_into_lap = source.mTimeIntoLap;
        destination.estimated_lap_time = source.mEstimatedLapTime;
        destination.best_lap_seconds = source.mBestLapTime;
        destination.last_lap_seconds = source.mLastLapTime;

        const double best_sector_ends[3] = {
            std::abs(source.mBestSector1),
            std::abs(source.mBestSector2),
            source.mBestLapTime,
        };
        std::copy_n(best_sector_ends, 3, destination.best_sector_ends);
        destination.speed_kph = std::sqrt(
            source.mLocalVel.x * source.mLocalVel.x +
            source.mLocalVel.y * source.mLocalVel.y +
            source.mLocalVel.z * source.mLocalVel.z) * 3.6;
        destination.path_lateral = source.mPathLateral;
        destination.track_edge = source.mTrackEdge;
        destination.lap_distance = source.mLapDist;
        destination.world_x = source.mPos.x;
        destination.world_y = -source.mPos.z;
        std::memcpy(destination.driver_name, source.mDriverName, sizeof(destination.driver_name));
        std::memcpy(destination.vehicle_class, source.mVehicleClass, sizeof(destination.vehicle_class));
        std::memcpy(destination.team_name, source.mPitGroup, sizeof(destination.team_name));
        std::memcpy(destination.vehicle_name, source.mVehicleName, sizeof(destination.vehicle_name));
        std::memcpy(destination.vehicle_filename, source.mVehFilename, sizeof(destination.vehicle_filename));

        const TelemInfoV01* vehicle_telemetry = nullptr;
        for (unsigned long telemetry_index = 0; telemetry_index < active_vehicles; ++telemetry_index) {
            if (telemetry.telemInfo[telemetry_index].mID == source.mID) {
                vehicle_telemetry = &telemetry.telemInfo[telemetry_index];
                break;
            }
        }
        if (vehicle_telemetry) {
            // Scoring actualiza las posiciones rivales a menor frecuencia. La
            // telemetría por vehículo es la fuente espacial más reciente y
            // evita que los marcadores del mapa avancen en pasos visibles.
            destination.world_x = vehicle_telemetry->mPos.x;
            destination.world_y = -vehicle_telemetry->mPos.z;
            destination.lap_start_elapsed_seconds = vehicle_telemetry->mLapStartET;
            destination.elapsed_seconds = vehicle_telemetry->mElapsedTime;
            destination.lap_invalidated = vehicle_telemetry->mLapInvalidated ? 1u : 0u;
            destination.track_limits_steps = static_cast<uint32_t>(vehicle_telemetry->mTrackLimitsSteps);
            destination.track_limits_available = 1u;
            destination.virtual_energy = static_cast<double>(vehicle_telemetry->mVirtualEnergy);
            destination.vehicle_class_id = static_cast<uint32_t>(vehicle_telemetry->mVehicleClass);
            std::memcpy(destination.vehicle_model, vehicle_telemetry->mVehicleModel, sizeof(destination.vehicle_model));
            std::memcpy(destination.tire_compound, vehicle_telemetry->mFrontTireCompoundName, sizeof(destination.tire_compound));
            std::memcpy(destination.rear_tire_compound, vehicle_telemetry->mRearTireCompoundName, sizeof(destination.rear_tire_compound));
            for (size_t i = 0; i < 4; ++i) {
                destination.wheel_compounds[i] = vehicle_telemetry->mWheel[i].mCompoundType;
            }
            if (destination.is_player) {
                output->player_engine_oil_temperature_c = vehicle_telemetry->mEngineOilTemp;
                output->player_engine_water_temperature_c = vehicle_telemetry->mEngineWaterTemp;
                double remaining = 100.0;
                for (size_t wheel_index = 0; wheel_index < 4; ++wheel_index) {
                    const TelemWheelV01& wheel = vehicle_telemetry->mWheel[wheel_index];
                    // LMU entrega mWear como fracción de banda restante: 1.0
                    // con neumático nuevo y 0.0 al agotarse.
                    const double tire_remaining = std::clamp(wheel.mWear * 100.0, 0.0, 100.0);
                    remaining = std::min(remaining, tire_remaining);
                    output->player_tire_temperature_c[wheel_index] =
                        wheel.mTireCarcassTemperature * 0.34
                        + wheel.mTireInnerLayerTemperature[0] * 0.22
                        + wheel.mTireInnerLayerTemperature[1] * 0.22
                        + wheel.mTireInnerLayerTemperature[2] * 0.22
                        - 273.15;
                    // LMU's HUD colors the three contact-patch bands from the
                    // surface samples reported as physical left/center/right.
                    for (size_t zone_index = 0; zone_index < 3; ++zone_index) {
                        output->player_tire_temperature_by_zone_c[wheel_index][zone_index] =
                            wheel.mTemperature[zone_index] - 273.15;
                    }
                    // LMU entrega mBrakeTemp en Kelvin pese al comentario heredado del SDK.
                    output->player_brake_temperature_c[wheel_index] = wheel.mBrakeTemp - 273.15;
                    output->player_tire_remaining_by_wheel_percent[wheel_index] = tire_remaining;
                    // mStaticUndeflectedRadius is a radius in centimetres. Longitudinal
                    // slip compares the tyre's peripheral speed with the ground
                    // velocity along the wheel, so lateral ground velocity must not
                    // be included in the denominator.
                    const double radius_m = static_cast<double>(wheel.mStaticUndeflectedRadius) / 100.0;
                    const double longitudinal_speed_mps = std::abs(wheel.mLongitudinalGroundVel);
                    output->player_tire_slip_ratio[wheel_index] =
                        longitudinal_speed_mps > 1.0 && radius_m > 0.0
                        ? std::abs(wheel.mRotation) * radius_m / longitudinal_speed_mps - 1.0
                        : 0.0;
                    output->player_tire_sliding_fraction[wheel_index] =
                        std::clamp(wheel.mGripFract, 0.0, 1.0);
                    output->player_tire_compounds[wheel_index] = wheel.mCompoundType;
                    output->player_tire_flat[wheel_index] = wheel.mFlat ? 1u : 0u;
                    output->player_tire_detached[wheel_index] = wheel.mDetached ? 1u : 0u;
                }
                output->player_tire_remaining_percent = std::clamp(remaining, 0.0, 100.0);
                std::memcpy(output->player_damage_severity, vehicle_telemetry->mDentSeverity,
                            sizeof(output->player_damage_severity));
                output->player_engine_overheating = vehicle_telemetry->mOverheating ? 1u : 0u;
                // mDetached solo cubre piezas de carrocería, nunca ruedas. En LMU
                // esa pieza es el alerón trasero, y es la señal que el propio
                // juego enciende en rojo parpadeante.
                output->player_body_part_detached = vehicle_telemetry->mDetached ? 1u : 0u;
            }

            unsigned int dent_total = 0;
            for (unsigned char severity : vehicle_telemetry->mDentSeverity) {
                dent_total += std::min<unsigned int>(severity, 2u);
            }
            bool wheel_detached = false;
            for (const TelemWheelV01& wheel : vehicle_telemetry->mWheel) {
                wheel_detached = wheel_detached || wheel.mDetached;
            }
            // Standings expresa como daño el inverso de la integridad usada
            // por TinyPedal: abolladuras + 50% por pieza de carrocería
            // desprendida (en LMU, normalmente el alerón trasero) + 100% por
            // una rueda desprendida. El resultado final se limita a 0-100%.
            const double body_damage = static_cast<double>(dent_total) / 16.0;
            const double detached_body_damage = vehicle_telemetry->mDetached ? 0.5 : 0.0;
            const double detached_wheel_damage = wheel_detached ? 1.0 : 0.0;
            destination.damage_percent = std::clamp(
                (body_damage + detached_body_damage + detached_wheel_damage) * 100.0,
                0.0, 100.0);
            if (destination.is_player) {
                output->player_damage_percent = destination.damage_percent;
            }
        }
        if (source.mPlace == 1) {
            output->leader_total_laps = static_cast<int32_t>(source.mTotalLaps);
            output->leader_lap_time = source.mEstimatedLapTime > 0.0
                ? source.mEstimatedLapTime
                : (source.mLastLapTime > 0.0 ? source.mLastLapTime : source.mBestLapTime);
            output->leader_time_into_lap = source.mTimeIntoLap;
        }
        if (destination.is_player) {
            output->player_position = static_cast<int32_t>(source.mPlace);
            output->player_in_garage = source.mInGarageStall ? 1u : 0u;
            output->player_sector = static_cast<int32_t>(source.mSector);
            output->player_total_laps = static_cast<int32_t>(source.mTotalLaps);
            output->estimated_lap_time = source.mEstimatedLapTime;
            output->best_lap_seconds = source.mBestLapTime;
            output->last_lap_seconds = source.mLastLapTime;
            output->current_sector1_seconds = std::abs(source.mCurSector1);
            output->current_sector2_seconds = std::abs(source.mCurSector2);
            std::copy_n(best_sector_ends, 3, output->player_best_sector_ends);
            output->player_time_into_lap = source.mTimeIntoLap;
            output->player_lap_distance = source.mLapDist;
        }
    }

    // Class position is what a multiclass grid is actually raced on, and the
    // scoring class name is filled for every car, including the ones without
    // per-vehicle telemetry, so it is the grouping that never leaves a car out.
    for (int index = 0; index < vehicle_count; ++index) {
        const LmuStandingEntry& player = output->standings[index];
        if (!player.is_player) {
            continue;
        }
        int class_position = 1;
        int class_size = 0;
        for (int other = 0; other < vehicle_count; ++other) {
            const LmuStandingEntry& rival = output->standings[other];
            if (std::strncmp(rival.vehicle_class, player.vehicle_class,
                             sizeof(rival.vehicle_class)) != 0) {
                continue;
            }
            ++class_size;
            if (rival.position > 0 && player.position > 0
                && rival.position < player.position) {
                ++class_position;
            }
        }
        output->player_class_position = class_position;
        output->player_class_size = class_size;
        break;
    }

    if (!selected_vehicle) {
        return 1;
    }

    const TelemInfoV01& vehicle = *selected_vehicle;
    output->player_active = 1;
    output->player_world_x = vehicle.mPos.x;
    output->player_world_y = -vehicle.mPos.z;
    output->player_vehicle_id = static_cast<int32_t>(selected_vehicle_id);
    output->player_lap_valid = vehicle.mLapInvalidated ? 0u : 1u;
    output->lap_number = static_cast<int32_t>(vehicle.mLapNumber);
    output->gear = static_cast<int32_t>(vehicle.mGear);
    output->speed_kph = std::sqrt(
        vehicle.mLocalVel.x * vehicle.mLocalVel.x +
        vehicle.mLocalVel.y * vehicle.mLocalVel.y +
        vehicle.mLocalVel.z * vehicle.mLocalVel.z) * 3.6;
    output->rpm = vehicle.mEngineRPM;
    output->max_rpm = vehicle.mEngineMaxRPM;
    output->throttle = vehicle.mUnfilteredThrottle;
    output->brake = vehicle.mUnfilteredBrake;
    output->clutch = vehicle.mUnfilteredClutch;
    output->brake_bias_percent = (1.0 - vehicle.mRearBrakeBias) * 100.0;
    output->track_limits_steps = static_cast<uint32_t>(vehicle.mTrackLimitsSteps);
    output->tc_active = vehicle.mTCActive ? 1u : 0u;
    output->abs_active = vehicle.mABSActive ? 1u : 0u;
    output->lift_and_coast_progress = static_cast<uint32_t>(vehicle.mLiftAndCoastProgress);
    // Driver-selectable electronics and the hybrid system. The car's own MFD is
    // the only other place they can be read, and reading it means taking the
    // eyes off the road, so every value is copied from the player telemetry.
    output->engine_map = vehicle.mMotorMap;
    output->engine_map_max = vehicle.mMotorMapMax;
    output->traction_control_level = vehicle.mTC;
    output->traction_control_max = vehicle.mTCMax;
    output->traction_control_slip = vehicle.mTCSlip;
    output->traction_control_slip_max = vehicle.mTCSlipMax;
    output->traction_control_cut = vehicle.mTCCut;
    output->traction_control_cut_max = vehicle.mTCCutMax;
    output->anti_lock_brakes_level = vehicle.mABS;
    output->anti_lock_brakes_max = vehicle.mABSMax;
    output->brake_migration = vehicle.mMigration;
    output->brake_migration_max = vehicle.mMigrationMax;
    output->front_anti_roll_bar = vehicle.mFrontAntiSway;
    output->front_anti_roll_bar_max = vehicle.mFrontAntiSwayMax;
    output->rear_anti_roll_bar = vehicle.mRearAntiSway;
    output->rear_anti_roll_bar_max = vehicle.mRearAntiSwayMax;
    // The limiter is published twice: mSpeedLimiter comes from the rFactor
    // telemetry block and mSpeedLimiterActive from the one LMU appends. A car
    // is not guaranteed to fill both, so either one engaged counts.
    output->speed_limiter_active =
        vehicle.mSpeedLimiter != 0 || vehicle.mSpeedLimiterActive ? 1u : 0u;
    output->headlights_on = vehicle.mHeadlights ? 1u : 0u;
    output->wiper_state = vehicle.mWiperState;
    // The charge is published twice and the two fields do not share a scale:
    // the fraction is documented as [0..1] while the state of charge has been
    // seen as a percentage. Read whichever one the car fills and normalise it.
    const double state_of_charge = static_cast<double>(vehicle.mSoC);
    const double charge_percent = state_of_charge > 0.0
        ? (state_of_charge > 1.5 ? state_of_charge : state_of_charge * 100.0)
        : vehicle.mBatteryChargeFraction * 100.0;
    output->battery_charge_percent = std::clamp(charge_percent, 0.0, 100.0);
    output->hybrid_regen_kw = static_cast<double>(vehicle.mRegen);
    output->hybrid_motor_temperature_c = vehicle.mElectricBoostMotorTemperature;
    output->hybrid_motor_rpm = vehicle.mElectricBoostMotorRPM;
    output->hybrid_motor_state = vehicle.mElectricBoostMotorState;
    // The cockpit wheel follows the physical controller input. Filtered steering
    // can include the vehicle steering ratio and over-rotate the overlay.
    output->steering = vehicle.mUnfilteredSteering;
    output->steering_range_degrees = vehicle.mPhysicalSteeringWheelRange > 0.0f
        ? static_cast<double>(vehicle.mPhysicalSteeringWheelRange)
        : static_cast<double>(vehicle.mVisualSteeringWheelRange);
    output->force_feedback = static_cast<double>(copied_memory.generic.FFBTorque);
    output->fuel_liters = vehicle.mFuel;
    output->fuel_capacity_liters = vehicle.mFuelCapacity;
    output->virtual_energy = static_cast<double>(vehicle.mVirtualEnergy);
    output->vehicle_class_id = static_cast<uint32_t>(vehicle.mVehicleClass);
    output->player_orientation_right_z = vehicle.mOri[0].z;
    output->player_orientation_forward_z = vehicle.mOri[2].z;
    for (const TelemWheelV01& wheel : vehicle.mWheel) {
        if (wheel.mSurfaceType >= 2 && wheel.mSurfaceType <= 4) {
            ++output->player_offroad_wheels;
        }
    }
    output->current_lap_seconds = std::max(0.0, vehicle.mElapsedTime - vehicle.mLapStartET);
    output->player_lap_start_elapsed_seconds = vehicle.mLapStartET;
    output->lap_delta_seconds = vehicle.mDeltaBest;
    std::memcpy(output->vehicle_name, vehicle.mVehicleName, sizeof(output->vehicle_name));
    std::memcpy(output->vehicle_model, vehicle.mVehicleModel, sizeof(output->vehicle_model));
    std::memcpy(output->track_name, vehicle.mTrackName, sizeof(output->track_name));

    return 1;
}
