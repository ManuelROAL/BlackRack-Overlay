import "./styles.css";
import "./chat.css";
import { fitOverlayToContentBox } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import { listenChat } from "./runtime-events";

interface ChatMessage {
  id: string;
  name: string;
  text: string;
}

interface ChatSnapshot {
  replace: boolean;
  messages: ChatMessage[];
}

interface RenderedMessage {
  element: HTMLLIElement;
  time: HTMLTimeElement;
  name: HTMLElement;
  text: HTMLElement;
  nameValue: string;
  textValue: string;
}

const MAX_MESSAGES = 8;
const panel = document.getElementById("chat-panel")!;
const empty = document.getElementById("chat-empty")!;
const list = document.getElementById("chat-messages")!;
const renderPerformance = createOverlayPerformanceTracker("chat");
const rendered = new Map<string, RenderedMessage>();

fitOverlayToContentBox(panel);
bindOverlayTransparency("chat");
bindOverlayInteractionMode();

const localTime = (date: Date): string =>
  `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;

const createMessage = (message: ChatMessage): RenderedMessage => {
  const element = document.createElement("li");
  element.className = "chat-message";
  element.dataset.messageId = String(message.id);

  const time = document.createElement("time");
  time.className = "chat-time";
  const receivedAt = new Date();
  time.dateTime = receivedAt.toISOString();
  time.textContent = localTime(receivedAt);

  const name = document.createElement("strong");
  name.className = "chat-name";
  const text = document.createElement("span");
  text.className = "chat-text";
  element.append(time, name, text);

  const entry = { element, time, name, text, nameValue: "", textValue: "" };
  updateMessage(entry, message);
  return entry;
};

const updateMessage = (entry: RenderedMessage, message: ChatMessage): void => {
  if (entry.nameValue !== message.name) {
    entry.name.textContent = message.name;
    entry.nameValue = message.name;
  }
  if (entry.textValue !== message.text) {
    entry.text.textContent = message.text;
    entry.textValue = message.text;
  }
};

const render = (snapshot: ChatSnapshot): void => {
  if (!snapshot?.replace || !Array.isArray(snapshot.messages)) return;
  const messages = snapshot.messages
    .filter((message): message is ChatMessage =>
      typeof message?.id === "string" && message.id.length > 0 && message.id.length <= 128
      && typeof message.name === "string" && typeof message.text === "string")
    .slice(-MAX_MESSAGES);
  const nextIds = messages.map(({ id }) => id);

  for (const [id, entry] of rendered) {
    if (!nextIds.includes(id)) {
      entry.element.remove();
      rendered.delete(id);
    }
  }

  for (const message of messages) {
    let entry = rendered.get(message.id);
    if (!entry) {
      entry = createMessage(message);
      rendered.set(message.id, entry);
    } else {
      updateMessage(entry, message);
    }
  }

  const currentIds = Array.from(list.children, (child) => (child as HTMLElement).dataset.messageId ?? "");
  if (currentIds.length !== nextIds.length || currentIds.some((id, index) => id !== nextIds[index])) {
    const fragment = document.createDocumentFragment();
    for (const id of nextIds) fragment.append(rendered.get(id)!.element);
    list.append(fragment);
  }

  empty.hidden = messages.length > 0;
  panel.dataset.empty = String(messages.length === 0);
};

void listenChat((snapshot: ChatSnapshot) =>
  renderPerformance.measure(() => render(snapshot), snapshot.messages?.length ?? 0)
);

if (import.meta.env.DEV) {
  const params = new URLSearchParams(window.location.search);
  if (params.has("preview")) {
    render({
      replace: true,
      messages: [
        { id: "1", name: "Race Control", text: "Chat de LMU listo" },
        { id: "2", name: "Driver", text: "Nos vemos en pista" }
      ]
    });
  }
}
