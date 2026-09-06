use serde::Serialize;
use std::time::{Duration, Instant};

use super::track_geometry::{cached_official_track_map_geometry, official_track_map_geometry};
use super::track_map_model::{
    learned_pit_speed, learned_pit_traversal_seconds, save_pit_speed, track_map_cache_key,
};

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub(crate) struct PitTraversalEstimate {
    pub seconds: Option<f64>,
    pub approximate: bool,
}

pub(super) fn choose_estimate(measured: f64, length: f64, speed: f64) -> PitTraversalEstimate {
    if measured.is_finite() && measured > 0.0 {
        return PitTraversalEstimate {
            seconds: Some(measured),
            approximate: false,
        };
    }
    let seconds = length / speed;
    if length.is_finite()
        && length > 0.0
        && speed.is_finite()
        && speed > 0.0
        && seconds.is_finite()
        && (5.0..=180.0).contains(&seconds)
    {
        PitTraversalEstimate {
            seconds: Some(seconds),
            approximate: true,
        }
    } else {
        PitTraversalEstimate::default()
    }
}

#[derive(Clone, Copy)]
pub(super) struct PitSpeedSample {
    pub time: f64,
    pub speed_ms: f64,
    pub in_pits: bool,
    pub limiter: bool,
    pub throttle: f64,
    pub brake: f64,
}

#[derive(Default)]
struct StableSpeed {
    candidate: Option<(f64, f64, f64)>,
}

impl StableSpeed {
    fn observe(&mut self, sample: PitSpeedSample) -> Option<f64> {
        if !sample.in_pits
            || !sample.limiter
            || sample.throttle < 0.95
            || sample.brake > 0.01
            || !sample.time.is_finite()
            || !sample.throttle.is_finite()
            || !sample.brake.is_finite()
            || !sample.speed_ms.is_finite()
            || !(5.0..=50.0).contains(&sample.speed_ms)
        {
            self.candidate = None;
            return None;
        }
        let (start, previous, speed) =
            self.candidate
                .unwrap_or((sample.time, sample.time, sample.speed_ms));
        if sample.time < previous
            || sample.time - previous > 0.5
            || (speed - sample.speed_ms).abs() > 0.4
        {
            self.candidate = Some((sample.time, sample.time, sample.speed_ms));
            return None;
        }
        self.candidate = Some((start, sample.time, speed));
        if sample.time - start >= 2.0 {
            self.candidate = None;
            Some((speed + sample.speed_ms) / 2.0)
        } else {
            None
        }
    }
}

#[derive(Default)]
pub(super) struct PitTraversalEstimator {
    key: String,
    speed: StableSpeed,
    last_request: Option<Instant>,
}

impl PitTraversalEstimator {
    pub fn observe(
        &mut self,
        track_name: &str,
        track_length: f64,
        sample: PitSpeedSample,
        requested: bool,
    ) -> PitTraversalEstimate {
        let key = track_map_cache_key(track_name, track_length);
        if self.key != key {
            self.key = key;
            self.speed = StableSpeed::default();
            self.last_request = None;
        }
        if track_name.trim().is_empty() || !track_length.is_finite() || track_length <= 100.0 {
            return PitTraversalEstimate::default();
        }
        if let Some(speed) = self.speed.observe(sample) {
            save_pit_speed(track_name, track_length, speed);
        }
        let measured = learned_pit_traversal_seconds(track_name, track_length);
        if measured > 0.0 {
            return choose_estimate(measured, 0.0, 0.0);
        }
        let geometry = cached_official_track_map_geometry(&self.key);
        if measured <= 0.0
            && geometry.is_none()
            && requested
            && self
                .last_request
                .is_none_or(|time| time.elapsed() >= Duration::from_secs(10))
        {
            self.last_request = Some(Instant::now());
            let key = self.key.clone();
            std::thread::spawn(move || {
                let _ = official_track_map_geometry(&key);
            });
        }
        choose_estimate(
            measured,
            geometry.map_or(0.0, |map| map.pit_length_meters),
            learned_pit_speed(track_name, track_length),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_passage_replaces_geometry_without_needing_speed() {
        assert_eq!(choose_estimate(0.0, 600.0, 20.0).seconds, Some(30.0));
        assert!(choose_estimate(0.0, 600.0, 20.0).approximate);
        assert_eq!(choose_estimate(34.0, 600.0, 0.0).seconds, Some(34.0));
        assert!(!choose_estimate(34.0, 600.0, 20.0).approximate);
        for (length, speed) in [
            (0.0, 20.0),
            (600.0, 0.0),
            (f64::NAN, 20.0),
            (600.0, f64::INFINITY),
        ] {
            assert!(choose_estimate(0.0, length, speed).seconds.is_none());
        }
    }

    #[test]
    fn calibration_requires_sustained_limiter_speed_and_resets_on_braking() {
        let mut state = StableSpeed::default();
        let sample = PitSpeedSample {
            time: 0.0,
            speed_ms: 20.0,
            in_pits: true,
            limiter: true,
            throttle: 1.0,
            brake: 0.0,
        };
        for step in 0..8 {
            assert!(state
                .observe(PitSpeedSample {
                    time: step as f64 * 0.25,
                    ..sample
                })
                .is_none());
        }
        assert_eq!(
            state.observe(PitSpeedSample {
                time: 2.0,
                ..sample
            }),
            Some(20.0)
        );
        state.observe(PitSpeedSample {
            time: 3.0,
            ..sample
        });
        state.observe(PitSpeedSample {
            time: 3.25,
            brake: 0.5,
            ..sample
        });
        assert!(state
            .observe(PitSpeedSample {
                time: 3.5,
                ..sample
            })
            .is_none());
        assert!(state
            .observe(PitSpeedSample {
                time: 8.0,
                ..sample
            })
            .is_none());
        assert!(state
            .observe(PitSpeedSample {
                time: 8.25,
                limiter: false,
                ..sample
            })
            .is_none());
        assert!(state.candidate.is_none());
    }

    #[test]
    fn acceleration_does_not_calibrate_a_pit_limit() {
        let mut state = StableSpeed::default();
        for step in 0..20 {
            assert!(state
                .observe(PitSpeedSample {
                    time: step as f64 * 0.25,
                    speed_ms: 10.0 + step as f64,
                    in_pits: true,
                    limiter: true,
                    throttle: 1.0,
                    brake: 0.0,
                })
                .is_none());
        }
    }
}
