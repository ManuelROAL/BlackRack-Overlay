# Chat overlay

## Scope and files

- Entry: `chat.html`
- Renderer/style: `src/chat.ts`, `src/chat.css`
- Source: the optional `LMU_BlackRackChatBridge.dll` callback through
  `src-tauri/src/telemetry/sim/lmu/chat.rs`, with LMU's active
  `UserData/Log/trace*.txt` file as an automatic fallback
- Capability: `chat`; the control-panel card is disabled when the simulator
  cannot expose a local chat trace
- OBS route: `/chat`
- Cadence: 4 Hz while the overlay or its browser source is active. The native
  bridge avoids LMU's delayed trace flush; the fallback trace is polled at most
  every 250 ms.

## Behavior

The overlay shows the most recent 24 chat lines. When
`LMU_BlackRackChatBridge.dll` is loaded by LMU, it receives chat through
`InternalsPlugin::WantsToDisplayMessage` and sends it through a private,
bounded shared-memory queue. When the plugin is absent or inactive, the source
falls back to LMU's `NetComm::PushToChats` trace, reading it incrementally and
starting with a bounded tail. System lines without a sender separator are
labelled `RACE CONTROL`.

The source is read-only. No chat contents are persisted by BlackRack Overlay,
sent to a remote service or accepted as HTML; the frontend inserts both sender
and message as text.

If LMU has no active trace or the log format changes, the overlay remains empty
and the rest of telemetry continues normally. The log path is discovered from
the same installed LMU locations used by the shared-memory dependency probe.

The optional bridge is built from the repository root with
`powershell -ExecutionPolicy Bypass -File .\build-lmu-chat-bridge.ps1`. Copy
`dist/LMU_BlackRackChatBridge.dll` to LMU's `Plugins` directory and enable
`"LMU_BlackRackChatBridge.dll": { " Enabled": 1 }` in
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
