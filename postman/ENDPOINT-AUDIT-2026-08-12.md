# Auditoría de endpoints de LMU y RaceOS — 12 de agosto de 2026

## Alcance

La prueba se hizo con la UI de Le Mans Ultimate instalada el 11 de agosto de
2026, una sesión `PRACTICE1` en Spa y 25 vehículos. Se consultó el Swagger local
de LMU, el inventario de rutas del cliente de RaceOS y respuestas reales de
ambos servicios.

No se ejecutaron operaciones capaces de registrar o desregistrar al jugador,
alterar perfil, equipos, configuraciones, repeticiones o estado del juego. Los
tickets y tokens temporales se mantuvieron en memoria y no se incluyeron en los
resultados.

## Inventario

### REST local de LMU

`http://127.0.0.1:6397/swagger-schema.json` anunció 179 rutas y 187 operaciones:

| Método | Operaciones |
| --- | ---: |
| GET | 79 |
| POST | 94 |
| PUT | 13 |
| DELETE | 1 |

Setenta GET no tenían parámetros obligatorios. Se probaron 67 de ellos; se
excluyeron el ticket de autenticación, `resetVRView` y el comodín `/webdata/.*`.
El resultado fue 64 respuestas `200`, dos `404` dependientes del contexto
(`CoopOverview` y `SaveLoad/getSaveJSON`) y un `400` en
`/rest/strategy/overall` durante la sesión de práctica.

El Swagger es el inventario completo de la versión local en ejecución, aunque
solo documenta método y ruta: no incluye esquemas de respuesta útiles. Para
conocer los campos fue necesario inspeccionar respuestas reales.

### RaceOS

El cliente de LMU referencia 106 operaciones de RaceOS:

| Método | Operaciones |
| --- | ---: |
| GET | 51 |
| POST | 37 |
| PUT | 5 |
| PATCH | 3 |
| DELETE | 9 |
| WebSocket | 1 |

RaceOS no publica un Swagger/OpenAPI accesible. Se probaron las lecturas sin
efectos laterales que podían resolverse con la sesión actual y algunos POST que
el propio cliente usa como consultas (`/api/v1/players` y
`/api/v1/event/overview`). Las rutas que necesitan un campeonato, equipo,
lineup, sesión cooperativa, servidor o evento concreto solo pueden validarse
cuando existe ese contexto. No se sondearon mutaciones con cuerpos inventados.

## REST local: datos relevantes y elección de fuente

| Endpoint | Datos observados | Evaluación |
| --- | --- | --- |
| `/rest/watch/standings` | Identidad, número asignado, clase, posiciones, vueltas, sectores, GAP de LMU, pit, penalizaciones, bandera, combustible y VE por coche, posición/velocidad 3D | Mantener como suplemento de Standings a 1 Hz. La respuesta fue de ~45 KB y cambió 7 veces en 20 muestras a 20 Hz; no compensa usarla como telemetría rápida frente a shared memory. |
| `/rest/watch/standings/history` | Historial de clasificación por slot | Útil para diagnóstico o resultados, no para el render en vivo. |
| `/rest/watch/sessionInfo` | Sesión, fase, tiempos, vueltas máximas, lluvia, humedad de pista, temperaturas, viento, banderas de sector y metadatos del servidor | Buen suplemento para datos de sesión o meteorología que falten, pero shared memory sigue siendo mejor para fase y banderas en tiempo real. |
| `/rest/strategy/pitstop-estimate` | `fuel`, `ve`, neumáticos, frenos, conductos, daños, cambio de piloto, penalizaciones y `total` | Es la mejor fuente para la estimación oficial. `total` debe seguir siendo autoritativo. Solo ~150 B y ~2,2 ms de mediana. |
| `/rest/strategy/usage` | Historial por piloto y vuelta: `lap`, `stint`, `pit`, VE; combustible y neumáticos aparecieron solo para el jugador | Es historial por vuelta, no estado actual. Sirve para validar consumo o estudiar rivales, no para reemplazar el cálculo local ni como fallback de `NRG`. |
| `/rest/strategy/overall` | No disponible en esta práctica (`400`) | Dependiente del tipo/estado de sesión; no debe ser una dependencia del overlay hasta capturarlo en carrera. |
| `/rest/garage/UIScreen/RepairAndRefuel` | Clima, combustible/batería, menú y recomendaciones de pit, tiempos internos de servicio, posición, equipo, forecast y `wearables` de carrocería, frenos, suspensión y neumáticos | Mantener para daño aerodinámico y suspensión. Es la única respuesta probada que separa `body.aero`; ~10,9 KB y ~5,1 ms de mediana son aceptables a 1 Hz. |
| `/rest/garage/getVehicleCondition` | Condición de frenos y neumáticos, combustible/capacidad, suspensión y daño agregado | Mucho más pequeño (~259 B), y la suspensión coincidió con `RepairAndRefuel`, pero no expone daño aero y la condición de frenos tiene otra semántica. No sustituye por sí solo al endpoint actual. |
| `/rest/garage/UIScreen/TireManagement` | Inventario, compuestos, wheel info, desgaste, forecast y recomendaciones | Rico para una pantalla de garaje, demasiado pesado y orientado al jugador para el overlay vivo. Shared memory es mejor para temperaturas, presión, desgaste y estado instantáneo. |
| `/rest/watch/trackmap` | Puntos 3D estáticos: tipo 0 es el trazado principal ordenado y tipo 1 el pitlane abierto; las familias restantes representan marcas de parrilla/boxes en las muestras estudiadas | Adoptado para Track Map como carga única por circuito, con validación y caché. Se transfiere fuera del roster de 20 Hz y conserva la vuelta aprendida como fallback. Pesa ~147 KB y tarda ~8 ms; nunca debe sondearse periódicamente. |
| `/rest/options/liveInputs` | Estado de entradas de control | No cambió durante la muestra y es más lento/pesado que shared memory. No usar para pedales o dirección. |
| `/rest/sessions/GetGameState` | Control del vehículo, monitor/replay, fase, pit, hora y nodo meteorológico próximo | Útil para diagnóstico de estado de UI; no mejora la ruta caliente de shared memory. |

