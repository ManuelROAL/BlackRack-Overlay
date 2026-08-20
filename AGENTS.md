# LMUOverlay agent guide

## Project

LMUOverlay is a Windows-first desktop telemetry overlay for Le Mans Ultimate.
The current release line is `0.5.x`. It uses Tauri 2, Rust, TypeScript, Vite and
plain HTML/CSS without a frontend framework. Live telemetry uses the official LMU
shared-memory SDK on Windows; builds without the SDK use the mock source.

## Reading workflow

Before changing code, read:

1. `docs/PROJECT_CONTEXT.md`
2. `docs/DECISIONS.md`
3. `docs/TODO.md`
4. The relevant file from `docs/overlays/README.md` when an overlay is involved.

Read a cross-cutting document only when the task needs it:

- Backend, data flow, persistence or windows: `docs/ARCHITECTURE.md`
- Shared telemetry, REST or RaceOS infrastructure: `docs/TELEMETRY.md`
- Shared overlay host, control panel or visual behavior: `docs/OVERLAYS.md`
- Profiling or optimization: `docs/PERFORMANCE.md`

The implementation and tests are the final source of truth when documentation is
stale. Update the owning overlay document whenever its behavior changes. Update a
central document only for a genuinely shared contract; do not copy an overlay rule
back into this file.

When creating a new overlay, create `docs/overlays/<overlay-id>.md` in the same
change and add it to `docs/overlays/README.md`. That file becomes the authoritative
record for the overlay's files, sources, cadence, behavior, invariants and focused
verification. Keep adding the rules and decisions that arise in the overlay's
dedicated task/chat to that file instead of growing `AGENTS.md` or a central
document.

## Common engineering rules

- Preserve unrelated user changes.
- Use UTF-8 for text files and keep the frontend framework-free.
- Keep overlay-specific CSS in its own file. Use `src/fonts.css` and
  `src/styles.css` only for genuinely shared rules.
- Keep `src/telemetry-types.ts` synchronized with serialized Rust frame fields.
- Keep domain calculations and roster selection in Rust. TypeScript owns
  presentation and browser-only state, not duplicated telemetry semantics.
- Prefer cached DOM nodes and changed-value updates in hot render paths. Do not
  move heavy work into the 50 Hz telemetry loop without measurements.
- Preserve grouped `telemetry://batch` delivery. Composite hosts forward only
  locally mounted targets and project reused overlay field allowlists through
  same-origin `postMessage`.
- Keep one transparent host WebView on the selected overlay monitor. Configure
  both the native window and WebView with transparent RGBA backgrounds.
- Start in click-through game mode. Embedded overlay documents remain inert;
  edit-mode input is reserved for panel movement and proportional resize.
- Do not set `color-scheme: dark` on an embedded overlay document root.
- Preserve independent general/per-overlay transparency and monitor values.
- Keep reset actions scoped and use the in-panel confirmation dialog.
- Import/export one validated, versioned configuration document. Exclude learned
  telemetry, session data, logs and credentials.
- Keep `dist/` embedded only through Tauri `frontendDist`. Keep every OBS route
  and mirrored browser preference synchronized with its overlay.
- Production JavaScript keeps stable serialized, DOM and settings property names,
  emits no source maps/comments, and disables DevTools. Rust release builds keep
  fat LTO, one codegen unit and symbol stripping.
- Keep the MSVC static-VCRuntime placeholder repair in `build.rs` while the Tauri
  2.6.x/MSVC 14.44 combination requires it.
- Never log or persist authentication tickets or access tokens. Do not add private
  server keys. Optional LMU REST and RaceOS failures must not stop shared-memory
  telemetry.
- TinyPedal may be used as a behavioral reference but GPL source must not be
  copied. Dox and Go Fast are visual or behavioral references only.
- Keep bundled logos, flags, badges and Roboto Condensed available in production
  and OBS browser pages.

## Commands

Run from the repository root on Windows and prefer `npm.cmd`:

```powershell
npm.cmd install
npm.cmd run dev
npm.cmd run tauri dev
npm.cmd run build
cargo fmt --manifest-path src-tauri\Cargo.toml
cargo test --manifest-path src-tauri\Cargo.toml --lib
npm.cmd run tauri build
```

`npm.cmd run tauri build` creates the Windows installer. Do not run it unless the
user explicitly requests a release/build artifact or a production performance
capture requires it.

## Verification

For Rust/backend changes:

1. `cargo fmt --manifest-path src-tauri\Cargo.toml`
2. `cargo test --manifest-path src-tauri\Cargo.toml --lib`
3. `npm.cmd run build` when serialized fields or frontend behavior changed

For frontend-only changes, run `npm.cmd run build`. For performance work, also
collect a comparable race/replay sample as described in `docs/PERFORMANCE.md`;
successful compilation alone is not performance evidence.

## Completion

- A change is complete only after its implementation, proportional verification
  and owning documentation are all up to date.
- If a required verification cannot run, report that explicitly instead of
  presenting the work as fully complete.
