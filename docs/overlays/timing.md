# Timing compacto

## Propósito

Timing compacto reúne en un único panel el tiempo actual, último y mejor de
sesión, tres sectores y un historial corto. Evita dividir información
estrechamente relacionada en varios micro-overlays.

## Archivos y propiedad

- `timing.html`, `src/timing.ts`, `src/timing.css`: presentación.
- `src/timing-settings.ts`: longitud visible del historial (oculto, 3 o 5 vueltas) y
  referencia de sectores (`lmu`, `session` u `overall`).
- `src-tauri/src/telemetry/delta_records.rs`: reconstrucción, referencias, sectores,
  historial y el estado de color de cada sector según la referencia elegida.
- `src/composite.ts`, `src/composite-layout.ts`: proyección y geometría compartida.
- `src-tauri/src/browser_source.rs`: ruta OBS `/timing` y preferencias reflejadas.

Rust posee toda la semántica temporal. TypeScript únicamente formatea el modelo.
El panel consume el ciclo base de 50 Hz y no construye clasificaciones.

## Comportamiento

- S1, S2 y S3 usan los cruces de sector oficiales que LMU publica para el jugador.
  El color de cada sector depende de la referencia configurada:
  - `lmu` (por defecto): usa el delta nativo de LMU (`mDeltaBest`, delta de vuelta
    frente a la mejor vuelta). Los sectores cruzados se marcan en verde mientras la
    vuelta en curso mejora su mejor (`mDeltaBest < 0`); en caso contrario quedan
    neutros. Al ser un valor de vuelta, los tres sectores comparten la tendencia.
  - `session`: un sector verde mejora el mejor de sesión; uno morado mejora el mejor
    absoluto persistido.
  - `overall`: solo el morado, cuando el cruce mejora el mejor sector absoluto
    persistido.
- Las referencias por sector (`overall`/`session`) se siguen aprendiendo en todos los
  modos aunque no se muestren, de modo que cambiar de referencia no pierde historial.
- El historial registra hasta cinco vueltas reconstruibles. El panel no muestra
  delta dinámico; el overlay Delta lo ofrece por separado cuando está visible.
- Las vueltas inválidas se conservan en el historial y se marcan como tales, pero
  nunca mejoran referencias de vuelta o sector.
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
- El panel usa una superficie compacta de 318 px de ancho, separaciones reducidas
  y el acento lima vertical izquierdo compartido con los demás overlays compactos.
  Al cambiar entre 0, 3 o 5 vueltas, conserva la escala visual elegida.

## Invariantes

- No depende de que el overlay Delta esté visible ni de su referencia configurada.
- Una vuelta debe cumplir los mismos controles de reconstrucción que Delta antes
  de incorporarse al historial.
- El historial aprendido no forma parte de importar/exportar configuración; sólo
  se exportan las preferencias visuales de filas y la referencia de sectores.
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
