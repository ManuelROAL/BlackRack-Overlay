# Overlay behavior and design

## Shared design rules

- The control panel separates overlay management, general display/shortcut
  settings and integrations into persistent top-level views. Overlay management
  provides text search and race/car/alert filters. Per-overlay transparency,
  monitor and reset controls stay collapsed until that overlay's settings are
  opened, while visibility remains directly accessible.
- Use bundled Roboto Condensed throughout overlays and the control panel. Users must
  not need the font installed system-wide.
- Optimize for legibility while driving: important values are larger than labels,
  vertical padding stays compact and columns have deliberate spacing.
- Resizing scales the complete overlay so content remains visible; it must not
  crop rows or introduce scrolling.
- Background transparency can use one general value or independent values for
  each overlay. Individual values remain stored while general mode is active.
- In edit mode, the per-monitor host shows a border, label and resize handle for
  each panel. The control panel remains above the full-monitor host.
- Embedded panel content is inert: it receives no clicks, focus, selection,
  native dragging or context menu. In edit mode pointer input only moves the
  panel or resizes it through its handle.
- In game mode, each complete monitor host ignores cursor events so the game
  remains clickable.
- The application starts in game mode by default. Switching to edit mode is an
  explicit action for the current run.
- Each overlay can be assigned to a detected monitor from the control panel. Its
  position and size are stored relative to that monitor.
- Monitor assignment can use one general monitor or independent assignments. The
  previous individual assignments are restored when returning from general mode.
- Every overlay card provides independent reset actions. Configuration reset
  restores its default transparency and any overlay-specific content options;
  position reset restores default coordinates and size on the currently assigned
  monitor. Neither action changes visibility or affects another overlay. Reset
  confirmation uses an in-panel dialog rather than a browser-origin prompt.
- Panels may cross any monitor edge to use the screen boundary as a deliberate
  crop. Movement retains a 32 px visible strip, allowing the panel to be dragged
  back in edit mode instead of becoming permanently unreachable.
- Embedded overlay documents must leave the root color scheme unset. Chromium
  paints the unused iframe canvas when `color-scheme: dark` is applied to the
  root, which exposes an opaque rectangle while resizing to a different aspect
  ratio.
- Overlays hide automatically when LMU is not foreground, the player is inactive,
  the game is not realtime, the player is in the garage or the session is over.
  Do not capture Escape to implement this; LMU state is authoritative.

## Standings

The standings is multiclass and has no scrollbar. The user can configure:

- Which optional columns are visible.
- The order of every data column, including mandatory identity columns. The
  transparent signals/status column is fixed at the far right.
- Number of rows for the player's class (3-30, default 10).
- Number of rows for every other class (1-15, default 3).
- Whether other classes are shown.

For the player's class, always select the first three plus the closest cars around
the player until the configured count is reached. This naturally takes more cars
from above when too few remain below, and vice versa. Other classes show their
leading configured rows.

Current columns, in source order, are position, assigned car number, manufacturer,
driver badge, driver name, DR/SR, gap, interval, best, last, average five, energy,
damage, track-limit steps, pit stops/time, tyre and signals. Position and driver name are mandatory;
the remainder are configurable according to `src/standings-settings.ts`.

Additional rules:

- Driver name is shortened to first name plus one surname.
- Use the session-assigned car number, not the team/model number.
- Manufacturer and nationality prefer the bundled LMU SVG assets, retain PNG
  fallbacks for unsupported entries and use text when no image is available.
- A uniform tyre set uses the bundled LMU compound SVG; mixed sets retain four
  compact colored circles. Vehicle counts and timing use the bundled helmet and
  stopwatch icons at fixed overlay sizes.
- Category headers use LMU-compatible colors, show current/initial car counts and
  expose DNF/DQ through the difference. In practice sessions they show only the
  current car count: initial counts are neither retained nor displayed. A separate
  compact general header can show the unabbreviated session type, event split as
  current/total,
  remaining race time and current lap followed by the estimated total session
  laps in one `current/~total` reading. It can also show the same
  environmental and player data as Relative: air and track temperature, brake
  bias, track-limit steps/threshold and local time. The complete header and each
  value can be enabled independently. Session type, split, remaining time and lap
  information are anchored from the left; environmental and player values are
  grouped against the right edge. Every category row contains the labels for the
  visible data columns.
- Every driver row keeps its category color as a left accent and subtle horizontal
  gradient, matching Relative. Standings groups those rows in compact outlined
  cards with a filled angled category tab and individually separated row surfaces.
  Pit tint and the full lime player highlight remain visible as layers above that
  category treatment.
- Each category header shows SOF and its profile coverage outside practice. SOF
  is not calculated or displayed during practice sessions. Visible categories
  are ordered by performance class (Hypercar/GTP, LMP2, LMP3, LMGT3, other).
- The category leader displays the current lap once in GAP and once in INT.
- User-row highlighting must stop before the external signals column. A lime
  marker closes that background on both sides, with a symmetric inward gradient,
  and a continuous lime tint highlights the full data row so the player remains
  identifiable from either side.