### Medición repetida de endpoints candidatos

Cada ruta se consultó 20 veces con 50 ms entre muestras y conexión reutilizada:

| Endpoint | Tamaño medio | p50 | p95 | Respuestas distintas |
| --- | ---: | ---: | ---: | ---: |
| `/rest/watch/standings` | 45.641 B | 1,41 ms | 3,03 ms | 7/20 |
| `/rest/watch/standings/history` | 13.554 B | 0,59 ms | 1,79 ms | 3/20 |
| `/rest/watch/sessionInfo` | 869 B | 1,68 ms | 2,97 ms | 20/20 |
| `/rest/watch/trackmap` | 147.071 B | 8,03 ms | 12,13 ms | 1/20 |
| `/rest/strategy/usage` | 3.799 B | 1,62 ms | 3,54 ms | 1/20 |
| `/rest/strategy/pitstop-estimate` | 150 B | 2,20 ms | 3,59 ms | 20/20 |
| `/rest/garage/getVehicleCondition` | 259 B | 1,80 ms | 3,24 ms | 20/20 |
| `/rest/garage/UIScreen/RepairAndRefuel` | 10.951 B | 5,11 ms | 9,20 ms | 20/20 |
| `/rest/options/liveInputs` | 2.346 B | 3,28 ms | 5,78 ms | 1/20 |

Que una respuesta sea distinta no garantiza que haya cambiado el dato de
interés; por ejemplo, `sessionInfo` incluye tiempo de sesión. La medida sí
permite distinguir respuestas estáticas o por vuelta de fuentes candidatas para
actualización frecuente.

## RaceOS: datos y elección de endpoint

