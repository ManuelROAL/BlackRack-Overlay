# Chat overlay

## Scope and files

- Entry: `chat.html`
- Renderer/style: `src/chat.ts`, `src/chat.css`
- Source: the optional `LMU_BlackRackPlugin.dll` callback through
  `src-tauri/src/telemetry/sim/lmu/chat.rs`, with LMU's active
  `UserData/Log/trace*.txt` file as an automatic fallback
- Capability: `chat`; the control-panel card is disabled when the simulator
  cannot expose a local chat trace
- OBS route: `/chat`
- Cadence: 50 Hz while the overlay or its browser source is active. The trace
  fallback is polled at most every 20 ms, so the overlay does not add a separate
  250 ms polling delay after LMU flushes a complete line.

## Behavior

The overlay shows the most recent 24 chat lines. When
`LMU_BlackRackPlugin.dll` is loaded by LMU, the source checks its private,
bounded shared-memory queue. The public `InternalsPlugin::WantsToDisplayMessage`
callback is an optional path and is not a guaranteed feed of incoming network
chat in the current LMU build. When that queue has not delivered a message,
the source falls back to LMU's `NetComm::PushToChats` trace, reading it
incrementally and starting with a bounded tail. System lines without a sender
separator are labelled `RACE CONTROL`.

An active bridge mapping is not treated as proof that callbacks are arriving:
the trace fallback remains available until the bridge delivers its first
message, so another plugin or an LMU callback variation cannot leave the panel
empty.

The source is read-only. No chat contents are persisted by BlackRack Overlay,
sent to a remote service or accepted as HTML; the frontend inserts both sender
and message as text.

If LMU has no active trace or the log format changes, the overlay remains empty
and the rest of telemetry continues normally. The log path is discovered from
the same installed LMU locations used by the shared-memory dependency probe.

The optional plugin is built from the repository root with
`powershell -ExecutionPolicy Bypass -File .\build-lmu-plugin.ps1`. Copy
`dist/LMU_BlackRackPlugin.dll` to LMU's `Plugins` directory and enable
`"LMU_BlackRackPlugin.dll": { " Enabled": 1 }` in
`UserData/player/CustomPluginVariables.JSON`. LMU must be restarted after the
DLL is installed. The plugin does not write chat messages into the game or
persist chat contents.

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
