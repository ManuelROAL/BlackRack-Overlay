# LMU Chat overlay

## Scope and data

- Entry: `chat.html`
- Renderer/style: `src/chat.ts`, `src/chat.css`
- Native/composite event: `chat://update`
- OBS browser source event: named SSE event `chat` on `/chat`
- The backend reads messages from LMU's local REST chat endpoint and sends a
  bounded snapshot only while the native overlay or an OBS `/chat` client is
  active. It publishes at most once every 2 seconds.
- Each snapshot contains at most the latest eight messages. The renderer uses
  each message's stable id to preserve its original local reception time and to
  avoid touching unchanged row content or order.

## Layout and behavior

- Show local `HH:mm` reception time, sender and message text in a compact,
  transparent panel. When the snapshot is empty, show only a subdued empty-state
  label and a lighter frame.
- This is a read-only view. It does not send chat messages, retain history after
  the bounded live window, or persist any message data.
- Keep the panel responsive to standard transparency and text-size preferences,
  proportional fit, game-mode click-through and the shared performance tracker.
- Avoid continuous animation and repaint only when a new or changed snapshot
  arrives.
