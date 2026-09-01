const optimalTireTemperature = (compound: string): number => {
  const initial = compound.trim().charAt(0).toUpperCase();
  if (initial === "W" || initial === "I") return 50;
  if (initial === "S") return 80;
  if (initial === "H") return 100;
  return 90;
};

export const tireTemperatureColor = (temperature: number, compound: string): string => {
  if (!Number.isFinite(temperature) || temperature < 0) return "#687481";
  const optimal = optimalTireTemperature(compound);
  if (temperature < optimal - 30) return "#5268e9";
  if (temperature < optimal - 20) return "#4b91ff";
  if (temperature < optimal - 10) return "#4dcff5";
  if (temperature < optimal) return "#55c8be";
  if (temperature < optimal + 10) return "#55d89a";
  if (temperature < optimal + 20) return "#8fe04f";
  if (temperature < optimal + 30) return "#efdb3d";
  if (temperature < optimal + 40) return "#f58a35";
  return "#f05252";
};

export const tireTemperatureTextColor = (temperature: number, compound: string): string =>
  !Number.isFinite(temperature) || temperature < optimalTireTemperature(compound) - 20
    ? "#f4f7fa"
    : "#07100f";

export const brakeTemperatureColor = (temperature: number): string => {
  if (!Number.isFinite(temperature) || temperature < 0) return "#69737d";
  if (temperature < 100) return "#5268e9";
  if (temperature < 200) return "#4b91ff";
  if (temperature < 300) return "#4dcff5";
  if (temperature < 400) return "#55c8be";
  if (temperature < 500) return "#55d89a";
  if (temperature < 600) return "#8fe04f";
  if (temperature < 700) return "#efdb3d";
  if (temperature < 800) return "#f58a35";
  return "#f05252";
};
