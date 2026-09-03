//! Typed view over the simulator's session string.
//!
//! The string only changes when the session does, so it is parsed once per
//! generation and everything derived from it is cached until the next one.
//!
//! It carries far more than this reads — the full roster, results and setup —
//! and each field is added here as the overlay that needs it lands. Today that
//! is the track, the car and the session schedule.

use super::yaml::{self, Node};

/// Session numbering the frame uses: practice below five, qualifying from
/// five, warm-up at nine and race from ten. The simulator names its sessions
/// instead of numbering them, so the name is mapped here once.
pub(super) fn session_type_code(name: &str) -> i32 {
    let name = name.to_ascii_uppercase();
    if name.contains("RACE") {
        10
    } else if name.contains("QUAL") {
        5
    } else if name.contains("WARMUP") || name.contains("WARM UP") {
        9
    } else {
        0
    }
}

#[derive(Clone, Default)]
pub(super) struct Schedule {
    pub(super) number: i32,
    pub(super) name: String,
    pub(super) laps: i32,
    pub(super) seconds: f64,
}

#[derive(Default)]
pub(super) struct Session {
    generation: i32,
    pub(super) track_name: String,
    pub(super) track_length_meters: f64,
    /// Lap fraction where each sector starts, first entry always zero.
    pub(super) sector_starts: Vec<f64>,
    pub(super) player_car_name: String,
    pub(super) fuel_capacity_liters: f64,
    pub(super) redline_rpm: f64,
    pub(super) schedule: Vec<Schedule>,
}

impl Session {
    pub(super) fn new() -> Self {
        Self {
            generation: i32::MIN,
            ..Self::default()
        }
    }

    /// Reparses only when the simulator publishes a new generation. Returns
    /// true when it did, so the caller can rate-limit the next attempt.
    pub(super) fn refresh(&mut self, generation: i32, document: &str) -> bool {
        if generation == self.generation {
            return false;
        }
        self.generation = generation;
        let root = yaml::parse(document);
        let weekend = root.get("WeekendInfo");
        self.track_name = track_name(weekend);
        self.track_length_meters = weekend
            .get("TrackLength")
            .number()
            .map(|kilometers| kilometers * 1_000.0)
            .filter(|meters| *meters > 0.0)
            .unwrap_or(0.0);
        self.sector_starts = sector_starts(root.get("SplitTimeInfo").get("Sectors"));

        let driver_info = root.get("DriverInfo");
        self.player_car_name = player_car_name(driver_info);
        // The tank is reported at its full size; the session may cap it.
        let capacity = driver_info
            .get("DriverCarFuelMaxLtr")
            .number()
            .unwrap_or(0.0);
        let allowed = driver_info
            .get("DriverCarMaxFuelPct")
            .number()
            .filter(|fraction| *fraction > 0.0 && *fraction <= 1.0)
            .unwrap_or(1.0);
        self.fuel_capacity_liters = (capacity * allowed).max(0.0);
        self.redline_rpm = driver_info.get("DriverCarRedLine").number().unwrap_or(0.0);

        self.schedule = root
            .get("SessionInfo")
            .get("Sessions")
            .items()
            .iter()
            .map(schedule)
            .collect();
        true
    }

    pub(super) fn scheduled(&self, number: i32) -> Option<&Schedule> {
        self.schedule.iter().find(|entry| entry.number == number)
    }

    /// Which sector a lap fraction falls in, numbered the way the frame counts
    /// them: one, two, then zero for the last one.
    pub(super) fn sector(&self, lap_fraction: f64) -> i32 {
        let count = self.sector_starts.len();
        if count < 2 {
            return 0;
        }
        let index = self
            .sector_starts
            .iter()
            .rposition(|start| lap_fraction >= *start)
            .unwrap_or(0);
        if index + 1 >= count {
            0
        } else {
            index as i32 + 1
        }
    }
}

fn track_name(weekend: &Node) -> String {
    let name = weekend.get("TrackDisplayName").text().trim().to_owned();
    let configuration = weekend.get("TrackConfigName").text().trim();
    if configuration.is_empty() || configuration.eq_ignore_ascii_case("n/a") {
        return name;
    }
    if name.is_empty() {
        return configuration.to_owned();
    }
    format!("{name} - {configuration}")
}

fn sector_starts(sectors: &Node) -> Vec<f64> {
    let mut starts: Vec<f64> = sectors
        .items()
        .iter()
        .filter_map(|sector| sector.get("SectorStartPct").number())
        .filter(|start| (0.0..1.0).contains(start))
        .collect();
    starts.sort_by(f64::total_cmp);
    starts.dedup();
    starts
}

