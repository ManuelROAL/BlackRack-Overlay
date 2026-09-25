export interface ChatSettings {
  maxMessages: number;
  maxHeight: number;
}

export const CHAT_SETTINGS_KEY = "blackrack-overlay.chat-settings.v1";
export const CHAT_SETTINGS_EVENT = "chat://settings";
export const CHAT_MAX_MESSAGES = 8;
export const CHAT_DEFAULT_MAX_HEIGHT = 240;
export const CHAT_MIN_MAX_HEIGHT = 80;
export const CHAT_MAX_MAX_HEIGHT = 400;
export const CHAT_MAX_HEIGHT_STEP = 20;

export const defaultChatSettings = (): ChatSettings => ({
  maxMessages: CHAT_MAX_MESSAGES,
  maxHeight: CHAT_DEFAULT_MAX_HEIGHT
});

export const normalizeChatSettings = (value: unknown): ChatSettings => {
  const source = value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
  const candidate = source.maxMessages;
  const heightCandidate = source.maxHeight;
  const maxMessages = typeof candidate === "number" && Number.isFinite(candidate)
    ? Math.max(1, Math.min(Math.round(candidate), CHAT_MAX_MESSAGES))
    : CHAT_MAX_MESSAGES;
  const maxHeight = typeof heightCandidate === "number" && Number.isFinite(heightCandidate)
    ? Math.max(CHAT_MIN_MAX_HEIGHT, Math.min(
      CHAT_MAX_MAX_HEIGHT,
      CHAT_MIN_MAX_HEIGHT + Math.round((heightCandidate - CHAT_MIN_MAX_HEIGHT) / CHAT_MAX_HEIGHT_STEP) * CHAT_MAX_HEIGHT_STEP
    ))
    : CHAT_DEFAULT_MAX_HEIGHT;
  return { maxMessages, maxHeight };
};

export const readChatSettings = (): ChatSettings => {
  try {
    const stored = localStorage.getItem(CHAT_SETTINGS_KEY);
    return stored ? normalizeChatSettings(JSON.parse(stored)) : defaultChatSettings();
  } catch {
    return defaultChatSettings();
  }
};
