# Codex project instructions

## Project context

BlackRack Overlay is a Windows-first Tauri 2 application with a Rust telemetry
core and a TypeScript/Vite/plain HTML/CSS frontend. It supports Le Mans Ultimate
and a mock source when no simulator is available.

Keep simulator-specific behavior under `src-tauri/src/telemetry/sim/<id>/`.
Rust owns telemetry and domain calculations; TypeScript owns presentation and
browser-only state. Preserve grouped telemetry delivery and the existing overlay
motion/blur safeguards.

Never log or persist authentication tickets or access tokens. Optional REST/RaceOS
failures must not stop shared-memory telemetry. TinyPedal is a behavioral reference;
do not copy GPL source.

## Task-scoped reading

Use the smallest relevant documentation set:

- Read relevant sections of `docs/PROJECT_CONTEXT.md` and `docs/DECISIONS.md` for
  code changes.
- Read the affected file under `docs/overlays/` for an overlay change.
- Read `docs/ARCHITECTURE.md`, `docs/TELEMETRY.md`, `docs/SIMULATORS.md`,
  `docs/OVERLAYS.md`, or `docs/PERFORMANCE.md` only when the task crosses that
  concern.

Update the owning documentation when behavior or a shared contract changes.
Do not perform a full repository tour for a localized change.

## Orchestration

Use the `astra-orchestrator` skill for repository coding work. Work solo by
default. Delegate only a substantial, bounded task when separate context or
parallel work provides a concrete benefit. Use independent review for a concrete
correctness, security, data-integrity, concurrency, or compatibility risk.

The root owns requirements, architecture, routing, integration, and final
verification. Define file ownership before delegation. Subagents must preserve
other contributors' edits, stay within their scope, and must not delegate further
unless explicitly assigned.

## Verification and completion

- Frontend or UI change: `npm.cmd run build`.
- Rust/backend change: `cargo fmt --manifest-path src-tauri\Cargo.toml`, then
  `cargo test --manifest-path src-tauri\Cargo.toml --lib`.
- Run `npm.cmd run build` when Rust serialization or frontend behavior is affected.
- Documentation or instruction-only change: inspect the changed text and run
  `git diff --check`; do not run application builds by default.
- Do not run release packaging unless the user explicitly requests a release
  artifact.
- For UI changes, inspect a local preview or screenshot when visual behavior is
  part of acceptance and the required browser capability is available. Otherwise
  report the visual-validation limitation.
- If a required check fails, classify it as introduced, pre-existing, or
  environment-related; perform one focused diagnostic/retry, then report exact
  evidence and stop if the next step requires user input or broader authority.
- A task is complete only when the requested change is implemented, proportional
  verification is recorded, and material gaps are reported.

## Git and release history

- When a task changes any file, create a Git commit after the requested
  verification passes. Commit only the files belonging to the current task and
  preserve unrelated user changes already present in the worktree.
- Write concise, specific, release-note-ready commit messages. Describe the
  user-visible change and its relevant area or simulator; avoid generic
  messages such as `update`, `fixes` or `miscellaneous`.
- Use the commit history as input when reviewing a new version and drafting its
  release-news text, so each commit should communicate a coherent change.
- Do not push, publish or rewrite history unless the user explicitly requests
  that exact action.

## Authority boundaries

Workspace write access does not authorize production access, external messages,
credential changes, commits, pushes, publishing, deployment, or deletion outside
the requested scope. Perform those actions only when the user explicitly requests
that exact action and the target is known.

User instructions take precedence. Preserve unrelated edits and never claim a
review, test, or delegated result that was not actually observed.
