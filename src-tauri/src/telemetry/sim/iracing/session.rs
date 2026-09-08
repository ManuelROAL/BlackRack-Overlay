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
    pub(super) results: Vec<ResultPosition>,
}

#[derive(Clone, Default)]
pub(super) struct Driver {
    pub(super) car_idx: i32,
    pub(super) user_name: String,
    pub(super) abbrev_name: String,
    pub(super) initials: String,
    pub(super) team_name: String,
    pub(super) car_number: String,
    pub(super) car_screen_name: String,
    pub(super) car_class_short_name: String,
    pub(super) license: String,
    pub(super) irating: i32,
    pub(super) nationality: String,
    pub(super) is_spectator: bool,
    pub(super) is_pace_car: bool,
}

#[derive(Clone, Copy, Default)]
pub(super) struct ResultPosition {
    pub(super) car_idx: i32,
    /// iRacing stores these positions zero-based in the session results. The
    /// source converts them to the one-based contract used by the frame.
    pub(super) overall_position: Option<i32>,
    pub(super) class_position: Option<i32>,
    pub(super) laps_complete: i32,
    pub(super) fastest_lap_time: f64,
    pub(super) last_lap_time: f64,
}

#[derive(Default)]
pub(super) struct Session {
    generation: i32,
    pub(super) track_name: String,
    pub(super) track_length_meters: f64,
    /// Lap fraction where each sector starts, first entry always zero.
    pub(super) sector_starts: Vec<f64>,
    pub(super) player_car_name: String,
    pub(super) player_car_idx: i32,
    pub(super) fuel_capacity_liters: f64,
    pub(super) redline_rpm: f64,
    pub(super) schedule: Vec<Schedule>,
    pub(super) drivers: Vec<Driver>,
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
        self.player_car_idx = driver_info.get("DriverCarIdx").integer().unwrap_or(-1);
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
        let parsed_drivers = driver_info
            .get("Drivers")
            .items()
            .iter()
            .map(driver)
            .filter(|driver| driver.car_idx >= 0 && !driver.is_spectator && !driver.is_pace_car)
            .collect();
        self.drivers = deduplicate_drivers(parsed_drivers);

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

    pub(super) fn results(&self, number: i32) -> &[ResultPosition] {
        self.scheduled(number)
            .map_or(&[], |schedule| schedule.results.as_slice())
    }

