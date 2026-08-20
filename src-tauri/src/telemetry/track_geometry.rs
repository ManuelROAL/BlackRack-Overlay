use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
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
    pub(super) x: f64,
    pub(super) y: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OfficialTrackMapGeometry {
    pub(super) main_path: Vec<TrackGeometryPoint>,
    pub(super) pit_path: Vec<TrackGeometryPoint>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct PreparedTrackGeometryPoint {
    x: f64,
    y: f64,
    distance: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrackMapGeometry {
    source: &'static str,
    main_path: Vec<PreparedTrackGeometryPoint>,
    main_length: f64,
    pit_path: Vec<TrackGeometryPoint>,
}

static GEOMETRY_CACHE: OnceLock<Mutex<HashMap<String, Arc<OfficialTrackMapGeometry>>>> =
    OnceLock::new();

const PIT_PATH_DISCONTINUITY_METERS: f64 = 25.0;

pub(super) fn cached_official_track_map_geometry(
    cache_key: &str,
) -> Option<Arc<OfficialTrackMapGeometry>> {
    GEOMETRY_CACHE.get().and_then(|cache| {
        cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(cache_key)
            .cloned()
    })
}

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
    let pit_path = select_primary_pit_path(collect(1));
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

fn path_length(points: &[TrackGeometryPoint]) -> f64 {
    points
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y))
        .sum()
}

fn select_primary_pit_path(points: Vec<TrackGeometryPoint>) -> Vec<TrackGeometryPoint> {
    let mut paths = Vec::new();
    let mut current = Vec::new();
    for point in points {
        if current.last().is_some_and(|previous: &TrackGeometryPoint| {
            (point.x - previous.x).hypot(point.y - previous.y) > PIT_PATH_DISCONTINUITY_METERS
        }) {
            if current.len() >= 2 {
                paths.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
        current.push(point);
    }
    if current.len() >= 2 {
        paths.push(current);
    }
    paths
        .into_iter()
        .max_by(|left, right| path_length(left).total_cmp(&path_length(right)))
        .unwrap_or_default()
}

pub(crate) fn official_track_map_geometry(
    cache_key: &str,
) -> Result<Arc<OfficialTrackMapGeometry>, String> {
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

    let geometry = Arc::new(fetch_geometry()?);
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key.to_string(), geometry.clone());
    Ok(geometry)
}

pub(crate) fn track_map_geometry(cache_key: &str) -> Result<TrackMapGeometry, String> {
    match official_track_map_geometry(cache_key) {
        Ok(geometry) => {
            let mut distance = 0.0;
            let mut main_path = Vec::with_capacity(geometry.main_path.len());
            for (index, point) in geometry.main_path.iter().enumerate() {
                if let Some(previous) = index
                    .checked_sub(1)
                    .and_then(|previous| geometry.main_path.get(previous))
                {
                    distance += (point.x - previous.x).hypot(point.y - previous.y);
                }
                main_path.push(PreparedTrackGeometryPoint {
                    x: point.x,
                    y: point.y,
                    distance,
                });
            }
            let main_length = main_path
                .first()
                .zip(main_path.last())
                .map_or(distance, |(first, last)| {
                    distance + (first.x - last.x).hypot(first.y - last.y)
                });
            Ok(TrackMapGeometry {
                source: "official",
                main_path,
                main_length,
                pit_path: geometry.pit_path.clone(),
            })
        }
        Err(official_error) => {
            let Some((points, track_length)) =
                super::track_map_model::learned_track_points(cache_key)
            else {
                return Err(official_error);
            };
            Ok(TrackMapGeometry {
                source: "learned",
                main_path: points
                    .into_iter()
                    .map(|point| PreparedTrackGeometryPoint {
                        x: point.x,
                        y: point.y,
                        distance: point.distance,
                    })
                    .collect(),
                main_length: track_length,
                pit_path: Vec::new(),
            })
        }
    }
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
    fn selects_longest_continuous_pitlane_without_joining_alternatives() {
        let mut points = (0..45)
            .map(|index| RawTrackMapPoint {
                kind: 0,
                x: index as f64,
                z: 0.0,
            })
            .collect::<Vec<_>>();
        points.extend((0..5).map(|index| RawTrackMapPoint {
            kind: 1,
            x: index as f64 * 5.0,
            z: 10.0,
        }));
        points.extend((0..3).map(|index| RawTrackMapPoint {
            kind: 1,
            x: 200.0 + index as f64 * 5.0,
            z: 100.0,
        }));

        let geometry = decode_geometry(points).expect("valid geometry");
        assert_eq!(geometry.pit_path.len(), 5);
        assert_eq!(geometry.pit_path.first().map(|point| point.x), Some(0.0));
        assert_eq!(geometry.pit_path.last().map(|point| point.x), Some(20.0));
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
