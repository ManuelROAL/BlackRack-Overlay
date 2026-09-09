# Chat overlay

## Scope and files

- Entry: `chat.html`
- Renderer/style: `src/chat.ts`, `src/chat.css`
- Source: LMU's active `UserData/Log/trace*.txt` file through
  `src-tauri/src/telemetry/sim/lmu/chat.rs`
- Capability: `chat`; the control-panel card is disabled when the simulator
  cannot expose a local chat trace
- OBS route: `/chat`
- Cadence: 4 Hz, with the trace polled at most every 250 ms while the overlay
  or its browser source is active

## Behavior

The overlay shows the most recent 24 chat lines written by LMU's
`NetComm::PushToChats` logger. It reads the local log incrementally and starts
with a bounded tail when enabled, so it does not scan the whole file on every
telemetry cycle. System lines without a sender separator are labelled `RACE
CONTROL`.

The source is read-only. No chat contents are persisted by BlackRack Overlay,
sent to a remote service or accepted as HTML; the frontend inserts both sender
and message as text.

If LMU has no active trace or the log format changes, the overlay remains empty
and the rest of telemetry continues normally. The log path is discovered from
the same installed LMU locations used by the shared-memory dependency probe.

## Presentation

- Keep the compact dark shell and lime left edge shared by the other overlays.
- Messages are ordered oldest to newest, with the newest line at the bottom.
- Driver names use lime; system notices use muted `RACE CONTROL` styling.
- Do not add a text input or write path: chat entry stays in the game.

## Verification focus

Run LMU in a session, hide its HUD chat and enable this overlay. Confirm driver
messages, quick chat and system notifications appear without HTML interpretation,
the newest messages replace the oldest after the 24-line limit, and stopping or
rotating the trace leaves shared-memory telemetry and other overlays running.
