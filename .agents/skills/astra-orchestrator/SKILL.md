---
name: astra-orchestrator
description: Select whether Codex coding work should run solo, use bounded Luna delegation, or receive an independent Astra review. Use for implementation, debugging, refactoring, and explicit orchestration requests; default to solo even for multi-file work. Do not activate for general questions unrelated to repository work.
---

# Selective Astra / Luna orchestration

User instructions take precedence. The root owns requirements, architecture, routing, integration, and final acceptance. Work solo by default; agents are optional tools, not a mandatory pipeline.

## Choose a route

Inspect enough context locally to understand the task before deciding. Before implementation or spawning, state one short sentence in the user's language naming the mode and its concrete reason. Do not ask permission merely to choose a route within authorized work.

- **solo**: root explores, implements, tests, and reviews. Choose for localized or tightly coupled work, mechanical multi-file edits, straightforward debugging, short documentation lookups, or when delegation overhead is likely to exceed its benefit.
- **delegate**: assign a substantial, bounded task to a Luna worker, explorer, tester, or researcher. Choose only when its scope and deliverable are clear and separate context or parallel work provides a concrete benefit. Root integrates and verifies; an independent reviewer is not automatic.
- **audit**: root implements and verifies, then a fresh read-only Astra reviewer inspects the actual diff. Choose when independent scrutiny is useful for a concrete risk such as authorization logic, destructive data migration, concurrency, or a subtle compatibility change, but implementation is best kept in the root.
- **full**: bounded Luna delegation plus independent Astra review. Choose only when both delegation and review have separate concrete justifications, or the user explicitly requests that workflow. It does not mean spawning every role.

File count, unfamiliar code, external research, and the existence of several roles are not sufficient reasons to spawn. Complexity alone does not make implementation separable. A small change can still justify audit if its consequences are substantial.

Use one auxiliary by default for delegate or audit. Full usually uses one Luna implementer followed by one Astra reviewer. Add more only for substantial independent tasks with clear ownership and expected benefit; respect runtime concurrency limits. Do not fill available slots just because they exist.

Reassess if new evidence changes scope or risk. Briefly explain a route change. Avoid further spawns when their benefit disappears; account for agents already running and their edits before continuing locally. Follow an explicit user request for agents or solo work, while reporting any resulting verification limitations honestly.

## Models and execution

Keep the configured root model: Astra for the Pro preset, Luna/max for the Plus preset. This skill cannot switch the running root model. Do not require Sol or modify global model settings.

Use gpt-5.6-luna for execution roles and gpt-6-astra for independent review. Use the configured named role when available and verify exposed model metadata. If an API supports explicit model selection, use it in accordance with that API's context-inheritance rules. Never claim a model was used without runtime evidence.

Use actual native subagents when a route selects them. Do not create separate user-owned tasks as a substitute. If a required role/model/tool is unavailable, report it; continue locally when it can fulfill the user request, and state that independent review was unavailable if relevant. Do not silently substitute models or claim a requested independent review was completed.

A worker encountering broader architectural decisions should return the decision to the root. Narrow the task or have the root handle the difficult part before considering another model. Do not upgrade routine workers to Astra automatically.

## Delegation contract

Include the objective, relevant context, exact question or file ownership, interfaces and constraints, acceptance criteria, and required verification. Tell writers they share the codebase, must preserve others' changes, and must not expand scope without coordinating with the root. Exploration, research, and review are read-only; testers edit tests only when assigned.

Ask for concise findings, changed files, verification commands and results, and unresolved issues. Auxiliary work substitutes for root work: do useful independent work while it runs, not the same investigation or implementation again. Inspect returned evidence and the diff; rerun checks when needed to establish correctness, rather than repeating every successful check mechanically.

Subagents must not spawn additional agents unless the root explicitly assigns a bounded nested delegation. Keep architecture and overall route selection in the root.

## Review and completion

In audit/full, provide a fresh reviewer the actual diff, requirements, and verification evidence. Request actionable correctness and regression findings with file references; no edits or style-only busywork. Resolve material findings, verify fixes, and obtain follow-up review of material corrections where needed. Do not add a reviewer in solo/delegate without a new risk-based route decision or user request.

Wait for selected work before claiming completion. On failure, inspect the reason, report it, and narrow, retry, or finish locally when reasonable. Do not leave writers active while taking over their files. Summarize the result, meaningful verification, and limitations; do not claim that the routing policy guarantees lower usage or perfect review.
