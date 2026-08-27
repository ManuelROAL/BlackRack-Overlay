# Timing

## Propósito

Timing reúne en un único panel el número de vuelta, tiempo actual, último,
estimado, promedio reciente, óptimo de sesión, mejor personal de sesión y mejor
personal absoluto, tres sectores y un historial corto. Evita dividir información
estrechamente relacionada en varios micro-overlays.

## Archivos y propiedad

- `timing.html`, `src/timing.ts`, `src/timing.css`: presentación.
- `src/timing-settings.ts`: tiempos visibles, longitud del historial (oculto, 3 o
  5 vueltas) y referencia de sectores (`lmu`, `session` u `overall`).
- `src-tauri/src/telemetry/delta_records.rs`: reconstrucción, referencias, sectores,
  historial y el estado de color de cada sector según la referencia elegida.
- `src/composite.ts`, `src/composite-layout.ts`: proyección y geometría compartida.
- `src-tauri/src/browser_source.rs`: ruta OBS `/timing` y preferencias reflejadas.

Rust posee toda la semántica temporal. TypeScript únicamente formatea el modelo.
El panel consume el ciclo base de 50 Hz y no construye clasificaciones.

## Comportamiento

- S1, S2 y S3 usan los cruces de sector oficiales que LMU publica para el jugador.
  El morado siempre identifica el mejor parcial global de la sesión entre todos
  los coches. El verde identifica una mejora personal según la referencia elegida:
  - `lmu` (por defecto): mejores parciales personales oficiales de LMU.
  - `session`: mejores sectores personales reconstruidos durante la sesión.
  - `overall`: mejores sectores personales absolutos persistidos.
  La comparación oficial usa los finales acumulados de S1/S2 y el tiempo de vuelta
  para S3, tal como los publica el SDK de LMU.
- Las referencias por sector (`overall`/`session`) se siguen aprendiendo en todos los
  modos aunque no se muestren, de modo que cambiar de referencia no pierde historial.
- El historial registra hasta cinco vueltas reconstruibles. El panel no muestra
  delta dinámico; el overlay Delta lo ofrece por separado cuando está visible.
- El mejor de sesión usa el mejor personal oficial de LMU; no muestra tiempos de
  otros pilotos. El mejor personal usa la vuelta absoluta persistida para la
  combinación coche/circuito.
- El promedio usa únicamente las vueltas válidas entre las cinco más recientes.
  La óptima suma los mejores sectores personales válidos aprendidos en la sesión.
- La cabecera muestra la vuelta actual y el total estimado (`actual/~total`) con
  la misma estimación de carrera usada por los demás overlays.
- La estimación proyecta la vuelta actual sobre la mejor traza disponible, con
  prioridad stint, sesión y absoluto, y suaviza el delta vivo. No usa
  `mEstimatedLapTime`, que puede anticipar tiempos irreales.
- Cada una de las siete filas de tiempo se puede ocultar de forma independiente.
  Todas están visibles por defecto; el panel reduce su altura según las filas
  activas sin ocultar los sectores ni el historial.
- La nota sobre los colores de sector aparece dentro del bloque de referencia de
  sectores, antes del separador y de las opciones de tiempos visibles.
- Las vueltas inválidas se conservan en el historial y se marcan como tales, pero
  nunca mejoran referencias de vuelta o sector. La validez retiene
  `mLapInvalidated` durante toda la vuelta y el tiempo oficial negativo confirma
  la invalidez al completarla; si LMU omite su tiempo oficial, se reconstruye con
  dos valores consecutivos de `mLapStartET`. `mCountLapFlag` no interviene.
- Al cambiar sesión se vacían historia y referencias personales de sesión. Los
  mejores absolutos permanecen en `lap-records.sqlite3`.
- Durante la salida de boxes, `ACTUAL` permanece sin valor. LMU
  conserva entonces un `mLapStartET` anterior que no representa el tiempo de la
  outlap; el contador empieza al primer paso por meta, cuando BlackRack observa
  el inicio real de una vuelta cronometrada.
- Una outlap tampoco rellena parciales de sector. El marcador `1.000` que resulta
  del valor oficial no inicializado `-1` de LMU se descarta; en una vuelta
  cronometrada invalidada se conserva en su lugar el tiempo real del cruce y se
  presenta con el estado inválido.
- El panel usa una superficie compacta de 250 px de ancho, separaciones reducidas
  y el acento lima vertical izquierdo compartido con los demás overlays compactos.
  Al cambiar entre 0, 3 o 5 vueltas, conserva la escala visual elegida. Un texto
  mayor añade solo la altura de línea necesaria, sin ensanchar el panel.

## Invariantes

- No depende de que el overlay Delta esté visible ni de su referencia configurada.
- Una vuelta debe cumplir los mismos controles de reconstrucción que Delta antes
  de incorporarse al historial.
- El historial aprendido no forma parte de importar/exportar configuración; sólo
  se exportan los tiempos visibles, las filas de historial y la referencia de sectores.
- Las referencias por sector se siguen aprendiendo en todos los modos de referencia.

## Verificación enfocada

- `cargo test --manifest-path src-tauri\Cargo.toml --lib` cubre reconstrucción,
  persistencia compartidas y la clasificación de estado por referencia.
- `npm.cmd run build` cubre las entradas independiente, compuesta y OBS.
- En LMU, validar cruces S1/S2/meta, vuelta inválida, reinicio de sesión, recarga
  del mejor absoluto tras reiniciar la aplicación y el cambio de referencia
  (`lmu`/`session`/`overall`) con el panel de control.

## Localización

Las etiquetas estáticas, el historial, el título y el texto accesible usan el
idioma incluido seleccionado. Las opciones del selector de referencia y la nota de
configuración también se traducen. Los tiempos conservan su notación compacta.
La ruta OBS usa el idioma compartido guardado o un override local `?lang=`.
