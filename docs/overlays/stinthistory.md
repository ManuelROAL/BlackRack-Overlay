# Historial de stint

## Propósito

Historial de stint resume los dos relevos más recientes de la sesión para comparar
ritmo, duración, consumo y neumáticos sin abrir una herramienta externa. TinyPedal
es la referencia funcional; el cálculo, la presentación y el código son originales
y siguen el lenguaje visual de BlackRack.

## Archivos y propiedad

- `stinthistory.html`, `src/stinthistory.ts`, `src/stinthistory.css`: presentación.
- `src-tauri/src/telemetry/delta_records.rs`: agregación de vueltas y modelo del historial.
- `src/composite.ts`, `src/composite-layout.ts`: proyección y geometría compartida.
- `src-tauri/src/browser_source.rs`: ruta OBS `/stinthistory`.

Rust posee la selección y los cálculos. TypeScript únicamente formatea el modelo,
compone las filas y selecciona los iconos de compuesto ya incluidos.

## Comportamiento

- El panel nativo está deshabilitado en modo espectador; conserva su visibilidad
  guardada para los modos juego y equipo.

- Muestra como máximo dos filas: el stint actual y el anterior, en orden reciente.
- Cada fila contiene número de stint, vueltas completadas, tiempo acumulado,
  combustible o energía virtual consumida, compuesto, desgaste medio de las cuatro
  ruedas, delta y consistencia. En coches híbridos también muestra SOC inicial
  y final, además de la energía regenerada acumulada en kWh.
- En coches con energía virtual, NRG sustituye al combustible y se expresa en
  porcentaje; las demás clases muestran litros.
- El delta es la diferencia entre la mejor vuelta limpia y el promedio de las
  demás vueltas limpias del stint. La consistencia es `mejor / promedio × 100`.
  Ambos requieren al menos dos vueltas limpias.
- Las vueltas inválidas, de formación o que pasan por boxes cuentan para el total,
  duración y consumo del stint, pero no participan en mejor, delta o consistencia.
- El consumo de combustible o NRG de una vuelta con boxes incluye el recurso
  recargado durante esa vuelta, por lo que una segunda parada consecutiva no
  oculta el consumo de salida.
- El desgaste parte del promedio válido de las cuatro ruedas al comenzar el stint
  y usa la lectura actual; nunca presenta desgaste negativo tras un cambio de goma.
- La regeneración híbrida se integra a partir de los kW instantáneos de LMU y el
  reloj de sesión en la cadencia de la fuente. Los huecos superiores a un segundo
  se ignoran para que pausar o perder telemetría no fabrique energía regenerada.
  La fila actual muestra el acumulado del stint y las filas anteriores conservan
  el total del stint completado.
- El compuesto usa un icono único cuando las cuatro ruedas coinciden y una matriz
  2×2 cuando hay una combinación mixta.
- La fila actual usa el acento lima compartido. La superficie conserva el fondo
  oscuro degradado, contorno fino, tipografía condensada y densidad de BlackRack.
- Los coches híbridos añaden las columnas `SOC` (`inicio→fin%`) y `REG` (`kWh`);
  los demás mantienen el diseño compacto original de ocho columnas.
- No se registran filas vacías. El panel espera a una vuelta cronometrada completa.
- El historial se reinicia al cambiar de sesión o coche. SQLite conserva el resumen
  técnico existente, pero este overlay muestra sólo la sesión viva.

## Rendimiento e invariantes

- La reconstrucción de vueltas sigue activa en el ciclo compartido de 50 Hz; el
  modelo se prepara sólo cuando el panel nativo o `/stinthistory` tiene demanda.
- La entrega visual es de 4 Hz porque los valores cambian en límites semánticos y
  no requieren cadencia de conducción.
- No duplica semántica de vueltas en TypeScript ni consulta SQLite desde el bucle.
- Mantiene el safeguard de movimiento/blur del host compuesto y no usa animación
  continua, transición larga ni `backdrop-filter`.

## Verificación enfocada

- La prueba Rust comprueba que vueltas no limpias quedan fuera de delta y
  consistencia sin perder su consumo, tiempo o recuento, incluso al recargar NRG
  antes de una segunda parada consecutiva.
- `cargo test --manifest-path src-tauri\Cargo.toml --lib` cubre agregación,
  reconstrucción y persistencia compartida.
- `npm.cmd run build` cubre las entradas independiente, compuesta y OBS.
- En LMU, validar un stint normal, una vuelta inválida, paso por boxes, cambio de
  stint, cambio de compuesto y coches con y sin energía virtual.

## Localización

Cabecera, columnas, estado vacío, guía, tarjeta del panel y título usan los
catálogos español/inglés. Las unidades y abreviaturas mantienen notación compacta.
La ruta OBS respeta el idioma compartido o el override local `?lang=`.
