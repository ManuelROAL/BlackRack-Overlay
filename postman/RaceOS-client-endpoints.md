# Endpoints de RaceOS referenciados por LMU

Inventario extraído el 5 de agosto de 2026 del cliente web incluido con Le Mans
Ultimate:

`Bin/UI.zip -> start/assets/app-DqtBeqOS.js`

El propio cliente define el host de producción `raceos.gg`, obtiene el ticket en
`/rest/profile/getAuthSessionTicket`, autentica mediante `/authenticate` y envía el
token resultante en la cabecera `Game-Authorization: Bearer <token>`.

Este documento recoge todas las rutas que aparecen en esa versión del cliente. No
demuestra que sean todos los endpoints implementados en el servidor. RaceOS no publica
un Swagger/OpenAPI accesible y algunas rutas pueden cambiar, requerir permisos
adicionales o aceptar estructuras que no están documentadas públicamente.

## Autenticación y conexión

| Método | Ruta | Observación |
| --- | --- | --- |
| `POST` | `/authenticate` | Autenticación normal con ticket de la plataforma. |
| `POST` | `/authenticate/steam` | Variante mediante Steam OpenID. |
| `POST` | `/create-account` | Crea una cuenta. Modifica datos. |
| `WS` | `/ws` | Notificaciones; utiliza el subprotocolo `access_token` seguido del token. |

## Jugador, perfil y estadísticas

| Método | Ruta |
| --- | --- |
| `GET` | `/api/v1/player` |
| `POST` | `/api/v1/player` |
| `PATCH` | `/api/v1/player/badge` |
| `POST` | `/api/v1/player/email-resend` |
| `POST` | `/api/v1/player/friends-list` |
| `POST` | `/api/v1/player/timezone` |
| `GET` | `/api/v1/player/tos` |
| `POST` | `/api/v1/players` |
| `GET` | `/api/v1/statistics` |
| `GET` | `/api/v1/statistics/overall` |
| `POST` | `/api/v1/statistics/overall` |
| `GET` | `/api/v1/subscription` |
| `POST` | `/api/v1/profanity/check` |
| `POST` | `/api/v1/protest` |
| `GET` | `/api/v1/steam/app-info/{appId}` |

## Eventos y resultados

| Método | Ruta |
| --- | --- |
| `GET` | `/api/v1/daily/list/{eventType}?take={take}` |
| `GET` | `/api/v1/daily/schedule` |
| `GET` | `/api/v1/event/entries/{eventType}/{eventId}?page={page}&take={take}&class={class}` |
| `GET` | `/api/v1/event/my-split/{eventType}/{eventId}` |
| `POST` | `/api/v1/event/overview` |
| `POST` | `/api/v1/event/register` |
| `POST` | `/api/v1/event/register/engineer` |
| `DELETE` | `/api/v1/event/unregister` |
| `DELETE` | `/api/v1/event/unregister/engineer` |
| `GET` | `/api/v1/event-joker/restrictions` |
| `POST` | `/api/v1/event-joker/play` |
| `GET` | `/api/v1/result?{query}` |
| `GET` | `/api/v1/results?{query}` |
| `GET` | `/api/v1/special-events/{eventType}` |
| `GET` | `/api/v1/practice/servers/{type}/{tier}?eventTitle={eventTitle}` |
| `GET` | `/api/v1/practice/pro/servers/{type}/{tier}?eventTitle={eventTitle}` |

## Campeonatos

| Método | Ruta |
| --- | --- |
| `GET` | `/api/v1/championship/{championshipId}` |
| `GET` | `/api/v1/championships/{championshipId}/practice/{week}` |
| `GET` | `/api/v1/championships/active?esports={boolean}` |
| `GET` | `/api/v1/championships/completed?page={page}&take={take}` |
| `GET` | `/api/v1/championships/summary/{championshipId}` |
| `GET` | `/api/v1/championships/upcoming?esports={boolean}` |
| `POST` | `/api/v1/championships/register` |
| `POST` | `/api/v1/championships/unregister` |

## Clasificaciones

| Método | Ruta |
| --- | --- |
| `GET` | `/api/v1/leaderboards/{name}?page={page}&take={take}` |
| `GET` | `/api/v1/leaderboards/around-me/{name}?take={take}` |
| `GET` | `/api/v1/leaderboards/friends/{name}?type={type}&page={page}&take={take}` |
| `POST` | `/api/v1/leaderboards/results-for-users` |

## Cooperativo