    /// DriverInfo can contain one entry per driver in a team car. Standings
    /// are car-based, and the simulator's last entry is the active driver in
    /// that case; the roster is already deduplicated during refresh so the hot
    /// standings path only borrows these records.
    pub(super) fn cars(&self) -> &[Driver] {
        &self.drivers
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

fn text_or(entry: &Node, keys: &[&str]) -> String {
    keys.iter()
        .map(|key| entry.get(key).text().trim())
        .find(|value| !value.is_empty())
        .unwrap_or_default()
        .to_owned()
}

fn boolean(entry: &Node, key: &str) -> bool {
    matches!(
        entry.get(key).text().trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes"
    )
}

fn driver(entry: &Node) -> Driver {
    let class = text_or(entry, &["CarClassShortName", "CarClassName"]);
    let class = if class.is_empty() {
        entry
            .get("CarClassID")
            .integer()
            .map_or_else(String::new, |id| format!("CLASS {id}"))
    } else {
        class
    };
    Driver {
        car_idx: entry.get("CarIdx").integer().unwrap_or(-1),
        user_name: text_or(entry, &["UserName", "Name"]),
        abbrev_name: text_or(entry, &["AbbrevName"]),
        initials: text_or(entry, &["Initials"]),
        team_name: text_or(entry, &["TeamName"]),
        car_number: text_or(entry, &["CarNumber", "CarNumberRaw"]),
        car_screen_name: text_or(entry, &["CarScreenName", "CarName"]),
        car_class_short_name: class,
        license: text_or(entry, &["LicString", "License"]),
        irating: entry.get("IRating").integer().unwrap_or(0).max(0),
        nationality: text_or(entry, &["LicCountry", "LicenseCountry", "Country"]),
        is_spectator: boolean(entry, "IsSpectator"),
        is_pace_car: boolean(entry, "CarIsPaceCar"),
    }
}

fn deduplicate_drivers(drivers: Vec<Driver>) -> Vec<Driver> {
    let mut cars = Vec::with_capacity(drivers.len());
    for driver in drivers {
        if let Some(existing) = cars
            .iter_mut()
            .find(|entry: &&mut Driver| entry.car_idx == driver.car_idx)
        {
            *existing = driver;
        } else {
            cars.push(driver);
        }
    }
    cars
}

fn one_based(value: Option<i32>) -> Option<i32> {
    value.filter(|value| *value >= 0).map(|value| value + 1)
}

fn first_time(entry: &Node, keys: &[&str]) -> f64 {
    keys.iter()
        .filter_map(|key| entry.get(key).number())
        .find(|value| *value > 0.0)
        .unwrap_or(0.0)
}

fn result_position(entry: &Node) -> ResultPosition {
    ResultPosition {
        car_idx: entry.get("CarIdx").integer().unwrap_or(-1),
        overall_position: one_based(entry.get("Position").integer()),
        class_position: one_based(entry.get("ClassPosition").integer()),
        laps_complete: entry.get("LapsComplete").integer().unwrap_or(-1),
        fastest_lap_time: first_time(entry, &["FastestTime", "FastestLapTime"]),
        last_lap_time: first_time(entry, &["LastTime", "LastLapTime"]),
    }
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
        results: entry
            .get("ResultsPositions")
            .items()
            .iter()
            .map(result_position)
            .filter(|result| result.car_idx >= 0)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::{deduplicate_drivers, session_type_code, Driver, Session};

    const DOCUMENT: &str = "WeekendInfo:\n TrackDisplayName: Spa\n TrackConfigName: Grand Prix\n TrackLength: 7.00 km\nSessionInfo:\n Sessions:\n - SessionNum: 0\n   SessionName: PRACTICE\n   SessionLaps: unlimited\n   SessionTime: 1800.0000 sec\n - SessionNum: 1\n   SessionName: RACE\n   SessionLaps: 25\n   SessionTime: unlimited\n   ResultsPositions:\n   - Position: 0\n     ClassPosition: 0\n     CarIdx: 3\n     LapsComplete: 4\n     FastestTime: 130.5 sec\n     LastTime: 131.5 sec\nSplitTimeInfo:\n Sectors:\n - SectorNum: 0\n   SectorStartPct: 0.0000\n - SectorNum: 1\n   SectorStartPct: 0.4000\n - SectorNum: 2\n   SectorStartPct: 0.7500\nDriverInfo:\n DriverCarIdx: 3\n DriverCarFuelMaxLtr: 100.000\n DriverCarMaxFuelPct: 0.500\n DriverCarRedLine: 7200.000\n Drivers:\n - CarIdx: 1\n   UserName: Ana Perez\n   CarScreenName: Porsche 911 GT3 R\n   CarClassShortName: GT3\n   CarNumber: 12\n   LicString: A 4.99\n   IRating: 2400\n   LicCountry: ES\n - CarIdx: 3\n   UserName: Juan Perez\n   CarScreenName: Ferrari 296 GT3\n   CarClassShortName: GT3\n";

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
        assert_eq!(parsed().player_car_idx, 3);
        assert_eq!(parsed().cars().len(), 2);
        assert_eq!(parsed().cars()[0].license, "A 4.99");
        assert_eq!(parsed().cars()[0].irating, 2400);
        assert_eq!(parsed().cars()[0].nationality, "ES");
    }

    #[test]
    fn keeps_the_last_driver_for_a_team_car() {
        let cars = deduplicate_drivers(vec![
            Driver {
                car_idx: 5,
                user_name: "First stint".into(),
                ..Driver::default()
            },
            Driver {
                car_idx: 5,
                user_name: "Active stint".into(),
                ..Driver::default()
            },
        ]);

        assert_eq!(cars.len(), 1);
        assert_eq!(cars[0].user_name, "Active stint");
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
        assert_eq!(session.results(1)[0].overall_position, Some(1));
        assert_eq!(session.results(1)[0].class_position, Some(1));
        assert_eq!(session.results(1)[0].laps_complete, 4);
        assert_eq!(session.results(1)[0].fastest_lap_time, 130.5);
        assert_eq!(session.results(1)[0].last_lap_time, 131.5);
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
