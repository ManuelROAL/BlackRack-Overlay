//! Serializes one grouped overlay batch per source cycle, reduced to the fields
//! its targets read.
//!
//! The field table is the same file the composite host projects with, so the
//! backend never ships a field no mounted overlay consumes and the host never
//! looks for one the backend left out.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

const FIELD_TABLE_JSON: &str = include_str!("../../../src/overlay-telemetry-fields.json");

#[derive(Deserialize)]
struct FieldTable {
    shared: Vec<String>,
    overlays: HashMap<String, Vec<String>>,
}

fn field_table() -> &'static FieldTable {
    static TABLE: OnceLock<FieldTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str(FIELD_TABLE_JSON)
            .expect("src/overlay-telemetry-fields.json is validated by the test suite")
    })
}

#[derive(Serialize)]
struct OverlayBatch<'a> {
    targets: &'a [&'a str],
    frame: Map<String, Value>,
}

/// The JSON bytes of `{ targets, frame }`, where `frame` carries only the shared
/// fields and those of each target. `None` when there is nothing to deliver.
pub(super) fn overlay_batch_bytes<T: Serialize>(
    frame: &T,
    targets: &[&str],
) -> serde_json::Result<Option<Vec<u8>>> {
    if targets.is_empty() {
        return Ok(None);
    }
    let Value::Object(mut source) = serde_json::to_value(frame)? else {
        return Ok(None);
    };
    let table = field_table();
    let mut projected = Map::new();
    let fields = table.shared.iter().chain(
        targets
            .iter()
            .filter_map(|target| table.overlays.get(*target))
            .flatten(),
    );
    for field in fields {
        if let Some(value) = source.remove(field) {
            projected.insert(field.clone(), value);
        }
    }
    serde_json::to_vec(&OverlayBatch {
        targets,
        frame: projected,
    })
    .map(Some)
}

#[cfg(test)]
mod tests {
    use super::{field_table, overlay_batch_bytes};
    use crate::telemetry::TelemetryFrame;
    use serde_json::Value;

    fn serialized_frame() -> serde_json::Map<String, Value> {
        match serde_json::to_value(TelemetryFrame::waiting_for_simulator(false)).unwrap() {
            Value::Object(fields) => fields,
            other => panic!("frame serialized as {other}"),
        }
    }

    #[test]
    fn every_projected_field_exists_on_the_frame() {
        let frame = serialized_frame();
        let table = field_table();
        for field in &table.shared {
            assert!(
                frame.contains_key(field),
                "shared field {field} is not serialized"
            );
        }
        for (overlay, fields) in &table.overlays {
            for field in fields {
                assert!(
                    frame.contains_key(field),
                    "{overlay} projects {field}, which the frame does not serialize"
                );
            }
        }
    }

    #[test]
    fn field_table_lists_every_overlay() {
        let table = field_table();
        for label in crate::OVERLAY_LABELS {
            assert!(
                table.overlays.contains_key(label),
                "{label} has no field list"
            );
        }
        assert_eq!(table.overlays.len(), crate::OVERLAY_LABELS.len());
    }

    #[test]
    fn batch_carries_only_the_targets_fields() {
        let frame = TelemetryFrame::waiting_for_simulator(false);
        let bytes = overlay_batch_bytes(&frame, &["flags", "rejoin"])
            .unwrap()
            .unwrap();
        let batch: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(batch["targets"], serde_json::json!(["flags", "rejoin"]));
        let mut fields: Vec<_> = batch["frame"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        fields.sort();
        assert_eq!(fields, ["capabilities", "flag_warning", "rejoin_warning"]);
    }

    #[test]
    fn no_targets_means_no_batch() {
        let frame = TelemetryFrame::waiting_for_simulator(false);
        assert!(overlay_batch_bytes(&frame, &[]).unwrap().is_none());
    }
}