| Método | Ruta |
| --- | --- |
| `PUT` | `/api/v1/coop` |
| `GET` | `/api/v1/coop/{sessionId}` |
| `DELETE` | `/api/v1/coop/{sessionId}` |
| `GET` | `/api/v1/coop/{sessionId}/active-driver` |
| `PUT` | `/api/v1/coop/{sessionId}/active-driver` |
| `DELETE` | `/api/v1/coop/{sessionId}/active-driver` |
| `PATCH` | `/api/v1/coop/{sessionId}/chat` |
| `POST` | `/api/v1/coop/create-rc-event` |
| `GET` | `/api/v1/coop/event/{id}` |
| `GET` | `/api/v1/coop/event/active?page=1&take=5` |
| `POST` | `/api/v1/coop/event/check-ownership` |
| `GET` | `/api/v1/coop/event/leaderboard/around-us?eventName={eventName}&sessionId={sessionId}&take={take}` |
| `GET` | `/api/v1/coop/event/leaderboard/list?eventName={eventName}&page={page}&take={take}` |
| `GET` | `/api/v1/coop/historic?page=1&take=15&type={type}` |
| `GET` | `/api/v1/coop/invites` |
| `POST` | `/api/v1/coop/invites/accept` |
| `POST` | `/api/v1/coop/invites/decline` |
| `POST` | `/api/v1/coop/join` |
| `POST` | `/api/v1/coop/leave` |
| `GET` | `/api/v1/coop/list?page=1&take=4&type={type}` |
| `POST` | `/api/v1/coop/status/{sessionId}/{status}` |
| `POST` | `/api/v1/coop/upload` |

## Servidores alojados

| Método | Ruta |
| --- | --- |
| `GET` | `/api/v1/hosted` |
| `GET` | `/api/v1/hosted/auth-token/{taskArn}` |
| `GET` | `/api/v1/hosted/extended/{taskArn}` |
| `GET` | `/api/v1/hosted/lobby-code/{lobbyCode}` |

## Equipos y liveries

| Método | Ruta |
| --- | --- |
| `POST` | `/api/v1/team` |
| `GET` | `/api/v1/team/{teamId}` |
| `PUT` | `/api/v1/team/{teamId}` |
| `GET` | `/api/v1/team/events/{teamId}` |
| `POST` | `/api/v1/team/invite` |
| `PUT` | `/api/v1/team/invite` |
| `DELETE` | `/api/v1/team/invite` |
| `GET` | `/api/v1/team/invites` |
| `POST` | `/api/v1/team/kick` |
| `POST` | `/api/v1/team/leave` |
| `POST` | `/api/v1/team/lineup` |
| `PUT` | `/api/v1/team/lineup` |
| `POST` | `/api/v1/team/lineup/driver` |
| `DELETE` | `/api/v1/team/lineup/driver` |
| `GET` | `/api/v1/team/lineups/namecheck/{lineupName}` |
| `GET` | `/api/v1/team/livery/{teamId}` |
| `DELETE` | `/api/v1/team/livery` |
| `POST` | `/api/v1/team/livery/copy` |
| `GET` | `/api/v1/team/livery/lineup/{teamId}/{lineupId}` |
| `GET` | `/api/v1/team/livery/player` |
| `POST` | `/api/v1/team/member/roles` |
| `DELETE` | `/api/v1/team/member/roles` |
| `GET` | `/api/v1/team/mine` |
| `GET` | `/api/v1/team/namecheck/{teamName}` |
| `GET` | `/api/v1/team/results/{teamId}?lineupId={lineupId}&eventType={eventType}&page={page}&take={take}` |
| `GET` | `/api/v1/team/stats/{teamId}` |
| `POST` | `/api/v1/team/token/image` |
| `POST` | `/api/v1/team/token/paint` |
| `POST` | `/api/v1/team/transfer` |

## Notificaciones y servicio

| Método | Ruta |
| --- | --- |
| `DELETE` | `/api/v1/notification` |
| `PATCH` | `/api/v1/notification/read/{id}` |
| `GET` | `/api/v1/notifications/global` |
| `GET` | `/api/v1/maintenance` |

## Uso seguro

Las rutas `POST`, `PUT`, `PATCH` y `DELETE` pueden inscribir al jugador, abandonar
eventos, modificar perfiles, gestionar equipos o enviar datos. No deben ejecutarse para
explorar la API. Para diagnóstico, utiliza únicamente rutas `GET` con tu propia sesión y
respeta los límites del servicio.
