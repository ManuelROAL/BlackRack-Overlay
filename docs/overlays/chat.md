# LMU Chat overlay

## Scope and data

- Entry: `chat.html`
- Renderer/style: `src/chat.ts`, `src/chat.css`
- Native/composite event: `chat://update`
- OBS browser source event: named SSE event `chat` on `/chat`
- The backend reads messages from LMU's local REST chat endpoint and sends a
  bounded snapshot only while the native overlay or an OBS `/chat` client is
  active. It publishes at most once every 2 seconds.
- Each snapshot contains at most the latest eight messages. The renderer shows
  the configured latest one to eight, defaulting to eight. It uses each
  message's stable id to preserve its original local reception time and to
  avoid touching unchanged row content or order. The same message limit is
  applied to the OBS browser source.

## Layout and behavior

- Show local `HH:mm` reception time, sender and message text in a compact,
  transparent panel. Keep the newest message at the bottom and preserve that
  bottom edge as the panel grows upward. For OBS, anchor it to the bottom of the
  browser source viewport. The user can limit the visible message count from
  one to eight; older rows are removed from the rendered list.
- Keep the panel invisible until a message arrives, then hide it after 20
  seconds without a new message. Show it again when another message arrives;
  repeated snapshots of the same messages do not reset the timer.
- This is a read-only view. It does not send chat messages, retain history after
  the bounded live window, or persist any message data.
- Keep the panel responsive to standard transparency and text-size preferences,
  proportional fit, game-mode click-through and the shared performance tracker.
- Avoid continuous animation and repaint only when a new or changed snapshot
  arrives.
