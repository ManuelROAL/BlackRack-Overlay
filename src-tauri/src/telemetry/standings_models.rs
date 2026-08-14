use serde::{Deserialize, Serialize};
use std::sync::{OnceLock, RwLock};

use super::{StandingEntry, TelemetryFrame};

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StandingsModelSettings {
    own_class_rows: usize,
    other_class_rows: usize,
    show_other_classes: bool,
}

impl Default for StandingsModelSettings {
    fn default() -> Self {
        Self {
            own_class_rows: 10,
            other_class_rows: 3,
            show_other_classes: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RelativeModelSettings {
    ahead_rows: usize,
    behind_rows: usize,
}

impl Default for RelativeModelSettings {
    fn default() -> Self {
        Self {
            ahead_rows: 4,
            behind_rows: 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub(crate) struct OverlayViewSettings {
    standings: StandingsModelSettings,
    relative: RelativeModelSettings,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct StrengthOfFieldModel {
    label: String,
    resolved_profiles: usize,
    total_profiles: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct StandingsClassModel {
    vehicle_class: String,
    display_class: String,
    class_tone: String,
    current_count: usize,
    initial_count: usize,
    retired_count: usize,
    strength_of_field: Option<StrengthOfFieldModel>,
    visible_vehicle_ids: Vec<i32>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct StandingsViewModel {
    groups: Vec<StandingsClassModel>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum RelativeRowKind {
    Ahead,
    Player,
    Behind,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct RelativeRowModel {
    vehicle_id: i32,
    relative_gap_seconds: f64,
    kind: RelativeRowKind,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct RelativeViewModel {
    rows: Vec<RelativeRowModel>,
}

static SETTINGS: OnceLock<RwLock<OverlayViewSettings>> = OnceLock::new();

fn settings_store() -> &'static RwLock<OverlayViewSettings> {
    SETTINGS.get_or_init(|| RwLock::new(OverlayViewSettings::default()))
}

pub(crate) fn set_overlay_view_settings(mut settings: OverlayViewSettings) {
    settings.standings.own_class_rows = settings.standings.own_class_rows.clamp(3, 30);
    settings.standings.other_class_rows = settings.standings.other_class_rows.clamp(1, 15);
    settings.relative.ahead_rows = settings.relative.ahead_rows.clamp(1, 10);
    settings.relative.behind_rows = settings.relative.behind_rows.clamp(1, 10);
    *settings_store()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = settings;
}

fn class_priority(vehicle_class: &str) -> u8 {
    let normalized = vehicle_class.trim().to_ascii_uppercase();
    if normalized.contains("HYPER") || normalized.contains("GTP") {
        0
    } else if normalized.contains("LMP2") {
        1
    } else if normalized.contains("LMP3") {
        2
    } else if normalized.contains("LMGT3") || normalized.contains("GT3") {
        3
    } else {
        4
    }
}

fn display_class(vehicle_class: &str) -> String {
    let normalized = vehicle_class.trim().to_ascii_uppercase();
    if normalized.contains("HYPER") || normalized.contains("GTP") {
        "HYPERCAR".into()
    } else if normalized.contains("LMP2") {
        "LMP2".into()
    } else if normalized.contains("LMP3") {
        "LMP3".into()
    } else if normalized.contains("LMGT3") || normalized.contains("GT3") {
        "LMGT3".into()
    } else {
        vehicle_class.trim().to_owned()
    }
}

fn class_tone(vehicle_class: &str) -> String {
    match class_priority(vehicle_class) {
        0 => "hypercar",
        1 => "lmp2",
        2 => "lmp3",
        3 => "lmgt3",
        _ => "other",
    }
    .into()
}

fn continuous_driver_rank(entry: &StandingEntry) -> Option<f64> {
    if !entry.driver_rank_progress.is_finite() || entry.driver_rank_progress < 0.0 {
        return None;
    }
    let rank = entry.driver_rank.trim().to_ascii_uppercase();
    let bytes = rank.as_bytes();
    if bytes.len() != 2 || !(b'1'..=b'3').contains(&bytes[1]) {
        return None;
    }
    let level = match bytes[0] {
        b'B' => 0,
        b'S' => 3,
        b'G' => 6,
        b'P' => 9,
        _ => return None,
    };
    Some(
        (level + usize::from(bytes[1] - b'0')) as f64 * 100.0
            + entry.driver_rank_progress.min(100.0),
    )
}

fn strength_of_field(entries: &[&StandingEntry]) -> Option<StrengthOfFieldModel> {
    let ratings = entries
        .iter()
        .filter_map(|entry| continuous_driver_rank(entry))
        .collect::<Vec<_>>();
    if ratings.is_empty() {
        return None;
    }
    let average = ratings.iter().sum::<f64>() / ratings.len() as f64;
    let capped = average.clamp(100.0, 1300.0);
    let terminal_rank = capped >= 1300.0;
    let band = if terminal_rank {
        12
    } else {
        (capped / 100.0).floor() as usize
    };
    let rank_index = band.saturating_sub(1).min(11);
    let levels = ["B", "S", "G", "P"];
    let rank = format!("{}{}", levels[rank_index / 3], rank_index % 3 + 1);
    let progress = if terminal_rank {
        100
    } else {
        (capped - band as f64 * 100.0).round() as i32
    };
    Some(StrengthOfFieldModel {
        label: format!("SOF {rank} {progress}%"),
        resolved_profiles: ratings.len(),
        total_profiles: entries.len(),
    })
}

fn visible_class_entries(entries: &[&StandingEntry], requested_rows: usize) -> Vec<i32> {
    let target_size = requested_rows
        .clamp(3, entries.len().max(3))
        .min(entries.len());
    let mut visible = entries
        .iter()
        .take(target_size.min(3))
        .map(|entry| entry.vehicle_id)
        .collect::<Vec<_>>();
    let Some(player_index) = entries.iter().position(|entry| entry.is_player) else {
        return entries
            .iter()
            .take(target_size)
            .map(|entry| entry.vehicle_id)
            .collect();
    };
    let mut candidates = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            (
                index.abs_diff(player_index),
                entry.position,
                entry.vehicle_id,
            )
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    for (_, _, vehicle_id) in candidates {
        if visible.len() >= target_size {
            break;
        }
        if !visible.contains(&vehicle_id) {
            visible.push(vehicle_id);
        }
    }
    visible.sort_by_key(|vehicle_id| {
        entries
            .iter()
            .find(|entry| entry.vehicle_id == *vehicle_id)
            .map_or(i32::MAX, |entry| entry.position)
    });
    visible
}

fn prepare_standings(
    entries: &[StandingEntry],
    settings: StandingsModelSettings,
    session_type: i32,
) -> StandingsViewModel {
    let player_class = entries
        .iter()
        .find(|entry| entry.is_player)
        .map(|entry| entry.vehicle_class.as_str());
    let mut class_names = entries
        .iter()
        .map(|entry| entry.vehicle_class.as_str())
        .collect::<Vec<_>>();
    class_names.sort_unstable_by(|left, right| {
        class_priority(left)
            .cmp(&class_priority(right))
            .then_with(|| left.cmp(right))
    });
    class_names.dedup();
    let practice = (0..=4).contains(&session_type);
    let groups = class_names
        .into_iter()
        .filter(|class| {
            player_class.is_none() || player_class == Some(*class) || settings.show_other_classes
        })
        .map(|vehicle_class| {
            let class_entries = entries
                .iter()
                .filter(|entry| entry.vehicle_class == vehicle_class)
                .collect::<Vec<_>>();
            let is_player_class = player_class == Some(vehicle_class);
            let current_count = class_entries
                .iter()
                .filter(|entry| !matches!(entry.finish_status, 2 | 3))
                .count();
            let recovered_initial = class_entries
                .iter()
                .map(|entry| entry.initial_class_count)
                .max()
                .unwrap_or(0);
            let initial_count = if practice {
                current_count
            } else {
                recovered_initial.max(class_entries.len())
            };
            let visible_vehicle_ids = if is_player_class {
                visible_class_entries(&class_entries, settings.own_class_rows)
            } else {
                class_entries
                    .iter()
                    .take(settings.other_class_rows)
                    .map(|entry| entry.vehicle_id)
                    .collect()
            };
            StandingsClassModel {
                vehicle_class: vehicle_class.to_owned(),
                display_class: display_class(vehicle_class),
                class_tone: class_tone(vehicle_class),
                current_count,
                initial_count,
                retired_count: initial_count.saturating_sub(current_count),
                strength_of_field: (!practice)
                    .then(|| strength_of_field(&class_entries))
                    .flatten(),
                visible_vehicle_ids,
            }
        })
        .collect();
    StandingsViewModel { groups }
}

fn prepare_relative(
    entries: &[StandingEntry],
    settings: RelativeModelSettings,
) -> RelativeViewModel {
    let Some(player) = entries.iter().find(|entry| entry.is_player) else {
        return RelativeViewModel::default();
    };
    let mut ahead = entries
        .iter()
        .filter(|entry| {
            !entry.is_player
                && !entry.in_garage
                && entry.relative_ahead_seconds.is_finite()
                && entry.relative_ahead_seconds < -0.05
        })
        .map(|entry| (entry.relative_ahead_seconds.abs(), entry))
        .collect::<Vec<_>>();
    ahead.sort_by(|left, right| left.0.total_cmp(&right.0));
    ahead.truncate(settings.ahead_rows);
    ahead.reverse();

    let mut behind = entries
        .iter()
        .filter(|entry| {
            !entry.is_player
                && !entry.in_garage
                && entry.relative_behind_seconds.is_finite()
                && entry.relative_behind_seconds > 0.05
        })
        .collect::<Vec<_>>();
    behind.sort_by(|left, right| {
        left.relative_behind_seconds
            .total_cmp(&right.relative_behind_seconds)
    });
    behind.truncate(settings.behind_rows);

    let rows = ahead
        .into_iter()
        .map(|(_, entry)| RelativeRowModel {
            vehicle_id: entry.vehicle_id,
            relative_gap_seconds: entry.relative_ahead_seconds,
            kind: RelativeRowKind::Ahead,
        })
        .chain(std::iter::once(RelativeRowModel {
            vehicle_id: player.vehicle_id,
            relative_gap_seconds: 0.0,
            kind: RelativeRowKind::Player,
        }))
        .chain(behind.into_iter().map(|entry| RelativeRowModel {
            vehicle_id: entry.vehicle_id,
            relative_gap_seconds: entry.relative_behind_seconds,
            kind: RelativeRowKind::Behind,
        }))
        .collect();
    RelativeViewModel { rows }
}

pub(super) fn prepare_overlay_models(frame: &mut TelemetryFrame) {
    if frame.standings.is_empty() {
        frame.standings_model = StandingsViewModel::default();
        frame.relative_model = RelativeViewModel::default();
        return;
    }
    let settings = *settings_store()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    frame.standings_model =
        prepare_standings(&frame.standings, settings.standings, frame.session_type);
    frame.relative_model = prepare_relative(&frame.standings, settings.relative);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i32, position: i32, class: &str, player: bool) -> StandingEntry {
        StandingEntry {
            vehicle_id: id,
            position,
            vehicle_class: class.into(),
            is_player: player,
            driver_rank: "G2".into(),
            driver_rank_progress: 50.0,
            initial_class_count: 8,
            relative_ahead_seconds: -(id as f64),
            relative_behind_seconds: id as f64,
            ..StandingEntry::default()
        }
    }

    #[test]
    fn player_class_keeps_top_three_and_player_neighbours() {
        let entries = (1..=8)
            .map(|id| entry(id, id, "LMGT3", id == 7))
            .collect::<Vec<_>>();
        let model = prepare_standings(
            &entries,
            StandingsModelSettings {
                own_class_rows: 5,
                ..Default::default()
            },
            10,
        );
        assert_eq!(model.groups[0].visible_vehicle_ids, vec![1, 2, 3, 6, 7]);
    }

    #[test]
    fn classes_are_ordered_and_other_classes_can_be_hidden() {
        let entries = vec![
            entry(1, 1, "LMGT3", false),
            entry(2, 1, "Hypercar", true),
            entry(3, 1, "LMP2", false),
        ];
        let all = prepare_standings(&entries, StandingsModelSettings::default(), 10);
        assert_eq!(
            all.groups
                .iter()
                .map(|group| group.class_tone.as_str())
                .collect::<Vec<_>>(),
            vec!["hypercar", "lmp2", "lmgt3"]
        );
        let own = prepare_standings(
            &entries,
            StandingsModelSettings {
                show_other_classes: false,
                ..Default::default()
            },
            10,
        );
        assert_eq!(own.groups.len(), 1);
        assert_eq!(own.groups[0].vehicle_class, "Hypercar");
    }

    #[test]
    fn abbreviated_hyper_class_uses_hypercar_header() {
        let model = prepare_standings(
            &[entry(1, 1, "Hyper", true)],
            StandingsModelSettings::default(),
            10,
        );

        assert_eq!(model.groups[0].display_class, "HYPERCAR");
        assert_eq!(model.groups[0].class_tone, "hypercar");
    }

    #[test]
    fn relative_is_prepared_in_physical_order_and_can_repeat_across_directions() {
        let entries = vec![
            entry(1, 1, "Hypercar", true),
            entry(2, 2, "Hypercar", false),
            entry(3, 3, "Hypercar", false),
        ];
        let model = prepare_relative(
            &entries,
            RelativeModelSettings {
                ahead_rows: 2,
                behind_rows: 1,
            },
        );
        assert_eq!(
            model
                .rows
                .iter()
                .map(|row| row.vehicle_id)
                .collect::<Vec<_>>(),
            vec![3, 2, 1, 2]
        );
        assert_eq!(model.rows[0].relative_gap_seconds, -3.0);
        assert_eq!(model.rows[3].relative_gap_seconds, 2.0);
    }
}
