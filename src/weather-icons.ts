import clearIconUrl from "./assets/lmu-icons/weather/clear.svg";
import lightCloudIconUrl from "./assets/lmu-icons/weather/light-cloud.svg";
import partiallyCloudyIconUrl from "./assets/lmu-icons/weather/partially-cloudy.svg";
import mostlyCloudyIconUrl from "./assets/lmu-icons/weather/mostly-cloudy.svg";
import overcastIconUrl from "./assets/lmu-icons/weather/overcast.svg";
import drizzleIconUrl from "./assets/lmu-icons/weather/cloudy-and-drizzle.svg";
import lightRainIconUrl from "./assets/lmu-icons/weather/cloudy-and-light-rain.svg";
import overcastLightRainIconUrl from "./assets/lmu-icons/weather/overcast-and-light-rain.svg";
import overcastRainIconUrl from "./assets/lmu-icons/weather/overcast-and-rain.svg";
import overcastHeavyRainIconUrl from "./assets/lmu-icons/weather/overcast-and-heavy-rain.svg";
import stormIconUrl from "./assets/lmu-icons/weather/overcast-and-storm.svg";

const WEATHER_ICONS: readonly string[] = [
  clearIconUrl,
  lightCloudIconUrl,
  partiallyCloudyIconUrl,
  mostlyCloudyIconUrl,
  overcastIconUrl,
  drizzleIconUrl,
  lightRainIconUrl,
  overcastLightRainIconUrl,
  overcastRainIconUrl,
  overcastHeavyRainIconUrl,
  stormIconUrl
];

export const weatherIconUrl = (sky: number): string =>
  Number.isInteger(sky) && sky >= 0 && sky < WEATHER_ICONS.length
    ? WEATHER_ICONS[sky]
    : clearIconUrl;