# Localization plan

## Goal and first scope

Make the control panel, composite edit chrome and every overlay usable in more
than one language without changing telemetry semantics, persisted option IDs or
the framework-free frontend. The first delivery should support Spanish (`es`)
and English (`en`); adding a later language should only require a complete
catalog and visual verification.

The locale applies to the control panel, native-hosted overlays and OBS browser
pages. A fresh installation starts in English and persists any explicit choice.
The General view must always expose an explicit language selector.

## Boundaries

- Keep stable domain and persistence values such as overlay IDs, `DeltaMode`,
  column IDs and JSON property names untranslated. Only their presentation labels
  are localized.
- Keep telemetry calculations and roster selection in Rust. Do not send prose in
  telemetry frames when a semantic enum, boolean or number can describe the same
  state.
- Treat log messages, developer diagnostics, track/car/driver names and raw
  external-service text as data, not catalog entries. A frontend may wrap a raw
  backend error in a localized explanation without translating the diagnostic.
- Localize visible text, document titles, dialog titles, tooltips, empty states,
  accessibility labels and dynamic status messages.
- Use locale-aware formatting for the real-world clock and ordinary decimal
  quantities. Preserve established motorsport notation for lap times, deltas,
  units and compact telemetry values where changing separators would reduce
  familiarity or width stability.
- Do not add a frontend framework or a runtime translation service. All catalogs
  must be bundled in `frontendDist` and work offline.

## Proposed frontend design

Add a small `src/i18n/` module with:

- a `Locale` union and supported-locale metadata;
- complete bundled `es` and `en` catalogs keyed by semantic names;
- a typed `t(key, parameters)` function with interpolation and plural selection;
- locale resolution, persistence and `document.documentElement.lang` updates;
- DOM helpers for static `data-i18n`, `data-i18n-title` and
  `data-i18n-aria-label` attributes;
- shared number, clock and plural formatters constructed once per locale.

Catalog keys should describe meaning, not the current Spanish copy; for example
`control.overlays.showAll`, `overlay.relative.waitingForPlayer` and
`status.telemetry.active`. Settings definitions should expose `labelKey` rather
than a translated `label`, while stable IDs and defaults remain unchanged.

Changing the language may reload the control panel and mounted overlay documents
in the first implementation. This keeps static HTML, dynamic text and iframe
state consistent without adding work to the 50 Hz render paths. Translation
lookups needed during telemetry rendering should be resolved or cached when the
locale/document loads, not repeated unnecessarily on every frame.

## Persistence and propagation

- Persist the selected locale under `lmu-overlay.locale.v1` as a supported
  language code, not as a translated name.
- Add the locale to the versioned configuration document as `ui.locale` and bump
  its schema version. Older imports resolve the local default; invalid or
  unsupported locale values fall back safely.
- Include the locale in `set_browser_source_preferences`. The generated
  `/browser-source-settings.js` must initialize the same locale key before an OBS
  overlay module renders.
- Allow an OBS URL query such as `?lang=en` to override the injected default for
  that page without rewriting the saved application preference. This permits two
  scenes to use different languages.
- Make the composite host translate its edit labels and ensure mounted iframes
  observe the resolved locale after a language change.

No locale field belongs in `TelemetryFrame`, and changing language must not split
or enlarge grouped telemetry payloads.

## Delivery phases

### Current rollout state

Phases 1, 2 and 3 are implemented. `src/i18n/` owns the bundled Spanish and English
catalogs, typed lookup/interpolation/plural helpers, cached locale formatters,
DOM attribute translation and the persisted `lmu-overlay.locale.v1` choice. The
complete control panel, including generated settings, dialogs, accessibility
labels and native file-dialog titles, uses that catalog. Configuration schema 7
round-trips the choice as `ui.locale` and imports schemas 1–6 using the current
local choice. Fresh profiles default to English. Every native overlay translates
its static HTML, dynamic status, tooltip and accessibility text; ordinary numbers
and the real-world clock follow the selected locale. The composite edit chrome
and its mounted documents reload together after a language change.

The control panel mirrors the selected locale into the optional browser-source
preferences before an OBS page loads. Each route accepts `?lang=es` or `?lang=en`
as a page-local override that does not rewrite the shared saved preference, and
the localized browser-source index preserves its resolved language in every link.
Known browser-server failures cross the Rust boundary as stable error kinds;
known shortcut failures use stable codes as well. Localized UI copy is selected
in TypeScript while raw operating-system details remain diagnostic.

`npm.cmd run build` validates catalog keys and bindings across all 14 HTML
documents before TypeScript and Vite. Phase 4 remains: stricter catalog parameter
checks, hard-coded-copy auditing, visual scaling checks and the contributor guide.

### 1. Infrastructure and control panel

1. Inventory visible strings in HTML, TypeScript and the browser-source index.
2. Add the typed catalogs, resolver, persistence and formatting helpers.
3. Add the General language selector and localize the complete control panel,
   including generated settings controls, confirmations and file-dialog titles.
4. Change settings metadata from translated labels to catalog keys.
5. Add automated catalog parity checks and fail on an unknown key.

### 2. Overlay surfaces

1. Migrate static overlay HTML and document titles.
2. Migrate dynamic strings in every renderer, including tooltips, accessibility
   text, warnings, empty states, compound/status labels and plurals.
3. Localize the composite edit chrome and verify that translated text fits at the
   minimum supported panel scale.
4. Update each owning `docs/overlays/<overlay-id>.md` as its surface is migrated.

Overlay migration can be split into reviewable groups: timing/driving, vehicle
condition, race tables, strategy/map and warnings. A phase is not complete while
one of its overlays mixes Spanish and English in normal operation.

### 3. OBS and backend-facing UI

1. Mirror the locale into browser-source preferences and implement the per-route
   query override.
2. Localize the browser-source catalog/index. Keep route IDs and URLs stable.
3. Replace any user-visible Rust prose crossing into the control panel with
   stable status/error kinds where practical; retain raw detail for diagnostics.
4. Verify that optional REST/RaceOS failures and startup diagnostics behave the
   same in both languages.

### 4. Hardening and later languages

1. Add a build-time check for equal catalog keys, parameter compatibility and
   supported locale metadata.
2. Search for remaining hard-coded user-visible prose outside catalogs and keep a
   small documented allowlist for diagnostics and source data.
3. Visually inspect Spanish and English at common Windows scaling values, narrow
   overlay configurations and OBS routes.
4. Document the contributor workflow for adding a locale, including terminology,
   catalog validation and screenshot checks.

## Verification and acceptance

- `npm.cmd run build` succeeds and the catalog validation runs as part of it.
- A clean profile resolves and persists the expected locale; an existing profile
  remains usable and can switch languages without losing settings or geometry.
- Export/import round-trips the locale and still accepts every older supported
  configuration schema.
- Every control, dialog, tooltip, empty state and accessibility label is available
  in Spanish and English; no raw catalog key is rendered.
- All native overlays and each documented OBS route use the chosen locale. An OBS
  query override affects only that page.
- Locale changes do not alter telemetry types, grouped batch sizes or scheduler
  cadence, and do not add per-frame catalog construction.
- Text remains legible without clipping at the normal and minimum practical panel
  sizes. English is expected to expose most width regressions, but both locales
  must be checked.

## Explicit non-goals for the first delivery

- Translating circuit, vehicle, team or driver names supplied by LMU.
- Translating diagnostic logs or third-party error bodies.
- Downloading community translations at runtime.
- Right-to-left layout support. Catalog and locale APIs must not prevent it, but
  RTL layout requires a separate designed and verified milestone.
