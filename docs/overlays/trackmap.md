# Track Map overlay

## Scope and files

- Entry: `trackmap.html`
- Renderer/style: `src/trackmap.ts`, `src/trackmap.css`
- Rust model: `src-tauri/src/telemetry/track_map_model.rs`
- REST geometry: `src-tauri/src/telemetry/track_geometry.rs`
- Shared-memory coordinates: `src-tauri/src/telemetry/lmu_bridge.cpp`
- OBS route/API: `/trackmap`, `/api/trackmap`
- Cadence: lightweight coordinate roster at approximately 30 Hz

Track Map must not request the enriched Standings roster. Rust owns fallback
learning, pit-passage validation/learning and post-stop lap-distance prediction;
TypeScript fetches static geometry, interpolates and renders it.

## Coordinates and geometry

- Prefer matched per-vehicle telemetry coordinates over slower scoring position.
  Normalize LMU lateral position as `world_y = -mPos.z`.
- Fetch `/rest/watch/trackmap` once per circuit. Type 0 is the ordered main
  centerline and type 1 the open pitlane. Filter other families and cache valid
  geometry by normalized circuit key.
- Split type-1 geometry at discontinuities and keep only its longest continuous
  route so unused alternate pit paths are not joined or displayed.
- Draw that pitlane as a thin secondary open line beneath the main circuit, using
  the same transform. Do not close it or invent one for learned and fallback maps.
- Transfer official/learned geometry outside telemetry frames and retain cached
  official paths by shared reference in hot backend paths.
- Align direction and lap-distance origin after a few metres of non-pit player
  movement.
- If official geometry is unavailable, sample the player every three metres and
  persist only a complete valid non-pit lap covering at least 88% of official
  length. The circular projection is the final fallback.
- Persist Rust learning in `track-map-learning.json`; validate and migrate old
  browser `localStorage` learning once.

## Pit passage and prediction

- The separate white `P` is the predicted post-stop player position, not the
  player marker. Its visibility is independently configurable and defaults to on;
  disabling it does not stop pit-passage learning.
- Combine authoritative REST service total with the median learned moving time
  through the pitlane, then convert it using the latest valid player lap with best
  lap as fallback. Rust serializes the prepared lap distance; the renderer only
  interpolates it.
- Learn moving time per circuit from complete pit passages by any visible vehicle.
  Exclude stationary service time and keep the last seven valid observations.
- With official type-1 geometry, accept only endpoint-to-endpoint progress. Do not
  seed from a partial passage or invent a circuit-wide fallback before a complete
  passage is observed. Latch the entry and exit endpoints throughout the observed
  pit traversal rather than requiring the `in_pits` transition sample itself to
  fall within endpoint proximity.
- Scan type-1 proximity only for vehicles entering, traversing or exiting pitlane.
- Hide prediction when the player is in pitlane or any required input is missing.

## Markers and rendering

- Draw the static circuit in SVG and cache vehicle markers in the HTML layer above
  it. Update transforms directly at telemetry cadence.
- Do not add continuous CSS transitions, a permanent animation loop or per-marker
  SVG filters; these keep the WebView2 GPU process awake.
- Vehicles use 24 px class-colored circles with their class position in black;
  pit cars remain at reduced opacity.
- The player keeps its class-position label in a 28 px circle and has a larger
  pulsing lime halo advanced by telemetry events. Never replace its label with `P`.
- Only the overall leader receives a static gold star. Keep the complete leader
  marker above all others and distinct from the player's halo; class leaders have
  no special mark.

## Verification focus

Validate type-0/type-1 geometry, alignment, fallback lap completion, persistence,
partial/full pit passages, service-time composition, marker stacking and GPU idle
behavior on at least one additional circuit.
