# Campos observados en respuestas LMU y RaceOS — 12 de agosto de 2026

Este catálogo conserva los esquemas inferidos de respuestas reales obtenidas
durante la auditoría de endpoints. No contiene valores de pilotos, identificadores
de cuenta, tickets, tokens ni credenciales. Los campos pueden cambiar entre
versiones de LMU/RaceOS y algunos solo aparecen en un estado de sesión concreto.

## REST local de LMU

### `GET /rest/watch/standings`

Devuelve un array, con un objeto por vehículo. Campos observados:

```text
attackMode { remainingCount, timeRemaining, totalCount }
bestLapSectorTime1, bestLapSectorTime2, bestLapTime
bestSectorTime1, bestSectorTime2
carAcceleration { velocity, x, y, z }
carClass, carId, carNumber
carPosition { type, x, y, z }
carVelocity { velocity, x, y, z }
countLapFlag
currentSectorTime1, currentSectorTime2
driverName, drsActive, estimatedLapTime, finishStatus, flag, focus
fuelFraction, fullTeamName, gamePhase, hasFocus, headlights
inControl, inGarageStall
lapDistance, lapStartET
lapsBehindClassLeader, lapsBehindLeader, lapsBehindNext, lapsCompleted
lastLapTime, lastSectorTime1, lastSectorTime2
pathLateral, penalties
pitGroup, pitLapDistance, pitState, pitstops, pitting
player, position, qualification, sector, serverScored, slotID, steamID
timeBehindClassLeader, timeBehindLeader, timeBehindNext, timeIntoLap
trackEdge, underYellow, upgradePack
veFraction, vehicleFilename, vehicleName
```

Notas:

- `fuelFraction` solo fue distinto de cero para el jugador en la muestra.
- `veFraction` estuvo disponible para varios rivales y es comparable entre
  vehículos; no debe sustituirse por el combustible del jugador.
- Posición, velocidad y aceleración existen, pero la respuesta es demasiado
  lenta/pesada para reemplazar el roster de shared memory a 20/50 Hz.

### `GET /rest/watch/standings/history`

Devuelve un objeto indexado por slot/vehículo con el historial de clasificación.
Es un dato de diagnóstico/resultados y no una fuente necesaria para el render
en vivo.

### `GET /rest/watch/sessionInfo`

```text
ambientTemp, averagePathWetness, currentEventTime, darkCloud, endEventTime
gameMode, gamePhase, inRealtime, lapDistance
maxPathWetness, maxPlayers, maxTime, maximumLaps, minPathWetness
numRedLights, numberOfPlayers, numberOfVehicles
passwordProtected, playerFileName, playerName
raceCompletion { timeCompletion }
raining, sectorFlag[], serverName, serverPort, session
startEventTime, startLightFrame, timeRemainingInGamePhase
trackName, trackTemp
windSpeed { velocity, x, y, z }
yellowFlagState
```

### `GET /rest/sessions/GetGameState`

```text
MultiStintState, PitEntryDist, PitState
closeestWeatherNode { Duration, Humidity, RainChance, Sky, StartTime,
                      Temperature, WindDirection, WindSpeed }
exitGarageState, gamePhase, haveILoadedAllMyTiresYet
inControlOfVehicle, inMonitor, isReplayActive, playerVehicleLoaded
raceFinished, teamVehicleState, timeOfDay
```

`closeestWeatherNode` conserva la grafía exacta observada en la respuesta.

### `GET /rest/strategy/pitstop-estimate`

```text
brakeDucts, brakes, damage, driverSwap, fuel, penalties, tires, total, ve
```

Los servicios pueden solaparse. `total` es el tiempo oficial y no debe
recalcularse sumando las filas.

### `GET /rest/strategy/usage`

Devuelve un objeto cuyas claves dinámicas son nombres de piloto. Cada valor es un
array de vueltas con:

```text
lap, stint, pit, ve
fuel       # observado para el jugador
tyres[]    # observado para el jugador
```

La respuesta permaneció estable durante el muestreo rápido, coherente con un
historial que cambia por vuelta/evento. No representa el balance instantáneo.

### `GET /rest/strategy/overall`

Respondió `400` durante `PRACTICE1`; no se pudo inferir su esquema. Debe volver a
capturarse en carrera y durante una parada antes de considerarlo utilizable.

### `GET /rest/garage/getVehicleCondition`

```text
brakeCondition[4]
fuel, fuelCapacity
suspensionDamage[4]
tireCondition[4]
vehicleDamage
```

`suspensionDamage` coincidió con `wearables.suspension` de
`RepairAndRefuel`. `brakeCondition` no tiene la misma escala que
`wearables.brakes`, por lo que no son campos intercambiables.

### `GET /rest/garage/UIScreen/RepairAndRefuel`

Campos principales y anidados observados:

