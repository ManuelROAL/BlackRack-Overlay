import "./styles.css";
import "./chat.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import type { ChatMessage } from "./telemetry-types";
import { listenTelemetry } from "./runtime-events";

fitOverlay({ width: 420, height: 270 }, { heightTextRatio: 0.08 });
bindOverlayTransparency("chat");
bindOverlayInteractionMode();
const performance = createOverlayPerformanceTracker("chat");
const card = document.getElementById("chat-card");
const messages = document.getElementById("chat-messages");
const empty = document.getElementById("chat-empty");
const count = document.getElementById("chat-count");

const render = (nextMessages: ChatMessage[]): void => {
  const signature = nextMessages.map(({ sender, text }) => `${sender}\u0000${text}`).join("\u0001");
  const available = nextMessages.length > 0;
  card?.setAttribute("data-state", available ? "active" : "waiting");
  if (count && count.textContent !== (available ? String(nextMessages.length) : "--")) {
    count.textContent = available ? String(nextMessages.length) : "--";
  }
  empty?.toggleAttribute("hidden", available);
  if (!messages || messages.dataset.signature === signature) return;
  messages.dataset.signature = signature;
  messages.replaceChildren(...nextMessages.map(({ sender, text }) => {
    const row = document.createElement("li");
    const system = sender === "RACE CONTROL";
    row.toggleAttribute("data-system", system);
    const senderNode = document.createElement("span");
    senderNode.className = "chat-sender";
    senderNode.textContent = sender;
    const textNode = document.createElement("span");
    textNode.className = "chat-text";
    textNode.textContent = text;
    row.append(senderNode, textNode);
    return row;
  }));
  messages.scrollTop = messages.scrollHeight;
};

void listenTelemetry((frame) => performance.measure(() => render(frame.chat)));

if (import.meta.env.DEV) {
  const preview = new URLSearchParams(window.location.search).get("preview");
  if (preview === "chat") {
    render([
      { sender: "K Niedzwiecki", text: "Sorry" },
      { sender: "L Petit#2851", text: "Go Right" },
      { sender: "RACE CONTROL", text: "Driver joined #7 Team" }
    ]);
  }
}
