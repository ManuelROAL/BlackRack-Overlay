export const SPECTATOR_MODE_KEY = "blackrack-overlay.spectator-mode.v1";
export const TEAM_MODE_KEY = "blackrack-overlay.team-mode.v1";

export const readSpectatorMode = (): boolean => {
  try {
    return JSON.parse(localStorage.getItem(SPECTATOR_MODE_KEY) ?? "false") === true;
  } catch {
    return false;
  }
};

export const saveSpectatorMode = (enabled: boolean): void => {
  localStorage.setItem(SPECTATOR_MODE_KEY, JSON.stringify(enabled));
};

export const readTeamMode = (): boolean => {
  try {
    return JSON.parse(localStorage.getItem(TEAM_MODE_KEY) ?? "false") === true;
  } catch {
    return false;
  }
};

export const saveTeamMode = (enabled: boolean): void => {
  localStorage.setItem(TEAM_MODE_KEY, JSON.stringify(enabled));
};
