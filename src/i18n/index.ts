import { catalogs, type Message, type TranslationKey } from "./catalogs";

export const SUPPORTED_LOCALES = ["es", "en"] as const;
export type Locale = (typeof SUPPORTED_LOCALES)[number];
export const LOCALE_STORAGE_KEY = "lmu-overlay.locale.v1";
export const LOCALE_OPTIONS: ReadonlyArray<{ code: Locale; label: string }> = [
  { code: "es", label: "Español" },
  { code: "en", label: "English" }
];

export const isLocale = (value: unknown): value is Locale =>
  typeof value === "string" && (SUPPORTED_LOCALES as readonly string[]).includes(value);

export const resolveLocale = (): Locale => {
  const stored = localStorage.getItem(LOCALE_STORAGE_KEY);
  return isLocale(stored) ? stored : "en";
};

let locale = resolveLocale();
let pluralRules = new Intl.PluralRules(locale);
let numberFormatter = new Intl.NumberFormat(locale);
let fixedNumberFormatters = Array.from({ length: 4 }, (_, digits) => new Intl.NumberFormat(locale, {
  minimumFractionDigits: digits,
  maximumFractionDigits: digits
}));
let clockFormatter = new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" });

export const getLocale = (): Locale => locale;
export const setLocale = (next: Locale): void => {
  locale = next;
  localStorage.setItem(LOCALE_STORAGE_KEY, next);
  pluralRules = new Intl.PluralRules(next);
  numberFormatter = new Intl.NumberFormat(next);
  fixedNumberFormatters = Array.from({ length: 4 }, (_, digits) => new Intl.NumberFormat(next, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits
  }));
  clockFormatter = new Intl.DateTimeFormat(next, { hour: "2-digit", minute: "2-digit" });
  document.documentElement.lang = next;
};

type Parameters = Readonly<Record<string, string | number>>;
export const t = (key: TranslationKey, parameters: Parameters = {}): string => {
  const message: Message = catalogs[locale][key];
  const template = typeof message === "string"
    ? message
    : message[pluralRules.select(Number(parameters.count)) === "one" ? "one" : "other"];
  return template.replace(/\{([A-Za-z][A-Za-z0-9]*)\}/g, (_, name: string) =>
    Object.hasOwn(parameters, name) ? String(parameters[name]) : `{${name}}`
  );
};

export const formatNumber = (value: number, digits?: number): string =>
  digits === undefined ? numberFormatter.format(value) : fixedNumberFormatters[Math.max(0, Math.min(3, digits))].format(value);
export const formatClock = (value: Date | number): string => clockFormatter.format(value);

const applyAttribute = (root: ParentNode, attribute: string, target: string): void => {
  root.querySelectorAll<HTMLElement>(`[${attribute}]`).forEach((element) => {
    const key = element.getAttribute(attribute) as TranslationKey | null;
    if (key) element.setAttribute(target, t(key));
  });
};

export const applyTranslations = (root: ParentNode = document): void => {
  document.documentElement.lang = locale;
  root.querySelectorAll<HTMLElement>("[data-i18n]").forEach((element) => {
    const key = element.dataset.i18n as TranslationKey | undefined;
    if (key) element.textContent = t(key);
  });
  applyAttribute(root, "data-i18n-title", "title");
  applyAttribute(root, "data-i18n-aria-label", "aria-label");
  applyAttribute(root, "data-i18n-placeholder", "placeholder");
};

setLocale(locale);
export type { TranslationKey };
