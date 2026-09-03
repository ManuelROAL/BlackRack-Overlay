/// Last resort when the simulator has not published the car's own compound
/// list: a guess by initial, which only holds for a car carrying the full
/// soft/medium/hard/wet ladder.
const assumedOptimalTemperature = (compound: string): number => {
  const initial = compound.trim().charAt(0).toUpperCase();
  if (initial === "W" || initial === "I") return 50;
  if (initial === "S") return 80;
  if (initial === "H") return 100;
  return 90;
};

/// `optimal` is the temperature the simulator reports for the compound actually
/// fitted, and -1 when it is unknown.
export const optimalTireTemperature = (optimal: number, compound: string): number =>
  Number.isFinite(optimal) && optimal > 0 ? optimal : assumedOptimalTemperature(compound);

export const tireTemperatureColor = (
  temperature: number,
  optimal: number,
  compound: string
): string => {
  if (!Number.isFinite(temperature) || temperature < 0) return "#687481";
  const reference = optimalTireTemperature(optimal, compound);
  if (temperature < reference - 30) return "#5268e9";
  if (temperature < reference - 20) return "#4b91ff";
  if (temperature < reference - 10) return "#4dcff5";
  if (temperature < reference) return "#55c8be";
  if (temperature < reference + 10) return "#55d89a";
  if (temperature < reference + 20) return "#8fe04f";
  if (temperature < reference + 30) return "#efdb3d";
  if (temperature < reference + 40) return "#f58a35";
  return "#f05252";
};

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
