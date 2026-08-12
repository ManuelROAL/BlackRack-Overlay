use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Deserialize)]
struct RawTrackMapPoint {
    #[serde(rename = "type")]
    kind: i32,
    x: f64,
    z: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct TrackGeometryPoint {
    x: f64,
    y: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OfficialTrackMapGeometry {
    main_path: Vec<TrackGeometryPoint>,
    pit_path: Vec<TrackGeometryPoint>,
}

static GEOMETRY_CACHE: OnceLock<Mutex<HashMap<String, OfficialTrackMapGeometry>>> = OnceLock::new();

fn decode_geometry(points: Vec<RawTrackMapPoint>) -> Result<OfficialTrackMapGeometry, String> {
    let collect = |kind| {
        points
            .iter()
            .filter(|point| point.kind == kind && point.x.is_finite() && point.z.is_finite())
            .map(|point| TrackGeometryPoint {
                x: point.x,
                // The bridge exposes map Y as -mPos.z so SVG coordinates keep
                // the same orientation as the existing learned circuit.
                y: -point.z,
            })
            .collect::<Vec<_>>()
    };
    let main_path = collect(0);
    let pit_path = collect(1);
    if main_path.len() < 40 {
        return Err("La geometria oficial no contiene un trazado principal valido".into());
    }
    if pit_path.len() < 2 {
        return Err("La geometria oficial no contiene un pitlane valido".into());
    }
    Ok(OfficialTrackMapGeometry {
        main_path,
        pit_path,
    })
}

pub(crate) fn official_track_map_geometry(
    cache_key: &str,
) -> Result<OfficialTrackMapGeometry, String> {
    let key = cache_key.trim();
    if key.is_empty() {
        return Err("Falta la clave del circuito".into());
    }
    let cache = GEOMETRY_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(geometry) = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(key)
        .cloned()
    {
        return Ok(geometry);
    }

    let geometry = fetch_geometry()?;
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key.to_string(), geometry.clone());
    Ok(geometry)
}

#[cfg(all(target_os = "windows", lmu_sdk))]
fn fetch_geometry() -> Result<OfficialTrackMapGeometry, String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_millis(400))
        .timeout(Duration::from_millis(800))
        .build()
        .map_err(|error| format!("No se pudo preparar la consulta del mapa: {error}"))?;
    let points = client
        .get("http://127.0.0.1:6397/rest/watch/trackmap")
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("LMU no ha devuelto la geometria del circuito: {error}"))?
        .json::<Vec<RawTrackMapPoint>>()
        .map_err(|error| format!("La geometria de LMU no tiene el formato esperado: {error}"))?;
    decode_geometry(points)
}

#[cfg(not(all(target_os = "windows", lmu_sdk)))]
fn fetch_geometry() -> Result<OfficialTrackMapGeometry, String> {
    let _ = Duration::ZERO;
    Err("La geometria oficial solo esta disponible con la telemetria de LMU".into())
}

#[cfg(test)]
mod tests {
    use super::{decode_geometry, RawTrackMapPoint};

    #[test]
    fn separates_main_track_and_pitlane_and_ignores_other_types() {
        let mut points = (0..45)
            .map(|index| RawTrackMapPoint {
                kind: 0,
                x: index as f64,
                z: index as f64 * 2.0,
            })
            .collect::<Vec<_>>();
        points.extend((0..3).map(|index| RawTrackMapPoint {
            kind: 1,
            x: 100.0 + index as f64,
            z: 200.0,
        }));
        points.push(RawTrackMapPoint {
            kind: 106,
            x: 999.0,
            z: 999.0,
        });

        let geometry = decode_geometry(points).expect("valid geometry");
        assert_eq!(geometry.main_path.len(), 45);
        assert_eq!(geometry.pit_path.len(), 3);
        assert_eq!(geometry.main_path[1].x, 1.0);
        assert_eq!(geometry.main_path[1].y, -2.0);
    }

    #[test]
    fn rejects_incomplete_geometry() {
        let points = vec![RawTrackMapPoint {
            kind: 0,
            x: 0.0,
            z: 0.0,
        }];
        assert!(decode_geometry(points).is_err());
    }
}
