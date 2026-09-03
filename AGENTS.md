# BlackRack Overlay agent guide

## Project

BlackRack Overlay is a Windows-first desktop telemetry overlay for sim racing.
The current release line is `0.5.x`. It uses Tauri 2, Rust, TypeScript, Vite and
plain HTML/CSS without a frontend framework. Le Mans Ultimate is read through its
official shared-memory SDK and is complete; iRacing is read through its own
memory-mapped interface and is being landed one overlay at a time. The active
simulator is chosen at runtime, and a build with neither uses the mock source.
See `docs/SIMULATORS.md`.

## Reading workflow

Before changing code, read:

1. `docs/PROJECT_CONTEXT.md`
2. `docs/DECISIONS.md`
3. `docs/TODO.md`
4. The relevant file from `docs/overlays/README.md` when an overlay is involved.

Read a cross-cutting document only when the task needs it:

- Backend, data flow, persistence or windows: `docs/ARCHITECTURE.md`
- Shared telemetry, REST or RaceOS infrastructure: `docs/TELEMETRY.md`
- The telemetry source contract or adding a simulator: `docs/SIMULATORS.md`
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

## Context and concurrency efficiency

- Use a dedicated Git worktree for every concurrent implementation task. Keep the
  local checkout for one foreground task, integration or read-only coordination;
  do not start parallel writers in the same checkout when a worktree is available.
- Keep command output proportional to the question. Start with `rg -n`, scoped
  paths/globs, `git status --short`, `git diff --stat` or `git diff --name-only`;
  inspect detailed content only for the relevant matches and task-owned files.
- Do not concatenate several large files, repository-wide searches or full diffs
  into one command. Read focused ranges, normally no more than 100-200 lines at a
  time, and narrow the next query when an output limit is reached.
- Read each required authoritative document once per task. Do not reread unchanged
  files or repeat searches whose result is already present in the current context.
- Never print the complete diff of a dirty shared worktree merely to identify task
  changes. Compare only the explicit task paths and use compact summaries first.
- Use modest tool-output limits. If output is truncated, rerun a narrower command
  instead of requesting the same broad output with a larger limit.
- Batch related edits before verification. Do not repeat a successful build or test
  without an intervening relevant change; preserve full failure diagnostics, but
  keep successful and repetitive output concise when the tool permits it.
- A lock failure, changed `HEAD` or overlapping task path is a concurrency signal,
  not a reason for repeated broad status/diff/retry loops. Re-evaluate ownership,
  move the task to a worktree when possible, or use the bounded fallback below.

## Common engineering rules

- Preserve unrelated user changes.
- Use UTF-8 for text files and keep the frontend framework-free.
- Keep overlay-specific CSS in its own file. Use `src/fonts.css` and
  `src/styles.css` only for genuinely shared rules.
- Native composite overlays must repaint only from bounded telemetry/UI updates.
  Do not add continuous CSS animations, long transitions or backdrop filters
  that make WebView2 present at the monitor refresh rate. Preserve the shared
  `composite-embed` motion/blur safeguard; use cadence-driven state changes for
  native warnings and keep decorative motion limited to standalone/OBS pages.
- Keep `src/telemetry-types.ts` synchronized with serialized Rust frame fields.
- Keep every simulator-specific detail inside `telemetry/sim/<id>/`. The
  frame, the loop, the domain models and the frontend name no simulator; they
  read the source contract and the capability set instead. Visible copy takes
  the simulator as a `{simulator}` parameter.
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
  copied. Use `https://deepwiki.com/s-victor/TinyPedal` as an additional guide
  to its architecture and data flow; because its index may lag behind, treat the
  local TinyPedal source as authoritative. Dox and Go Fast are visual or
  behavioral references only.
- Keep bundled logos, flags, badges and Roboto Condensed available in production
  and OBS browser pages.

## Commands

Run from the repository root on Windows and prefer `npm.cmd`:

```powershell
npm.cmd install
npm.cmd run dev
npm.cmd run tauri:dev
npm.cmd run build
cargo fmt --manifest-path src-tauri\Cargo.toml
cargo test --manifest-path src-tauri\Cargo.toml --lib
npm.cmd run tauri build
```

`npm.cmd run tauri:dev` runs the app with the simulators that are still being
filled in; plain `npm.cmd run tauri dev` and `npm.cmd run tauri build` leave
them out, so anything handed to a tester never offers one. See
`docs/SIMULATORS.md`.

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
- Concurrent implementation sessions should use separate worktrees. If a task is
  already running in a shared checkout, unrelated staged changes are not a reason
  to wait: create the task commit with a temporary alternate `GIT_INDEX_FILE` and
  stage only explicit task paths there. Never use interactive `git add -p`, print a
  repository-wide diff or alter unrelated real-index entries. Recheck `HEAD` before
  committing; if it changed, rebuild the temporary index once from the new `HEAD`.
  If it changes again or task paths overlap, stop committing and report the
  collision instead of entering another repair loop. After a successful commit,
  synchronize only the committed task paths into the real index.
- After completing and verifying a change, create the Git commit directly with a
  concise message that summarizes only the work completed in the current task.
  Do not include copy-ready commit messages in the handoff. Preserve unrelated
  user changes and never include them in the commit.

## Verbosidad

- Sé extremadamente conciso. Respuestas de 1-2 líneas salvo que se pida detalle.
- Resumenes muy breves.
- Solo da explicaciones cuando el usuario las pida explícitamente.
