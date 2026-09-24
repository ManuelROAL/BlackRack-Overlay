import { formatNumber } from "./i18n";

export type TemperatureUnit = "celsius" | "fahrenheit";
export type SpeedUnit = "kmh" | "mph";

export interface DisplayUnits {
  temperature: TemperatureUnit;
  speed: SpeedUnit;
}

export const DISPLAY_UNITS_STORAGE_KEY = "blackrack-overlay.display-units.v1";

const DEFAULT_DISPLAY_UNITS: DisplayUnits = {
  temperature: "celsius",
  speed: "kmh"
};

export const normalizeDisplayUnits = (value: unknown): DisplayUnits => {
  if (typeof value !== "object" || value === null) return { ...DEFAULT_DISPLAY_UNITS };
  const candidate = value as Partial<DisplayUnits>;
  return {
    temperature: candidate.temperature === "fahrenheit" ? "fahrenheit" : "celsius",
    speed: candidate.speed === "mph" ? "mph" : "kmh"
  };
};

const readStoredDisplayUnits = (): DisplayUnits => {
  try {
    return normalizeDisplayUnits(JSON.parse(localStorage.getItem(DISPLAY_UNITS_STORAGE_KEY) ?? "null"));
  } catch {
    return { ...DEFAULT_DISPLAY_UNITS };
  }
};

let displayUnits = readStoredDisplayUnits();

export const readDisplayUnits = (): DisplayUnits => ({ ...displayUnits });

export const persistDisplayUnits = (value: unknown): DisplayUnits => {
  displayUnits = normalizeDisplayUnits(value);
  localStorage.setItem(DISPLAY_UNITS_STORAGE_KEY, JSON.stringify(displayUnits));
  return readDisplayUnits();
};

/** Apply a settings event received from the control panel without writing storage. */
export const applyDisplayUnits = (value: unknown): DisplayUnits => {
  displayUnits = normalizeDisplayUnits(value);
  return readDisplayUnits();
};

export const temperatureValue = (celsius: number): number =>
  displayUnits.temperature === "fahrenheit" ? celsius * 9 / 5 + 32 : celsius;

export const speedValue = (kilometresPerHour: number): number =>
  displayUnits.speed === "mph" ? kilometresPerHour / 1.609344 : kilometresPerHour;

export const temperatureUnit = (): "°C" | "°F" =>
  displayUnits.temperature === "fahrenheit" ? "°F" : "°C";

export const speedUnit = (): "km/h" | "mph" =>
  displayUnits.speed === "mph" ? "mph" : "km/h";

export const speedUnitShort = (): "KM/H" | "MPH" =>
  displayUnits.speed === "mph" ? "MPH" : "KM/H";

export const formatTemperatureValue = (celsius: number, digits = 0): string =>
  Number.isFinite(celsius) ? formatNumber(temperatureValue(celsius), digits) : "--";

export const formatTemperature = (celsius: number, digits = 0): string =>
  `${formatTemperatureValue(celsius, digits)}${temperatureUnit()}`;

export const formatSpeedValue = (kilometresPerHour: number, digits = 0): string =>
  Number.isFinite(kilometresPerHour) ? formatNumber(speedValue(kilometresPerHour), digits) : "--";

export const formatSpeed = (kilometresPerHour: number, digits = 0): string =>
  `${formatSpeedValue(kilometresPerHour, digits)} ${speedUnit()}`;
