//! Shared progress math for standings-family telemetry adapters.

/// Converts the progress and timing difference between an ahead car and a
/// behind car into the common GAP/INT pair. The simulator adapters supply the
/// native progress and, when available, a more precise same-lap timing delta.
pub(crate) fn class_relative_gap(
    lap_difference: f64,
    timing_difference: Option<f64>,
    pace_seconds: f64,
) -> (i32, f64) {
    if !lap_difference.is_finite() {
        return (0, 0.0);
    }
    if lap_difference >= 1.0 {
        return (lap_difference.floor() as i32, 0.0);
    }
    let Some(mut time_gap) = timing_difference.or_else(|| {
        pace_seconds
            .is_finite()
            .then_some(pace_seconds)
            .filter(|pace| *pace > 0.0)
            .map(|pace| lap_difference * pace)
    }) else {
        return (0, 0.0);
    };
    if time_gap < 0.0 && lap_difference > 0.0 {
        time_gap += if pace_seconds.is_finite() {
            pace_seconds.max(1.0)
        } else {
            1.0
        };
    }
    (0, time_gap.abs().max(0.0))
}

#[cfg(test)]
mod tests {
    use super::class_relative_gap;

    #[test]
    fn reports_laps_before_seconds_when_progress_is_a_full_lap_ahead() {
        assert_eq!(class_relative_gap(1.2, Some(-2.0), 100.0), (1, 0.0));
    }

    #[test]
    fn uses_native_timing_for_a_same_lap_gap() {
        assert_eq!(class_relative_gap(0.02, Some(1.75), 100.0), (0, 1.75));
    }

    #[test]
    fn derives_same_lap_time_from_progress_when_native_timing_is_missing() {
        assert_eq!(class_relative_gap(0.02, None, 100.0), (0, 2.0));
    }
}
