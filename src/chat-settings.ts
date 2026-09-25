export interface ChatSettings {
  maxMessages: number;
}

export const CHAT_SETTINGS_KEY = "blackrack-overlay.chat-settings.v1";
export const CHAT_SETTINGS_EVENT = "chat://settings";
export const CHAT_MAX_MESSAGES = 8;

export const defaultChatSettings = (): ChatSettings => ({ maxMessages: CHAT_MAX_MESSAGES });

export const normalizeChatSettings = (value: unknown): ChatSettings => {
  const candidate = value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>).maxMessages
    : undefined;
  const maxMessages = typeof candidate === "number" && Number.isFinite(candidate)
    ? Math.max(1, Math.min(Math.round(candidate), CHAT_MAX_MESSAGES))
    : CHAT_MAX_MESSAGES;
  return { maxMessages };
};

export const readChatSettings = (): ChatSettings => {
  try {
    const stored = localStorage.getItem(CHAT_SETTINGS_KEY);
    return stored ? normalizeChatSettings(JSON.parse(stored)) : defaultChatSettings();
  } catch {
    return defaultChatSettings();
  }
};