- Signals have no persistent background when empty.
- The `NRG` column shows virtual energy only for regulated Hypercar and LMGT3
  vehicles. Non-virtual-energy categories display `--` for every driver; the
  player's private fuel percentage is never mixed into an opponent comparison.
- The compact optional `TL` column shows each driver's accumulated normalized
  track-limit points (four SDK steps equal one displayed game point) and displays
  `--` while that vehicle's telemetry is unavailable.
  Its cell background is white below 60% of the session penalty threshold,
  yellow from 60% and red from 80%. Standings and Relative header counters use
  the same thresholds on their text color instead of their background.
- Supported states include yellow cause, pit, garage, stop/go and other available
  penalties. Blue, fastest-lap and time-penalty badges were intentionally removed
  from the signals column; fastest-lap color belongs to timing cells.

Rendering uses cached row/header nodes and replaces only cells whose signatures
changed. Preserve this approach when adding columns.
The fitted Standings surface grows from its 450 px design minimum when the
configured category headers and 21 px rows need more height. Large layouts are
therefore scaled as a complete table instead of clipping the final rows.
The Standings shell override intentionally uses greater specificity than the
shared `.overlay-shell` rule. Vite extracts shared CSS after entry-specific CSS
in production, so equal-specificity overrides would restore the full-window
background even though development appears transparent.

## Fuel / energy

The overlay is an endurance strategy tool, not only a simple autonomy display.
It shows actual resource, race projection, pit window/load and four consumption
scenarios: estimated, average, qualifying and last lap. Hybrid cars show separate
energy scenarios plus a compact fuel strategy card, with the global pit plan
covering both resources. All text uses a consistent compact size hierarchy matching
standings.

The PIT indicator is neutral above three laps of autonomy and changes color as
the stop approaches. A separate full-power visual is used when the automatic
target is capped by qualifying consumption.

No configurable reserve is subtracted from the displayed strategy.

## Relative

The Relative overlay has its own `src/relative.ts` renderer and
`src/relative.css` stylesheet. It follows the same visual language as Standings
without sharing its hot render path or overlay-specific CSS, so changes to one
table do not alter the other. Its selection difference is that race position
does not determine which rows appear: it shows the nearest cars physically ahead,
the player and the nearest cars physically behind, including lapped traffic and
other classes. The number of rows in each direction is configurable from 1 to 10
(default 4), and the Standings columns can be configured independently for the
Relative window. Because categories share one physical-order table, every row
uses its official class color (Hypercar red, LMP2 blue, LMP3 purple and LMGT3
green) as an accent and layered gradient. The mixed physical-order roster uses
the same outlined card, separated row surfaces and compact framed-header language
as Standings, without introducing category grouping. Position, driver and physical
relative time are always visible; the relative-time cell is deliberately larger
and heavier because it is the primary datum in every row. Its configurable layout
includes car number, country flag,
license, DR/SR with its estimated change, position change, lap number,
average/last/best lap, virtual energy, damage, per-driver track-limit steps, pit
information, tyre compound and flags. Relative does not render a column-label
header row. The complete layout scales without scrolling. Its design width is
derived from the active columns (with a compact 760 px minimum), so enabling all
columns scales the full table rather than clipping the right side. Tire icons and
mixed-compound colors match Standings exactly.
The control panel separates Relative row counts, header data and columns in the
same way as Standings. Shared options use the Standings labels (`Dorsal`,
`Insignia`, `DR / SR`, `Media 5`, `Energía`, `Daño`, `Cortes de circuito`,
`Paradas / tiempo`, `Neumático` and `Banderas / estados`) while their visibility
remains independent per overlay. Estimated DR change is part of the `DR / SR`
cell and does not have a separate visibility option. Every Relative column can
also be reordered independently from Standings, including its mandatory columns,
except `Banderas / estados`, which remains fixed at the far right because its
surface is transparent.
Relative renders up to the configured number of rows above and below the player.
As in LMU's own relative, the same car may appear once ahead and once behind when
the track order wraps, but it is never repeated multiple times in one direction;
therefore a driver appears at most twice in total. Cars in the garage are excluded
before selection. Its telemetry and cached DOM renderer update at 20 Hz;
Standings remains at 10 Hz.
Relative reserves 23 px for every selected row when calculating its fitted
surface; this includes the separated-card spacing and prevents the final row from
being clipped.
Its compact header can show air/track temperature with distinct bundled air-flow
and track SVG icons, front brake bias, the player's current track-limit
steps/penalty threshold and the local clock. The complete header and each value
can be enabled independently. Relative has no footer.
Its shell follows the same production-cascade rule as Standings: the transparent
entry-specific override must outrank the shared `.overlay-shell` background.

## Trailing + Pedal

The compact driving overlay combines a short local history of throttle, brake and
clutch with vertical current-input meters, speed, gear, a signed Force Feedback bar
and a rotating steering-wheel indicator with its physical angle in degrees. The
history is rendered in the browser window from the 50 Hz telemetry stream.
It is an independent window and can be enabled beside or instead of the full
dashboard. Yellow points on the throttle trace mark LMU's explicit TC interventions;
white points on the brake trace mark explicit ABS interventions.

