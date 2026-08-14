# Detailed Damage overlay

## Scope and files

- Entry: `damage.html`
- Renderer/style: `src/damage.ts`, `src/damage.css`
- REST/shared-memory aggregation: `src-tauri/src/telemetry/lmu.rs`,
  `src-tauri/src/telemetry/lmu_rest.rs`
- OBS route: `/damage`
- Cadence: 20 Hz

Detailed Damage is a compact percentage summary. Localized chassis, suspension,
brake and tyre SVGs belong only to Damage + Tyres (`tires.md`).

## Data semantics

- `Aero`: REST `wearables.body.aero` from RepairAndRefuel.
- `Susp`: maximum of the four REST suspension wearables, with 100% for a detached
  wheel.
- `Body`: shared-memory `sum(mDentSeverity) / 16`.
- `Neum.`: wear of the least healthy tyre, `100 - minimum remaining tread`.
- Keep categories authoritative and independent. Missing REST or tyre data is
  unavailable; never infer one category from another.
- Tyre wear uses its own scale: normal below 30%, warning from 30%, heavy from 50%
  and critical from 75%.

## Presentation

- Use the same restrained 2 px lime left-edge accent as Trailing + Pedal. Keep it
  visible in click-through mode as the product mark without changing panel width.
- Show percentages only.
- Keep label and value columns narrow and fixed. Reserve only enough value width
  for `100%`, align it right and leave no padding after the values.
- Preserve the intentionally compact square width and lower host minimum.

## Verification focus

Check independent missing-data states, detached-wheel suspension override, least-
healthy tyre selection, threshold boundaries and fitted compact width.
