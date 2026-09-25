import "./styles.css";
import "./chat.css";
import { fitOverlayToContentBox } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import { normalizeChatSettings, readChatSettings } from "./chat-settings";
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

const MAX_REMEMBERED_MESSAGES = 64;
const CHAT_IDLE_TIMEOUT_MS = 20_000;
const panel = document.getElementById("chat-panel")!;
const empty = document.getElementById("chat-empty")!;
const list = document.getElementById("chat-messages")!;
const renderPerformance = createOverlayPerformanceTracker("chat");
const rendered = new Map<string, RenderedMessage>();
const knownMessages = new Map<string, { name: string; text: string }>();
let chatSettings = readChatSettings();
let latestSnapshot: ChatSnapshot = { replace: true, messages: [] };
let inactivityTimer = 0;

if (document.documentElement.dataset.browserSource !== "true") {
  fitOverlayToContentBox(panel);
}
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

const showForRecentMessage = (): void => {
  panel.dataset.visible = "true";
  if (inactivityTimer) window.clearTimeout(inactivityTimer);
  inactivityTimer = window.setTimeout(() => {
    panel.dataset.visible = "false";
    inactivityTimer = 0;
  }, CHAT_IDLE_TIMEOUT_MS);
};

const render = (snapshot: ChatSnapshot): void => {
  if (!snapshot?.replace || !Array.isArray(snapshot.messages)) return;
  const validMessages = snapshot.messages
    .filter((message): message is ChatMessage =>
      typeof message?.id === "string" && message.id.length > 0 && message.id.length <= 128
      && typeof message.name === "string" && typeof message.text === "string");
  let hasNewMessage = false;
  for (const message of validMessages) {
    const known = knownMessages.get(message.id);
    if (!known || known.name !== message.name || known.text !== message.text) {
      hasNewMessage = true;
      knownMessages.set(message.id, { name: message.name, text: message.text });
    }
  }
  while (knownMessages.size > MAX_REMEMBERED_MESSAGES) {
    const oldestId = knownMessages.keys().next().value;
    if (oldestId === undefined) break;
    knownMessages.delete(oldestId);
  }
  const messages = validMessages.slice(-chatSettings.maxMessages);
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
  if (hasNewMessage) showForRecentMessage();
};

void listenChat((snapshot: ChatSnapshot) => {
  latestSnapshot = snapshot;
  renderPerformance.measure(() => render(snapshot), snapshot.messages?.length ?? 0);
}, (payload) => {
  const next = normalizeChatSettings(payload);
  if (next.maxMessages === chatSettings.maxMessages) return;
  chatSettings = next;
  renderPerformance.measure(
    () => render(latestSnapshot),
    latestSnapshot.messages.length
  );
});

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