## Damage + tyres

The compact overlay separates the vehicle into the same two information levels
used by the telemetry: an eight-zone chassis map and four independent wheel
modules. It follows TinyPedal's dense, immediately localizable layout principle
without copying its drawing or implementation. Every ordered shared-memory
`mDentSeverity` location has its own visible chassis segment; values are never
merged into a broader front, side or rear group. The three lateral zones are
defined once and mirrored exactly across the chassis centerline, preventing the
left and right silhouette from drifting apart while keeping distinct telemetry.

Each wheel module reports remaining tread, carcass temperature, brake-disc
temperature, estimated flat-spot wear (`PL`), its own REST suspension-damage
percentage and the bundled LMU compound SVG used by Standings. Tyre, disc and
suspension shapes remain spatially connected to the correct corner. Tyre and
disc colors follow their independent temperature scales; suspension follows its
damage scale; flat, punctured or detached wheels use a critical state. Missing
REST suspension data is shown as unavailable per wheel rather than inferred from
body damage. It updates at 50 Hz and scales as one surface without clipping. The
independent detailed damage summary remains at 20 Hz.

## Detailed damage

The independent compact damage overlay reports damage percentage rather than
remaining integrity. `Aero` uses LMU's local RepairAndRefuel wearable value,
`Susp` uses the most damaged of the four suspension values (or 100% for a detached
wheel), and `Body` aggregates the eight shared-memory dent severities. `Neum.` is
the wear of the least healthy tyre (`100 - minimum remaining tread`). This panel
contains percentages only; the localized body, suspension, brake and tyre SVGs
belong exclusively to Damage + Tyres. Missing REST or tyre values display as
unavailable instead of being inferred from another damage category. Tyre wear has
its own warning scale: normal below 30%, yellow from 30%, orange from 50% and red
from 75%, so ordinary stint wear is not presented as vehicle damage.
Labels use a compact fixed column and percentages reserve only enough width for
`100%`, aligned right across every row. Do not stretch either column or
reintroduce unused horizontal space after the values.

## Pit-stop estimate

The compact pit-stop overlay shows repairs, fuel or virtual energy, tyres and
driver-swap time. Its resource label follows the player's active strategy resource;
penalties appear as an extra row only when non-zero. `Total` is LMU's official
local REST estimate and is not the sum of the displayed rows, because multiple
services can run concurrently. Missing or stale REST data displays as unavailable.

## Track Map

Track Map draws a transparent square circuit view with a light track surface,
dark outline and start/finish line. Vehicles use the established class colors and
show position in class. Cars in the pits remain visible with reduced opacity.
The overall race leader has a single static gold star above its normal marker.
The player's marker retains its class color and number, grows slightly and uses a
pulsing lime halo; it does not display a `P`.

A separate white `P` marks the player's estimated post-stop position. It combines
the official configured service duration with the learned moving time through the
pitlane, then converts that total into track distance using the latest valid player
lap (best lap as fallback). It stays hidden while the player is already in the
pitlane or while any required estimate is unavailable.

The official circuit centerline is loaded once from LMU when the overlay first
sees a circuit. It is drawn immediately, then aligned to lap distance after a few
metres of non-pit player movement. The official pitlane validates that learned
moving-time samples cover a complete passage. If the REST geometry is unavailable
or invalid, the first complete valid non-pit lap teaches and stores the circuit;
a circular map and `APRENDIENDO CIRCUITO` remain as the final fallback. The
renderer caches SVG nodes and receives target positions at 20 Hz. A short 110 ms
compositor-managed CSS transform transition bridges the normal 50 ms cadence and absorbs an
isolated delayed update without a permanent JavaScript animation loop or
per-marker SVG filters.

## Flags

- Green state contains no car information.
- Yellow and blue show distance, class position and category.
- Yellow includes an arrow for ahead/behind.
- Checkered has priority and contains no distance/car details.
- Warning visibility and causation semantics are documented in `TELEMETRY.md`.

## Rejoin

Armed in the pit lane, below 8 m/s or while all four wheels are on grass, dirt or
gravel, and held for ten seconds after recovery or pit exit. It becomes visible
only for on-track traffic behind within a 15-second physical time gap. The car
with the earliest positive arrival time is preferred; if none is closing, the
smallest physical time gap is used. It displays distance, arrival time, position
and category. Color states are safe, caution and danger.

## OBS browser source

The optional server listens only on `http://127.0.0.1:47636` and exposes:

- `/standings`
- `/relative`
- `/fuel`
- `/pitstop`
- `/flags`
- `/rejoin`
- `/dashboard`
- `/driving`
- `/tires`
- `/damage`
- `/trackmap`

It uses a single SSE endpoint and existing telemetry frames. It is off by default;
when disabled it opens no port and performs no frame serialization. Preferences
for standings, Relative and transparency are injected into browser pages.
