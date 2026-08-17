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

- Preserve unrelated user changes; the working tree may be dirty.
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
- Keep one transparent host WebView per monitor. Configure both the native window
  and WebView with transparent RGBA backgrounds.
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

## GitFlow

Use `master` and `develop` as the two permanent branches:

- `master` contains only the latest production-ready version. Do not develop or
  commit routine changes directly on it.
- `develop` contains the latest integrated development state. Do not implement a
  feature directly on it; use a support branch and merge the completed work back.

Before starting work, inspect the current branch and working tree. Preserve
unrelated changes, and never move them into a new branch, commit or merge without
the user's authorization. Use lowercase, short, descriptive branch names with
hyphen-separated words.

### Approval gate for feature and hotfix branches

- The user's initial request authorizes creating and working on the corresponding
  `feature/*` or `hotfix/*` branch, but it does not authorize merging or deleting
  that branch.
- After implementation, documentation, verification and commits are complete,
  keep the support branch open and report its name, commits and verification
  results to the user.
- Do not merge a `feature/*` branch into `develop`, or a `hotfix/*` branch into
  `master` or `develop`, until the user gives explicit approval after reviewing
  that completion report. Do not treat silence or the original task request as
  approval.
- If the user requests changes instead of approving, continue on the same support
  branch, commit the corrections and present a new completion report for approval.
- Delete a merged `feature/*` or `hotfix/*` branch only with separate explicit
  user approval. Until then, leave it available even after its merges complete.

### Feature branches

- Create `feature/<name>` from an up-to-date `develop` branch.
- Keep the branch limited to one feature, fix, documentation change or other
  coherent unit of work.
- Complete the implementation, documentation and proportional verification on
  the feature branch and commit the finished logical change there.
- Once the approval gate is satisfied, merge the completed branch into `develop`.
  Never merge a feature branch directly into `master`.

### Release branches

- Create `release/<version>` from `develop` when the integrated state is ready to
  become a release.
- Restrict the branch to version synchronization, release notes, packaging and
  release-blocking fixes; do not add unrelated features.
- Keep the version in `package.json`, `src-tauri/Cargo.toml` and
  `src-tauri/tauri.conf.json` synchronized and run the full release verification.
- Merge the completed release into both `master` and `develop`, then tag the
  release commit on `master` as `v<version>`.
- Build a Windows installer or other release artifact only when the user has
  explicitly requested it.

### Hotfix branches

- Create `hotfix/<name>` from `master` only for an urgent production correction.
- Make the smallest complete change, update the patch version and release notes
  when applicable, and run verification proportional to the affected behavior.
- Once the approval gate is satisfied, merge the completed hotfix into both
  `master` and `develop`, resolving the latter carefully if an active release
  already contains related changes.
- Tag a published hotfix on `master` as `v<version>`.

Use explicit, non-interactive Git commands. Before each merge, confirm that the
target branch is correct and the working tree contains no uncommitted changes
belonging to the operation. Do not rewrite published history, force-push, or
delete an unmerged support branch unless the user explicitly requests it.

## Completion and commits

- A change is complete only after its implementation, proportional verification
  and owning documentation are all up to date.
- Commit every completed logical change before handing it back to the user. Use a
  concise message that describes the finished behavior or documentation change.
- Stage and commit only files belonging to that change. Never sweep unrelated
  modifications from a dirty working tree into the commit.
- If a required verification cannot run or the change cannot be committed, report
  that explicitly instead of presenting the work as fully complete.