```text
currentWeather {
  airPressure, ambientTempKelvin, cloudCoverage, humidity, lightLevel,
  rainIntensity, raining, trackTempKelvin
}
fuelInfo { currentBattery, currentFuel, maxBattery, maxFuel }
pitMenu { pitMenu[] { "PMC Value", currentSetting, default, name, settings[] } }
pitRecommendations { FL TIRE, FR TIRE, RL TIRE, RR TIRE, TIRES,
                     fuel, virtualEnergy }
pitStopLength { timeInSeconds }
pitStopTimes { times { ... } }
racePosition {
  gapToFirstInClassLaps, gapToFirstInClassTime,
  gapToLastInClassLaps, gapToLastInClassTime,
  placeInClass, placeOverall
}
sessionTime { timeOfDay }
teamInfo { driverNames, teamName, vehicleName }
wearables {
  body { aero, detachableParts[] },
  brakes[4], suspension[4], tires[4]
}
weatherForecast { nodes { Humidity, RainChance, Sky, Temperature,
                          WindDirection, WindSpeed } }
```

Claves observadas dentro de `pitStopTimes.times`:

```text
BrakeChange, BrakeTimeConcurrent, DriverChange, DriverConcurrent,
DriverDamage, DriverRandom, FenderFlareAdjust, FixAeroDamage, FixAllDamage,
FixRandomDelay, FixTimeConcurrent, FourTireChange, FrontWingAdjust,
FrontWingReplace, FuelFillRate, FuelInsert, FuelRandomDelay, FuelRemove,
FuelTimeConcurrent, OnTheFlyPressure, PressureChange, RadiatorChange,
RandomBrakeDelay, RandomTireDelay, RearWingAdjust, RearWingReplace,
SimultaneousStopGo, SpringRubberChange, TireTimeConcurrent, TrackBarChange,
TwoTireChange, WedgeChange
```

### `GET /rest/watch/trackmap`

Devuelve un array de puntos:

```text
type, x, y, z
```

La captura posterior en Circuit de Barcelona devolvió 1.502 puntos. El tipo 0
formó una polilínea cerrada de 931 puntos y el tipo 1 una polilínea abierta de
279 puntos que coincidió con el pitlane. Los tipos 2–103 aparecieron como pares de
marcas de parrilla y 106–149 como pares de cajones de boxes. Track Map consume
solo 0 y 1 y valida un mínimo de puntos, por lo que un cambio de semántica vuelve
al aprendizaje local.

En Spa se observaron 1.863 puntos: 1.396 de tipo `0`, 175 de tipo `1` y dos
puntos para cada tipo `2`–`149`. La respuesta fue estática durante 20 muestras.
La semántica de 0 y 1 quedó confirmada en Barcelona; la interpretación de las
familias restantes sigue siendo preliminar y no se usa en el overlay.

### Otros GET locales cuya forma se comprobó

| Endpoint | Forma/campos principales observados |
| --- | --- |
| `/navigation/GetLoadingScreen` | `selectedCar`, `trackInfo` |
| `/navigation/state` | `loadingStatus`, `state` |
| `/rest/garage/brakeinfo` | Array numérico |
| `/rest/garage/getPlayerGarageData` | Mapa grande de parámetros `VM_*` |
| `/rest/garage/PitMenu/receivePitMenu` | Array con nombre, ajuste actual, default y settings |
| `/rest/garage/setup` | Setups con fechas, nombre y compatibilidad de vehículo |
| `/rest/garage/summary` | Setup activo, coche, circuito, comparativa y cambios sin guardar |
| `/rest/garage/tireinfo` | `frontLeft`, `frontRight`, `rearLeft`, `rearRight`, `unitSystem` |
| `/rest/garage/UIScreen/CarSetupOverview` | Setup, tiempo, clima, posición y equipo |
| `/rest/garage/UIScreen/SessionSetup` | Clases, parrilla, coche y circuito seleccionados |
| `/rest/garage/UIScreen/TireManagement` | Inventario, compuestos, wheel info, desgaste y forecast |
| `/rest/hud` | `chat`, `mfd`, `speedo`, `timing`, `trackMap` |
| `/rest/options/display` | Opciones de display/gráficos |
| `/rest/options/liveInputs` | `liveInputs` |
| `/rest/options/settings` | Mapa grande de opciones de control, sesión y gráficos |
| `/rest/profile/` | `name`, `nick`, `steamID` |
| `/rest/race/car` | Catálogo de coches, DLC, fabricante, propiedad e imágenes |
| `/rest/race/track` | Catálogo de circuitos, longitud, DLC, propiedad e imágenes |
| `/rest/sessions/weather` | Configuración meteorológica por práctica, quali y carrera |
| `/rest/watch/replays` | Repeticiones, metadata, ruta, tamaño y timestamp |

## RaceOS

### Autenticación

Flujo observado y usado por el proyecto:

1. `GET /rest/profile/getAuthSessionTicket` devuelve `authSessionTicket`.
2. `POST https://raceos.gg/authenticate` recibe `token`, `game="lmu"` y
   `platform="steam"`.
3. La respuesta contiene `accessToken`, usado temporalmente en
   `Game-Authorization: Bearer ...`.

