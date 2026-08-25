import profileIconUrl from "./assets/lmu-icons/helmet-race-svgrepo-com.svg";
import timingIconUrl from "./assets/lmu-icons/timing.svg";
import fuelIconUrl from "./assets/lmu-icons/fuel.svg";
import tiresIconUrl from "./assets/lmu-icons/tires.svg";
import airTemperatureIconUrl from "./assets/lmu-icons/air-temperature.svg";
import trackTemperatureIconUrl from "./assets/lmu-icons/track-temperature.svg";
import hardCompoundIconUrl from "./assets/lmu-icons/compounds/hard.svg";
import intermediateCompoundIconUrl from "./assets/lmu-icons/compounds/intermediate.svg";
import mediumCompoundIconUrl from "./assets/lmu-icons/compounds/medium.svg";
import softCompoundIconUrl from "./assets/lmu-icons/compounds/soft.svg";
import unknownCompoundIconUrl from "./assets/lmu-icons/compounds/unknown.svg";
import wetCompoundIconUrl from "./assets/lmu-icons/compounds/wet.svg";

export {
  airTemperatureIconUrl,
  fuelIconUrl,
  profileIconUrl,
  timingIconUrl,
  tiresIconUrl,
  trackTemperatureIconUrl
};

export const compoundIconUrl = (compound: string): string => {
  const normalized = compound.trim().toLowerCase();
  if (normalized === "s" || normalized.includes("soft")) return softCompoundIconUrl;
  if (normalized === "m" || normalized.includes("medium")) return mediumCompoundIconUrl;
  if (normalized === "h" || normalized.includes("hard")) return hardCompoundIconUrl;
  if (normalized === "i" || normalized.includes("inter")) return intermediateCompoundIconUrl;
  if (normalized === "w" || normalized.includes("wet")) return wetCompoundIconUrl;
  return unknownCompoundIconUrl;
};
