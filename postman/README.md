# Colección de Postman de LMUOverlay

## Importación

1. Importa `LMUOverlay.postman_collection.json` en Postman.
2. Importa `LMUOverlay.local.postman_environment.json`.
3. Selecciona el entorno **LMUOverlay - Local**.
4. Abre Le Mans Ultimate e inicia sesión.
5. Ejecuta `1.1 Obtener ticket local de LMU`.
6. Ejecuta `1.2 Autenticar en RaceControl`.
7. Ejecuta cualquiera de las peticiones de la carpeta `2. Datos`.

Los scripts de las dos primeras peticiones guardan automáticamente
`auth_session_ticket` y `racecontrol_access_token` en el entorno activo.

## Variables

- `driver_usernames_json`: array JSON con los pilotos que se consultarán de una vez.
- `event_id`: UUID del evento online necesario para consultar el split.
- `auth_session_ticket`: ticket temporal obtenido de la API local de LMU.
- `racecontrol_access_token`: token temporal obtenido de RaceControl.

## Seguridad

Los archivos entregados no contienen tickets, tokens ni claves privadas. No exportes el
entorno después de usarlo con los valores secretos incluidos y no compartas los tokens.
Las rutas de RaceControl no son una API pública documentada y pueden cambiar o aplicar
límites sin previo aviso; usa la colección únicamente con tu propia sesión de LMU.

## Inventario completo de RaceOS

- `RaceOS-client-endpoints.md` documenta las rutas encontradas en el cliente de LMU.
- `RaceOS.complete.postman_collection.json` contiene todas esas rutas.
- `RaceOS.complete.postman_environment.json` contiene sus variables.
- `ENDPOINT-AUDIT-2026-08-12.md` compara respuestas reales, coste y fuente
  recomendada para los datos usados por LMUOverlay.
- `OBSERVED-RESPONSE-FIELDS-2026-08-12.md` conserva los campos y formas de
  respuesta observados sin incluir valores personales ni credenciales.

Las operaciones que no sean de lectura están bloqueadas inicialmente. Para ejecutar
conscientemente una operación que modifica datos habría que cambiar `allow_mutations` a
`true` y proporcionar el cuerpo correcto en `request_body_json`. La conexión `/ws` se
incluye únicamente como referencia porque el formato de colecciones HTTP de Postman no
representa una petición WebSocket completa.
