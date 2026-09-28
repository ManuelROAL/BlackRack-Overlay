# Minimap overlay

## Scope and files

- Entry: `minimap.html`
- Renderer/style: `src/minimap.ts`, `src/minimap.css`
- Shared with Track Map: `src/track-map-common.ts` (geometry fetch, class colours
  and class positions) and the Track Map settings for the player's marker.
- OBS route: `/minimap`; geometry comes from the Track Map's `/api/trackmap`.
- Cadence: the Track Map's lightweight coordinate roster, approximately 30 Hz in
  Smooth, 16.7 Hz in Balanced and 8.3 Hz in Efficiency.

The Minimap is a second view of the Track Map data, not a second model. A visible
Minimap or an OBS client on its route requests the same `track_map_vehicles`
roster and `track_map_model`, so Rust keeps learning geometry and pit passages
while only the Minimap is open. It never requests the enriched Standings roster
or the REST supplement.

## View

- A 260 px disc with the player fixed at its centre. It shows 190 m of circuit
  between the centre and the rim.
- The disc turns so the direction of travel points up. The frame carries no yaw,
  so the heading comes from the player's own movement: a new direction needs at
  least 2 m of travel, a jump of more than 80 m between frames (teleport, return
  to the garage) re-anchors without turning, and the rotation eases towards the
  target with a 120 ms time constant at telemetry cadence. A stationary car keeps
  its last heading. Reversing turns the view around.
- The circuit is the official type-0 centreline or the learned lap, drawn at world
  scale so stroke widths are in metres. The official type-1 pitlane is drawn
  beneath it as an open line; learned maps never invent one.
- Without geometry only the cars are drawn and the status reads *learning
  circuit*. Without a player position (garage, no car) the map is hidden and the
  status reads *waiting for car*.

## Markers

- Every car with a world position is placed around the player and hidden once it
  is more than 12 px past the rim; cars without a world position and cars in the
  garage are not drawn.
- Markers are 20 px class-coloured discs with the class position; the player is a
  26 px disc with a static lime ring. The car Rust infers as causing a yellow has a
  static yellow ring, and pit cars are dimmed.
- The player's custom colour and image come from the Track Map settings and follow
  its `trackmap://settings` event, so both maps always show the same marker.
- Like the Track Map, markers are cached HTML elements moved with transforms. No
  CSS transition, animation loop or SVG filter is used: every change happens on a
  telemetry event, which keeps the WebView2 GPU process idle between frames.

## Not shown

Sector colours, yellow-sector segments, the start line, the race-leader star and
the pit-exit prediction stay on the Track Map. All of them need the lap-distance
calibration of the full map, and the Minimap is read for nearby traffic.

## Localization

The document title, accessibility label and both status messages use the bundled
locale. Its OBS route follows the shared saved locale or a page-local `?lang=`
override.
