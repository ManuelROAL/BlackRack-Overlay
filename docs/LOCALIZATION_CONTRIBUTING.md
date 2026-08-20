# Localization contributor guide

## Source of truth

- `src/i18n/catalogs.ts` contains bundled messages and plural forms.
- `src/i18n/index.ts` owns supported locale metadata, persistence and formatters.
- Static HTML uses `data-i18n`, `data-i18n-title`,
  `data-i18n-aria-label` or `data-i18n-placeholder`.
- Dynamic presentation calls typed `t(...)`; stable IDs, telemetry enums and
  persistence keys are never translated.
- `browser.html` is the localized OBS route index. Rust only serves the built
  asset and injects preferences, so adding a locale requires no backend copy.

## Terminology

- Keep product and domain names such as LMU, BlackRack Overlay, Relative, Delta, DR,
  SR, NRG, PIT and OUT unchanged unless a product decision explicitly renames
  them.
- English uses British motorsport spelling (`tyre`, not `tire`) in visible copy.
- Spanish compact lap notation uses `V`; English uses `L`. Lap times and deltas
  retain motorsport formatting.
- Translate interface meanings, not serialized values. Circuit, vehicle, team,
  driver, manufacturer and externally supplied text remain source data.
- Prefer short labels that fit the documented minimum overlay dimensions. Record
  an intentional abbreviation in the owning overlay document.

## Adding a locale

1. Add the locale code to `SUPPORTED_LOCALES` and its autonym to
   `LOCALE_OPTIONS` in `src/i18n/index.ts`.
2. Add a catalog object in `src/i18n/catalogs.ts` that satisfies `Catalog`, then
   register it in the exported `catalogs` object. It may inherit the Spanish
   baseline, but every language-sensitive message must be overridden; inherit
   only genuinely neutral values such as product names or units.
3. Preserve every placeholder exactly. If Spanish contains `{count}` or
   `{overlay}`, every form in the new locale must contain the same parameter.
   Plural messages define both `one` and `other` until the runtime explicitly
   supports additional categories.
4. Do not add the locale to Rust. Browser-source preferences receive
   `SUPPORTED_LOCALES`, and OBS routes resolve the same frontend catalog.
5. Run `npm.cmd run check:i18n`. It checks resolved key equality, duplicate and
   unknown keys, message shape, parameter compatibility, supported-locale
   metadata, all HTML bindings, typed calls and the visible-copy audit.

## Visible-copy audit

`tools/audit-visible-copy.mjs` examines likely HTML and TypeScript UI sinks.
Unexpected literals fail the build. Move normal prose into the catalog. Add an
entry to `tools/i18n-visible-copy-allowlist.json` only for deliberate technical or
user data, using the exact fingerprint reported by the audit and a durable reason.
The allowlist also fails when an entry becomes stale.

English messages that deliberately inherit an identical Spanish value are listed
separately in `tools/i18n-inherited-message-allowlist.json`. This keeps neutral
product names and abbreviations explicit and prevents an untranslated Spanish
message from silently falling through the `...es` baseline.

Developer diagnostics, logs, raw third-party errors and source data are outside
the translation surface, but they must not be rendered directly as ordinary UI
copy. Wrap user-visible failures in a localized message or return a stable kind
from Rust.

## Visual verification matrix

Check Spanish and English before accepting a locale change:

1. Control panel at 1280×720 and a compact 1024×720 viewport; visit Overlays,
   General and Integrations, including expanded settings and dialogs.
2. Every overlay at its documented minimum/design size with representative mock
   or preview data. Check values, tooltips, empty states and accessibility labels.
3. Long-content configurations for Standings and Relative, and 0/3/5-row Timing.
4. OBS index and representative routes with the saved locale, `?lang=es` and
   `?lang=en`. An override must not change another page's language.
5. Windows display scaling at 100%, 125% and 150% in a Tauri build before a
   release. Browser viewport checks do not replace this native WebView2 pass.

Record any intentional compact abbreviation in `docs/overlays/<id>.md`. Do not
increase a panel's minimum size without checking existing saved geometry.
