/// dox paints the tread bands on a fixed ramp - blue at 40, green at 90, yellow
/// at 130 and red at 170 - and every state captured beside a read of shared
/// memory says the game paints its own HUD the same way. A rear tread at 200
/// after a slide is red on all three, a front at 46 is cyan, blankets at 62 are
/// green. Wets settle it: at 18 degrees the game and dox both paint blue, so the
/// scale does not move onto the compound's own window - a wet is simply judged
/// cold until it warms, and the optimum LMU publishes per compound does not
/// enter the colour.
const tireRamp: [number, string][] = [
  [40, "#1e90ff"],
  [90, "#00ff00"],
  [130, "#ffff00"],
  [170, "#ff0000"]
];

const channel = (color: string, offset: number): number =>
  parseInt(color.slice(offset, offset + 2), 16);

const mix = (from: string, to: string, factor: number): string => {
  const weight = Math.max(0, Math.min(1, factor));
  const blended = [1, 3, 5].map((offset) =>
    Math.round(channel(from, offset) + (channel(to, offset) - channel(from, offset)) * weight)
      .toString(16)
      .padStart(2, "0")
  );
  return `#${blended.join("")}`;
};

export const tireTemperatureColor = (temperature: number): string => {
  if (!Number.isFinite(temperature) || temperature < 0) return "#687481";
  const step = tireRamp.findIndex(([limit], index) => index > 0 && temperature <= limit);
  const [from, fromColor] = tireRamp[step > 0 ? step - 1 : tireRamp.length - 2];
  const [to, toColor] = tireRamp[step > 0 ? step : tireRamp.length - 1];
  return mix(fromColor, toColor, (temperature - from) / (to - from));
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