Los valores de `authSessionTicket` y `accessToken` nunca deben persistirse ni
registrarse.

### `GET /api/v1/player`

```text
avatar, driverRank, enforcement, id, jokers, lastLogin, lastNameChange
name, platforms, profile, requiresReview, safetyRank, subscription
timezone, username
```

Es el perfil del usuario autenticado, no un roster.

### `POST /api/v1/players`

Cuerpo observado:

```json
{ "usernames": ["..."] }
```

Devuelve un array de perfiles con:

```text
id, platforms, lastLogin, lastNameChange, requiresReview
name, username, avatar, driverRank, safetyRank, profile
jokers, timezone, enforcement
```

La consulta de 25 nombres devolvió los perfiles en un único batch. Es la fuente
adecuada para DR/SR, nacionalidad y badge del roster.

### `GET /api/v1/statistics`

```text
total, career, championships, dailyRaces, specialEvents
```

### `GET /api/v1/statistics/overall`

El cliente envía un cuerpo `{ playerIds: [...] }` incluso con método GET. Sin
cuerpo respondió `404`; no se conservó un esquema de respuesta válido.

### Eventos diarios

`GET /api/v1/daily/list/{beginner|intermediate|advanced}?take=N` devolvió un
array con:

```text
id, seriesId, seriesStarts, type, teamEvent, started, lastUpdatedAt, status
configuration, registrations, raceStartsAt, raceEndsAt
splits, servers, error, history
```

Los eventos observados usaban `type="daily"`. Las tres listas completas son
demasiado pesadas para resolver periódicamente el evento actual.

`GET /api/v1/daily/schedule` devolvió:

```text
tiers, frequency
```

### `POST /api/v1/event/overview`

Cuerpo compatible con el cliente y con Dox:

```json
{
  "game": "lmu",
  "eventType": "daily",
  "eventId": "<id del evento>"
}
```

Campos principales observados:

```text
id, seriesId, seriesStarts, status, started, type, teamEvent
configuration, raceStartsAt, raceEndsAt
split, totalSplits, eventFull, registrations, registered, friends, driverRank
```

Dentro de `configuration` se observaron:

```text
cardImage, competitionMetadata, content, game, heroImage, isCompetition
openEvent, ratings, registrationOpens, seriesId, serverSize, sessions
settings, smallImage, splitSetting, starts, teamEvent, tier, title
```

Reglas de consumo para LMUOverlay:

- Es la fuente primaria para resolver el split online.
- Preferir el objeto `split` del usuario autenticado frente al roster completo
  `splits` que pueda aparecer en otras respuestas.
- Obtener `totalSplits` de la misma respuesta cuando esté disponible.
- Usar el estado local `lmu.cs.registeredEvents` solo como fallback.
- Dejar de consultar cuando split y total estén resueltos; repetir únicamente si
  cambia el event ID detectado.

En la muestra de un evento en el que el usuario no estaba registrado,
`split=null` y `totalSplits=0`; esto es un resultado contextual esperado, no una
prueba de que el endpoint sea inadecuado.

### `GET /api/v1/event/my-split/{eventType}/{eventId}`

La ruta existe en el cliente y devolvió error para un evento no registrado. Se
conserva en el inventario, pero no es la fuente elegida por LMUOverlay; la
resolución vigente usa `POST /api/v1/event/overview`.

### Otros endpoints RaceOS observados

| Endpoint | Forma/campos principales observados |
| --- | --- |
| `/api/v1/player/tos` | `termsOfServiceVer`, `competitionRulesVer` |
| `/api/v1/daily/schedule` | `tiers`, `frequency` |
| `/api/v1/event-joker/restrictions` | Límites, uso mensual y progreso para obtener joker |
| `/api/v1/championships/completed` | `championships`, `total` |
| `/api/v1/coop/event/active` | Evento, circuito, descripción, DLC, fechas y save file |
| `/api/v1/hosted` | task/region/IP/puerto, jugadores, nombre, pista, sesión y configuración |
| `/api/v1/team/livery/player` | IDs de pintura/vehículo/equipo, lineups, iconos, ficheros y metadata |
| `/api/v1/team/mine` | Equipos, lineups, miembros, enforcement y eventos completados |
| `/api/v1/maintenance` | `maintenancePeriods` |

`/api/v1/subscription` y `/api/v1/notifications/global` respondieron `404` en la
versión probada. `/api/v1/results` sin filtros estrictos devolvió aproximadamente
1,31 MB. `/api/v1/hosted/auth-token/{taskArn}` no se consultó porque devuelve
material sensible de autenticación de servidor.

## Contextos pendientes

- Capturar `event/overview` estando registrado en un evento online para guardar
  la forma interna de `split`, `totalSplits` y los parámetros DR.
- Capturar `/rest/strategy/overall` en carrera/parada.
- Decodificar los tipos de `/rest/watch/trackmap` en varios circuitos.
- Comparar `/rest/strategy/usage` con el consumo local durante varias vueltas y
  ciclos de pit.