| Endpoint | Resultado observado | Evaluación |
| --- | --- | --- |
| `GET /api/v1/player` | Perfil completo del usuario actual, DR, SR, avatar, plataforma, jokers, enforcement y suscripción; ~1,4 KB y 66 ms | Mejor para el propio usuario, no para enriquecer un roster. |
| `POST /api/v1/players` | Los 25 perfiles del roster en una petición; DR, SR, nacionalidad/perfil, avatar y enforcement; ~21,7 KB y 216 ms | Sigue siendo la mejor ruta para Standings: una petición por roster y caché por sesión. |
| `GET /api/v1/statistics` | Totales, carrera, campeonatos, carreras diarias y eventos especiales; ~2,3 KB | Interesante para una futura vista de perfil, no para overlays en carrera. |
| `GET /api/v1/statistics/overall` | El cliente envía `{ playerIds }` incluso siendo GET; sin cuerpo respondió `404` | No aporta ventaja frente a `/players` para DR/SR y exige una petición no convencional. |
| `GET /api/v1/daily/list/{beginner|intermediate|advanced}` | Eventos, configuración, registros, servidores, splits e historial; 115–325 KB por categoría y 201–286 ms | Demasiado pesado para resolver el evento actual periódicamente. Solo usar en una pantalla de calendario. |
| `GET /api/v1/daily/schedule` | Tiers y frecuencia; ~11,5 KB | Adecuado para calendario, no para el overlay. |
| `GET /api/v1/event/my-split/{type}/{id}` | Split del jugador para un evento concreto; devuelve error si el jugador no pertenece al evento | Ruta observada, pero no es la fuente elegida por LMUOverlay. Se conserva para comparación manual. |
| `POST /api/v1/event/overview` | Configuración completa, registros, amigos, rating y campos `split`/`totalSplits`; ~7,3 KB y 162 ms | Fuente primaria vigente para el split. Usa el cuerpo Dox-compatible `game`/`eventType`/`eventId`, prefiere el objeto `split` del usuario autenticado y mantiene el estado local de LMU como fallback. En un evento no registrado es normal obtener `split=null`. |
| `GET /api/v1/championships/completed` | Resumen paginado; ~162 KB | Solo para una futura UI de campeonatos. |
| `GET /api/v1/results` | Respuesta de ~1,31 MB y ~1 s en la consulta genérica | Evitar en runtime; requiere filtros estrictos y uso interactivo. |
| `GET /api/v1/hosted` | Servidores, región, IP/puerto, sesión, pista y configuración; ~184 KB | No tiene valor para los overlays actuales. No consultar `hosted/auth-token` para explorar: devuelve material de autenticación de servidor. |
| `GET /api/v1/team/mine` y `team/livery/player` | Equipos, lineups, miembros, liveries y ficheros asociados | Solo para funciones futuras de equipos/liveries; no aporta telemetría. |
| `GET /api/v1/maintenance` | Periodos de mantenimiento; ~104 B | Útil solo para diagnóstico de conectividad RaceOS. |

En esta versión, `/api/v1/subscription` y
`/api/v1/notifications/global` respondieron `404`; la información de suscripción
ya apareció dentro de `/api/v1/player`. Deben tratarse como rutas obsoletas o
dependientes de despliegue, no como nuevas dependencias.

## Recomendación final por dominio

| Dominio | Fuente recomendada | Motivo |
| --- | --- | --- |
| Telemetría rápida, controles, tiempos, posiciones, flags, combustible/VE y neumáticos | Shared memory | 50 Hz, menor coste y fuente oficial autoritativa. |
| Número asignado, qualification, estado de pit/finish y suplemento comparable de rivales | `/rest/watch/standings` a 1 Hz | Aporta campos que faltan o son más fiables sin cargar la ruta caliente. |
| Duración de parada | `/rest/strategy/pitstop-estimate` a 1 Hz | Endpoint específico, pequeño y con `total` oficial. |
| Aero y suspensión detallada del jugador | `/rest/garage/UIScreen/RepairAndRefuel` a 1 Hz | Única fuente probada con daño aero separado. |
| DR/SR, nacionalidad y badge del roster | RaceOS `POST /api/v1/players`, una vez por roster y con caché | Batch específico; evita una petición por piloto. |
| Split online | RaceOS `POST /api/v1/event/overview`, una vez resuelto | Devuelve el objeto `split` autenticado, `totalSplits` y parámetros del evento con el cuerpo compatible con Dox. El estado local de LMU queda como fallback. |
| Geometría de circuito | `/rest/watch/trackmap` una vez por circuito | Tipo 0 alimenta el trazado oficial y tipo 1 valida recorridos completos de pitlane; el backend filtra, valida y cachea la respuesta. |
| Historial de consumo rival | Investigar `/rest/strategy/usage` solo por vuelta | Tiene datos únicos, pero no representa el estado actual y no debe alimentar `NRG`. |

La arquitectura actual elige correctamente las fuentes principales. Los dos
hallazgos fueron `watch/trackmap`, ya incorporado como geometría oficial de carga
única, y `strategy/usage` como historial de consumo todavía no adoptado. Ninguno
forma parte del loop de 50 Hz.

## Validaciones pendientes por contexto

1. Repetir `event/overview` durante un evento online en el que el jugador esté
   registrado y guardar la forma interna de `split`, `totalSplits` y parámetros
   DR. `my-split` puede compararse manualmente, pero no es la fuente vigente.
2. Capturar `/rest/strategy/overall` durante carrera y durante una parada para
   documentar su esquema y su cadencia real.
3. Confirmar en más circuitos que los tipos 0 y 1 mantienen la semántica observada;
   la implementación debe seguir validando y volver al mapa aprendido si no se cumple.
4. Comparar `strategy/usage` con el aprendizaje local al completar varias vueltas,
   incluyendo stint, pit-in, pit-out y cambio de piloto.