/// The roster is indexed by car slot, so the local car is the entry whose slot
/// matches the one the session names as the driver's own.
fn player_car_name(driver_info: &Node) -> String {
    let Some(player_index) = driver_info.get("DriverCarIdx").integer() else {
        return String::new();
    };
    driver_info
        .get("Drivers")
        .items()
        .iter()
        .find(|entry| entry.get("CarIdx").integer() == Some(player_index))
        .map(|entry| entry.get("CarScreenName").text().trim().to_owned())
        .unwrap_or_default()
}

fn schedule(entry: &Node) -> Schedule {
    let name = {
        let session_name = entry.get("SessionName").text().trim();
        if session_name.is_empty() {
            entry.get("SessionType").text().trim().to_owned()
        } else {
            session_name.to_owned()
        }
    };
    Schedule {
        number: entry.get("SessionNum").integer().unwrap_or(-1),
        name,
        // Unlimited is published as text, so a session that is not lap or time
        // limited stays at zero the way the frame expects.
        laps: entry.get("SessionLaps").integer().unwrap_or(0).max(0),
        seconds: entry.get("SessionTime").number().unwrap_or(0.0).max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::{session_type_code, Session};

    const DOCUMENT: &str = "WeekendInfo:\n TrackDisplayName: Spa\n TrackConfigName: Grand Prix\n TrackLength: 7.00 km\nSessionInfo:\n Sessions:\n - SessionNum: 0\n   SessionName: PRACTICE\n   SessionLaps: unlimited\n   SessionTime: 1800.0000 sec\n - SessionNum: 1\n   SessionName: RACE\n   SessionLaps: 25\n   SessionTime: unlimited\nSplitTimeInfo:\n Sectors:\n - SectorNum: 0\n   SectorStartPct: 0.0000\n - SectorNum: 1\n   SectorStartPct: 0.4000\n - SectorNum: 2\n   SectorStartPct: 0.7500\nDriverInfo:\n DriverCarIdx: 3\n DriverCarFuelMaxLtr: 100.000\n DriverCarMaxFuelPct: 0.500\n DriverCarRedLine: 7200.000\n Drivers:\n - CarIdx: 1\n   CarScreenName: Porsche 911 GT3 R\n - CarIdx: 3\n   CarScreenName: Ferrari 296 GT3\n";

    fn parsed() -> Session {
        let mut session = Session::new();
        assert!(session.refresh(1, DOCUMENT));
        session
    }

    #[test]
    fn reads_track_and_car_configuration() {
        let session = parsed();
        assert_eq!(session.track_name, "Spa - Grand Prix");
        assert_eq!(session.track_length_meters, 7_000.0);
        assert_eq!(session.fuel_capacity_liters, 50.0);
        assert_eq!(session.redline_rpm, 7_200.0);
    }

    #[test]
    fn picks_the_local_car_out_of_the_roster() {
        assert_eq!(parsed().player_car_name, "Ferrari 296 GT3");
    }

    #[test]
    fn maps_the_schedule_to_frame_session_codes() {
        let session = parsed();
        assert_eq!(
            session.scheduled(0).map(|entry| entry.seconds),
            Some(1_800.0)
        );
        assert_eq!(session.scheduled(0).map(|entry| entry.laps), Some(0));
        assert_eq!(session.scheduled(1).map(|entry| entry.laps), Some(25));
        assert_eq!(session.scheduled(1).map(|entry| entry.seconds), Some(0.0));
        assert_eq!(session_type_code("PRACTICE"), 0);
        assert_eq!(session_type_code("OPEN QUALIFY"), 5);
        assert_eq!(session_type_code("WARMUP"), 9);
        assert_eq!(session_type_code("RACE"), 10);
    }

    #[test]
    fn places_a_lap_fraction_in_its_sector() {
        let session = parsed();
        assert_eq!(session.sector_starts.len(), 3);
        assert_eq!(session.sector(0.0), 1);
        assert_eq!(session.sector(0.39), 1);
        assert_eq!(session.sector(0.40), 2);
        assert_eq!(session.sector(0.99), 0);
    }

    #[test]
    fn only_reparses_a_new_generation() {
        let mut session = parsed();
        assert!(!session.refresh(1, ""));
        assert_eq!(session.track_name, "Spa - Grand Prix");
        assert!(session.refresh(2, ""));
        assert!(session.track_name.is_empty());
    }
}
