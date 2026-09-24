# Track Map overlay

## Scope and files

- Entry: `trackmap.html`
- Renderer/style: `src/trackmap.ts`, `src/trackmap.css`
- Rust model: `src-tauri/src/telemetry/track_map_model.rs`
- REST geometry: `src-tauri/src/telemetry/track_geometry.rs`
- Shared-memory coordinates: `src-tauri/src/telemetry/sim/lmu/bridge.cpp`
- OBS route/API: `/trackmap`, `/api/trackmap`
- Cadence: lightweight coordinate roster at approximately 30 Hz in Smooth,
  16.7 Hz in Balanced and 8.3 Hz in Efficiency

Track Map must not request the enriched Standings roster. Rust owns fallback
learning, pit-passage validation/learning and post-stop lap-distance prediction;
TypeScript fetches static geometry, interpolates and renders it.

## Coordinates and geometry

- Prefer matched per-vehicle telemetry coordinates over slower scoring position.
  Normalize LMU lateral position as `world_y = -mPos.z`.
- In a solo session LMU may expose the selected telemetry vehicle without a
  scoring roster; keep the player marker from that selected vehicle's position
  and lap distance instead of publishing an empty map batch, and map its SDK
  class identifier to the same class colour used by roster vehicles.
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
  length. Validity follows the shared latched `mLapInvalidated` signal, not
  `mCountLapFlag`. The circular projection is the final fallback.
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
- Before a complete passage is learned, use official open type-1 path length
  divided by the calibrated pit speed. Length is computed once when decoding
  geometry, never by closing the path or subtracting main-track lap distances.
  Prefer measured median as soon as available; approximations never enter the
  measured sample history. The initial marker is `P~`, returning to `P` when measured.
- LMU does not supply a pit-speed limit in the consumed SDK. Rust calibrates it
  after two seconds in pitlane with the limiter on, throttle at least 95%, brake
  at most 1%, speed within 0.4 m/s and sample gaps at most 0.5 seconds. Braking,
  acceleration, leaving pitlane and clock rollback reset the candidate. Persist
  the result per circuit in the existing learning file; old files remain valid.
  Missing geometry or speed leaves the estimate unavailable; never assume a limit.
- Geometry may be prefetched outside the telemetry thread by Fuel/Timing/Standings
  demand as well as Track Map. Failed requests retry no faster than every ten
  seconds. The fallback needs no complete passage, but a new circuit still needs
  a short stable limiter run before its first speed reference is available.
- Learn moving time and pit-entry lap distance per circuit from complete pit
  passages by any visible vehicle. Exclude stationary service time and keep the
  last seven valid observations. Fuel's stint targets reuse the learned entry to
  apply TinyPedal's finish-line-to-pit-entry bias while another stop remains.
- With official type-1 geometry, accept only endpoint-to-endpoint progress as a
  measured passage. Never seed measured samples from partial passages. Latch the entry and exit endpoints throughout the observed
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
- Every performance profile retains the complete roster; only delivery/rendering
  cadence and the Efficiency halo behavior change.
- The player keeps its class-position label in a 28 px circle and has a larger
  lime halo advanced by telemetry events. It pulses in Smooth and Balanced and is
  static in Efficiency. Never replace its label with `P`.
- The player alone may override its class colour or use a custom image. The image
  is clipped inside the 28 px disc; its position label stays above it, while the
  existing halo and leader star retain their placement. A null colour restores
  class colour. Settings and imported profiles retain only a validated bounded
  PNG data URL; arbitrary image strings are never used as CSS.
- When a custom player image is selected, its marker stays above other vehicles
  and the pit prediction when their positions overlap.
- Image selection accepts PNG, JPEG and WebP up to 1 MiB and 256×256 pixels.
  The frontend reads dimensions from the file header before bitmap decoding,
  rejects animated PNG/WebP, then scales proportionally into a 128×128 canvas
  and encodes a static PNG. The normalized PNG is capped at 96 KiB binary / 128
  KiB data URL characters. Its maximum
  decoded RGBA bitmap is 64 KiB; the encoded setting adds at most 96 KiB, plus
  small object/string overhead. GIF, SVG and animated formats are excluded.
- Only the overall leader receives a static gold star. Keep the complete leader
  marker above normal vehicles and distinct from the player's halo; class leaders
  have no special mark.
- A vehicle inferred by Rust as causing a yellow keeps its normal class marker and
  receives a yellow halo driven by telemetry events. It pulses in Smooth and
  Balanced and remains static in Efficiency like the player's halo.
- Official yellow-sector flags paint the corresponding circuit segment yellow once
  Rust has observed and learned the two scoring-sector boundaries. The normal track
  remains visible until aligned geometry and both boundaries are available. Only
  the SDK's exact local-yellow value (`mSectorFlag == 1`) activates a segment.
- In qualifying only, each sector the player closes repaints its segment with the
  same result Timing paints in its cells: purple for the best time of the player's class, green for
  a personal best against the reference chosen in Timing, and nothing otherwise.
  The decision is made once in `delta_records` and travels on the frame, so the
  two overlays cannot disagree and the map does not need the timing panel open.
  The map latches the last painted result of each sector, because the shared
  state returns to `pending` between laps and a segment waiting for its next
  visit should not blink back to the plain track. Any other session clears the
  three segments: a lap is the point of a qualifying session, while in practice
  or a race the map is read for traffic and flags. An invalidated sector clears
  its colour, and a yellow always outranks both on the same segment.

## Verification focus

Validate type-0/type-1 geometry, alignment, fallback lap completion, persistence,
partial/full pit passages, service-time composition, marker stacking and GPU idle
behavior on at least one additional circuit.
Cover calibrated-speed fallback, missing inputs, acceleration rejection, open-path
length and replacement by measured time without contaminating measured samples.

## Localization

The learning state, pit prediction tooltip, document title and accessibility text
use the bundled locale. Circuit geometry, category IDs and marker calculations
remain language-neutral.
Its OBS route follows the shared saved locale or a page-local `?lang=` override.
