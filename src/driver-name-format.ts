export const DRIVER_NAME_FORMATS = [
  { id: "full", labelKey: "name.full" },
  { id: "initialLast", labelKey: "name.initialLast" },
  { id: "firstLastInitial", labelKey: "name.firstInitial" },
  { id: "lastOnly", labelKey: "name.lastOnly" },
  { id: "firstOnly", labelKey: "name.firstOnly" },
  { id: "lastFirstInitial", labelKey: "name.lastFirst" }
] as const;

export type DriverNameFormat = (typeof DRIVER_NAME_FORMATS)[number]["id"];

export const DEFAULT_DRIVER_NAME_FORMAT: DriverNameFormat = "full";

export const isDriverNameFormat = (value: unknown): value is DriverNameFormat =>
  DRIVER_NAME_FORMATS.some(({ id }) => id === value);

export const formatDriverName = (value: string, format: DriverNameFormat): string => {
  const parts = value.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "—";
  if (parts.length === 1) return parts[0];

  const first = parts[0];
  const last = parts.slice(1).join(" ");
  const firstInitial = `${first.charAt(0)}.`;
  const lastInitial = `${parts[1].charAt(0)}.`;

  switch (format) {
    case "initialLast": return `${firstInitial} ${last}`;
    case "firstLastInitial": return `${first} ${lastInitial}`;
    case "lastOnly": return last;
    case "firstOnly": return first;
    case "lastFirstInitial": return `${last}, ${firstInitial}`;
    case "full": return parts.join(" ");
  }
};
