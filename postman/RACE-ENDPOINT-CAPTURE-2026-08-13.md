# Captura de endpoints durante carrera — 13 de agosto de 2026

## Contexto

Captura realizada durante `RACE1`, en tiempo real, con 38 vehículos en Monza. El
trace de LMU confirmó que correspondía a un evento online registrado.

Solo se hicieron lecturas del REST local y consultas de datos de RaceOS. El
ticket local y el access token se mantuvieron en memoria. Este documento no
contiene nombres, IDs de pilotos, Steam IDs, event ID, hosts, contraseñas ni
tokens de servidor.

## REST local durante carrera

| Endpoint | Estado | Tamaño | Latencia puntual | Observación |
| --- | ---: | ---: | ---: | --- |
| `/rest/strategy/overall` | 200 | 14.436 B | 41 ms | Disponible en carrera; había respondido 400 en práctica. |
| `/rest/strategy/usage` | 200 | 6.093 B | 27 ms | Historial de 38 pilotos. |
| `/rest/strategy/pitstop-estimate` | 200 | 121 B | 29 ms | Estimación oficial activa. |
| `/rest/watch/standings` | 200 | 72.578 B | 91 ms | 38 vehículos y campos ya documentados. |
| `/rest/watch/standings/history` | 200 | 52.784 B | 98 ms | 38 slots con historial de vueltas. |
| `/rest/watch/sessionInfo` | 200 | 873 B | 27 ms | Confirmó `RACE1`, realtime y 38 vehículos. |
| `/rest/sessions/GetGameState` | 200 | 474 B | 28 ms | Estado vivo de carrera, vehículo, pit y clima. |

Las latencias son una sola muestra y no sustituyen la medición repetida del
informe principal.

### `/rest/strategy/overall`

La respuesta es un array con una entrada por piloto. Cada entrada es una tupla:

```text
[
  "<clave dinámica de piloto>",
  [ {
      driver, driverSwap, lap, penalty, previousStintDuration, time, ve,
      tyres: {
        fl: { changed, compound, new },
        fr: { changed, compound, new },
        rl: { changed, compound, new },
        rr: { changed, compound, new }
      }
  } ]
]
```

Había 38 entradas y una fila de stint por entrada. Es un resumen de
stint/estrategia, no una fuente de alta frecuencia. La clave exterior y `driver`
contienen identidad y no deben escribirse en logs diagnósticos.

### `/rest/strategy/usage`

Había 38 claves dinámicas de piloto. La forma seguía siendo:

```text
<driver>: [ { lap, pit, stint, ve } ]
```

Los campos opcionales `fuel` y `tyres[]` pueden aparecer para el jugador. Es
historial por vuelta, no balance actual.

### `/rest/watch/standings/history`

El objeto tenía 38 claves de slot. Cada una contenía un array con:

```text
carClass, driverName, finishStatus, lapTime, pitting, position
sectorTime1, sectorTime2, slotID, totalLaps, vehicleName
```

Confirma su utilidad para recuperación tardía de parrilla, roster inicial y
vueltas válidas. No proporciona el instante exacto de entrada a boxes y
`lapTime = -1` sigue significando una vuelta sin duración utilizable.

## RaceOS durante un evento registrado

### `POST /api/v1/event/overview`

Cuerpo usado:

```json
{
  "game": "lmu",
  "eventType": "daily",
  "eventId": "<event ID detectado en el trace>"
}
```

Respondió `200` en 153 ms. Campos de primer nivel:

```text
configuration, driverRank, eventFull, friends, id
raceEndsAt, raceStartsAt, registered, registrations
seriesId, seriesStarts, split, started, status
teamEvent, totalSplits, type
```

No apareció un campo de primer nivel `splits`. El objeto autenticado `split`
tuvo esta forma resumida:

```text
splitNo
drivers[] {
  registeredAt,
  driver {
    id, steamId, name, username, avatar,
    driverRank { elo, rank, tier, progress },
    safetyRank { rank, tier, rating, progress, contactImpactsPerLapRatio },
    profile { nationality, badge }
  },
  team {
    teamId, teamName, nationality, teamNumbers[], paintId,
    lineup {
      id, default, inactive, name, icon,
      drivers[] {
        state, default, driverId, name, nationality, badge,
        driverRank { elo, rank, tier, progress },
        safetyRank { rank, tier, rating, progress, contactImpactsPerLapRatio },
        avatar, roles[]
      },
      rating { elo, rank, tier, progress },
      safety { rank, tier, rating, progress, contactImpactsPerLapRatio },
      completedEvents
    },
    icons { logo, card, background }
  },
  class, manufacturer,
  car { workshopId, itemDefId, name, friendly, sig, version,
        subItems[] { name, veh } },
  paintId, carImage { small, large }
}
server {
  status, type, arn, host, port, agentPort, password,
  error, splitNo, endsAt, failures, firstFailure
}
sof { elo, rank, tier, progress }
```

`split.drivers[]` contenía únicamente el registro autenticado en esta respuesta,
no los 38 participantes. Esto confirma que `split.splitNo` es el dato primario
del usuario y no debe inferirse desde un supuesto roster completo de overview.

La respuesta indicó 8 splits. La configuración contenía:

```text
drSettings { base: 1, k: 45, d: 500, log: 10 }
```

Son valores del evento capturado, no constantes globales. El resolver debe usar
defaults cuando no estén disponibles.

El objeto incluye datos personales y campos de conexión del servidor. Solo deben
extraerse los datos necesarios; nunca se debe registrar ni persistir la respuesta
completa.

### `GET /api/v1/event/my-split/daily/{eventId}`

Respondió `200` en 103 ms. Forma observada:

```text
eventId, splitNo, numOfSplits
drivers[] {
  id, name,
  driverRank { rank, tier }, safetyRank { rank, tier }, carClass,
  car { workshopId, itemDefId, name, friendly, sig, version,
        subItems[] { name, veh } }
}
server { taskArn, host, port, status, password, auth }
sof { elo, rank, tier, progress }
```

Aquí `drivers[]` sí contenía los 38 integrantes del split. `splitNo` y
`numOfSplits` llegaron como strings, no como números JSON; el parser debe aceptar
ambas representaciones.

Es un fallback read-only adecuado si overview falla o no incluye el split
autenticado. También contiene credenciales/campos sensibles del servidor, por lo
que nunca debe guardarse o registrarse completo.

## Decisión confirmada

1. Detectar el event ID desde el trace de LMU.
2. Consultar `POST /api/v1/event/overview` y preferir `split.splitNo` junto con
   `totalSplits` y `drSettings`.
3. Si overview falla o no contiene el split autenticado, consultar
   `GET /api/v1/event/my-split/daily/{eventId}`.
4. Usar `lmu.cs.registeredEvents` como último fallback.
5. Dejar de consultar cuando número y total estén resueltos, salvo que cambie el
   event ID.
