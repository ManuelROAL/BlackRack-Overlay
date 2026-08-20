# Timing compacto

## Propósito

Timing compacto reúne en un único panel el delta dinámico contra la mejor vuelta
global conocida, el tiempo actual, último y mejor de sesión, tres sectores y un
historial corto. Evita dividir información estrechamente relacionada en varios
micro-overlays.

## Archivos y propiedad

- `timing.html`, `src/timing.ts`, `src/timing.css`: presentación.
- `src/timing-settings.ts`: longitud visible del historial (oculto, 3 o 5 vueltas).
- `src-tauri/src/telemetry/delta_records.rs`: reconstrucción, referencias, sectores,
  historial y congelación.
- `src/composite.ts`, `src/composite-layout.ts`: proyección y geometría compartida.
- `src-tauri/src/browser_source.rs`: ruta OBS `/timing` y preferencias reflejadas.

Rust posee toda la semántica temporal. TypeScript únicamente formatea el modelo.
El panel consume el ciclo base de 50 Hz y no construye clasificaciones.

## Comportamiento

- El delta compara la vuelta en curso con la mejor vuelta global persistida para
  el mismo circuito, coche y longitud de pista.
- S1, S2 y S3 usan los cruces de sector oficiales que LMU publica para el jugador.
  Un sector verde mejora el mejor de la sesión; uno morado mejora el mejor
  absoluto persistido.
- El resultado del delta se congela durante 2 segundos al cruzar S1 o S2. La
  congelación de meta reutiliza la referencia de vuelta y el historial registra
  hasta cinco vueltas reconstruibles.
- Las vueltas inválidas se conservan en el historial y se marcan como tales, pero
  nunca mejoran referencias de vuelta o sector.
- Al cambiar sesión se vacían historia y referencias personales de sesión. Los
  mejores absolutos permanecen en `lap-records.sqlite3`.

## Invariantes

- No depende de que el overlay Delta esté visible ni de su referencia configurada.
- Una vuelta debe cumplir los mismos controles de reconstrucción que Delta antes
  de incorporarse al historial.
- El historial aprendido no forma parte de importar/exportar configuración; sólo
  se exporta la preferencia visual de 0, 3 o 5 filas.

## Verificación enfocada

- `cargo test --manifest-path src-tauri\Cargo.toml --lib` cubre reconstrucción y
  persistencia compartidas.
- `npm.cmd run build` cubre las entradas independiente, compuesta y OBS.
- En LMU, validar cruces S1/S2/meta, vuelta inválida, reinicio de sesión y recarga
  del mejor absoluto tras reiniciar la aplicación.

## Localización

Las etiquetas estáticas, el historial, el título y el texto accesible usan el
idioma incluido seleccionado. Los tiempos y deltas conservan su notación compacta.
