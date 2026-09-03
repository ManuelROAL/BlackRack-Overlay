// LMU numera las sesiones como 0-4 práctica, 5-8 clasificación, 9 warmup y
// 10-13 carrera. Varias columnas de Standings y Relative solo tienen sentido
// cuando existen parrilla, clasificación final y estrategia comparable, es
// decir únicamente en carrera; el resto de sesiones las oculta o las degrada.
export const isRaceSession = (sessionType: number): boolean =>
  sessionType >= 10 && sessionType <= 13;

export const isPracticeSession = (sessionType: number): boolean =>
  sessionType >= 0 && sessionType <= 4;
