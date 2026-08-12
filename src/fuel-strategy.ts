export interface ResourceStrategyInput {
  current: number;
  capacity: number;
  consumption: number;
  lapsRemaining: number;
  lapProgress: number;
  completedLaps: number;
  pitCycleConsumption: number;
  pitOutConsumption: number;
  pitOutLap: boolean;
}

export interface ResourceStrategy {
  stops: number;
  targetStops: number;
  targetConsumption: number;
  savingPercent: number;
  autonomy: number;
  minutes: number;
  earliestPitLap: number;
  latestPitLap: number;
  nextFill: number;
  totalAdditional: number;
  endRemaining: number;
}

const validPositive = (value: number): boolean => Number.isFinite(value) && value > 0;

const pitAdjustment = (consumption: number, pitCycleConsumption: number): number =>
  pitCycleConsumption > 0 ? pitCycleConsumption - 2 * consumption : 0;

const remainingPitAdjustment = (
  consumption: number,
  input: ResourceStrategyInput,
  stops: number
): number => {
  if (input.pitOutLap && input.pitOutConsumption > 0) {
    const laterStops = Math.max(stops - 1, 0);
    return input.pitOutConsumption - consumption
      + pitAdjustment(consumption, input.pitCycleConsumption) * laterStops;
  }
  return pitAdjustment(consumption, input.pitCycleConsumption) * stops;
};

const requiredForStops = (
  input: ResourceStrategyInput,
  stops: number,
  consumption = input.consumption
): number =>
  Math.max(
    consumption * input.lapsRemaining + remainingPitAdjustment(consumption, input, stops),
    0
  );

const availableForStops = (
  current: number,
  capacity: number,
  stops: number
): number =>
  Math.max(current, 0) + Math.max(capacity, 0) * stops;

export const stopsRequired = (input: ResourceStrategyInput): number => {
  if (
    !validPositive(input.consumption) ||
    !validPositive(input.capacity) ||
    !validPositive(input.lapsRemaining)
  ) {
    return 0;
  }

  for (let stops = 0; stops <= 100; stops += 1) {
    if (
      availableForStops(input.current, input.capacity, stops) + 1e-6 >=
      requiredForStops(input, stops)
    ) {
      return stops;
    }
  }
  return 100;
};

const consumptionForStops = (input: ResourceStrategyInput, stops: number): number => {
  const available = availableForStops(input.current, input.capacity, stops);
  const activePitOut = input.pitOutLap && input.pitOutConsumption > 0;
  const laterStops = Math.max(stops - (activePitOut ? 1 : 0), 0);
  const replacedLaps = (activePitOut ? 1 : 0) + 2 * laterStops;
  const fixedPitConsumption = (activePitOut ? input.pitOutConsumption : 0)
    + input.pitCycleConsumption * laterStops;
  const denominator = input.lapsRemaining - replacedLaps;
  if (denominator <= 0) return available / input.lapsRemaining;
  return Math.max((available - fixedPitConsumption) / denominator, 0);
};

export const calculateResourceStrategy = (
  input: ResourceStrategyInput,
  lapSeconds: number,
  minimumStops = 0
): ResourceStrategy | undefined => {
  if (
    !validPositive(input.consumption) ||
    !validPositive(input.capacity) ||
    !validPositive(input.lapsRemaining) ||
    !Number.isFinite(input.current)
  ) {
    return undefined;
  }

  const stops = Math.max(stopsRequired(input), Math.max(0, Math.floor(minimumStops)));
  // Una parada que ya está en curso no se puede eliminar ahorrando combustible.
  const targetStops = input.pitOutLap ? stops : Math.max(stops - 1, 0);
  const targetConsumption = consumptionForStops(input, targetStops);
  const savingPercent = Math.max(
    (input.consumption - targetConsumption) / input.consumption * 100,
    0
  );
  const autonomy = Math.max(input.current, 0) / input.consumption;
  const minutes = lapSeconds > 0 ? autonomy * lapSeconds / 60 : 0;

  const crossingsToLatest = Math.max(
    0,
    Math.floor(autonomy + Math.max(0, Math.min(1, input.lapProgress)) + 1e-6)
  );
  let earliestPitLap = 0;
  let latestPitLap = 0;
  let nextFill = 0;

  if (stops > 0) {
    const fullStintDistance = Math.max(input.capacity, 0) / input.consumption;
    const distanceNeededBeforePit = Math.max(
      input.lapsRemaining - fullStintDistance * stops,
      0
    );
    const earliestCrossing = Math.max(
      1,
      Math.ceil(distanceNeededBeforePit + input.lapProgress - 1e-6)
    );
    const latestCrossing = Math.max(earliestCrossing, crossingsToLatest);
    earliestPitLap = input.completedLaps + earliestCrossing;
    latestPitLap = input.completedLaps + latestCrossing;

    if (input.pitOutLap) {
      nextFill = Math.min(
        input.capacity,
         Math.max(requiredForStops(input, stops) - input.capacity * (stops - 1), 0)
      );
    } else {
      const distanceToPit = Math.max(latestCrossing - input.lapProgress, 0);
      const remainingAfterPit = Math.max(input.lapsRemaining - distanceToPit, 0);
      const balancedNextStint = remainingAfterPit / stops;
      nextFill = Math.min(
        input.capacity,
        input.consumption * balancedNextStint
      );
    }
  }

  const required = requiredForStops(input, stops);
  const totalAdditional = Math.max(required - input.current, 0);
  const endRemaining = Math.max(input.current + totalAdditional - required, 0);

  return {
    stops,
    targetStops,
    targetConsumption,
    savingPercent,
    autonomy,
    minutes,
    earliestPitLap,
    latestPitLap,
    nextFill,
    totalAdditional,
    endRemaining
  };
};
