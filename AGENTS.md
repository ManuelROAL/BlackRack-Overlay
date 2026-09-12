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
  environment-related, then fix and re-run checks for issues introduced by the
  change when safe. Report exact evidence for unresolved failures.
- For implementation tasks, continue through implementation and relevant
  validation until the requested behavior works and material introduced issues
  are resolved, or a decision genuinely requires user input.

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
credential changes, pushes, publishing, deployment, or deletion outside the
requested scope. The repository workflow above authorizes a scoped commit for
an in-scope file change after verification; all other commits and external or
irreversible actions require an explicit request and a known target.

User instructions take precedence. Preserve unrelated edits and never claim a
review, test, or delegated result that was not actually observed.


# Codex project instructions

For complex coding tasks, use the `astra-orchestrator` skill when its trigger conditions match.

The root agent owns architecture, decomposition, integration, and final verification.
Prefer specialized subagents for bounded exploration, implementation, testing, review, and technical research.

Do not delegate trivial work merely for parallelism.
Do not let multiple implementation agents edit the same files without explicit ownership boundaries.
User instructions always take precedence over this orchestration policy.
