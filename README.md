# LMU Overlay

Overlay de telemetría para **Le Mans Ultimate**, pensado para Windows y Linux. La aplicación usa Tauri 2, una interfaz TypeScript sin framework y un núcleo Rust. En Windows lee la interfaz oficial de memoria compartida `LMU_Data`; si el SDK del juego no está disponible al compilar, mantiene una fuente simulada para desarrollo.

## Estado actual

- Panel de control para abrir y ocultar cada overlay de forma independiente.
- Dashboard con velocidad, marcha, RPM, pedales, tiempos, delta y combustible.
- Clasificación multiclase con posición de salida, gaps, intervalos, mejor/última vuelta,
  media de las últimas cinco vueltas válidas, energía virtual, daño general, neumático,
  banderas, vuelta rápida, estado en boxes y rangos DR/SR cuando RaceControl los facilita.
  Muestra 10 coches de la clase del jugador y 3 de cada clase restante, sin scroll.
- Calculadora de Energía Virtual para Hypercar/LMGT3 con tres escenarios: consumo promedio, vuelta rápida de Qualy y última vuelta.
- Perfiles persistentes de consumo por coche y circuito, con proyección de la vuelta actual según la distancia recorrida.
- Promedio limpio separado del consumo real: formación, neutralizaciones y boxes no alteran el ritmo base.
- Aprendizaje persistente del consumo de entrada y salida de boxes para corregir las estrategias con varias paradas.
- Cálculos de energía necesaria basados en la fracción real de vuelta pendiente, no solo en vueltas enteras.
- Cada escenario muestra consumo, autonomía, energía/combustible a cargar y remanente estimado al final.
- Modo de combustible automático para LMP2, LMP3 y clases sin regulación NRG.
- Fuente de datos desacoplada mediante `TelemetrySource` y lector oficial de LMU en Windows.
- `Ctrl+Shift+O` alterna entre edición y modo juego; los overlays dejan pasar el ratón.
- `Ctrl+Shift+M` muestra y enfoca el panel de control.
- La selección, posición y tamaño de las ventanas se conservan entre ejecuciones.
- Los overlays se ocultan fuera del juego y en el garaje, y reaparecen al volver a pista.
- Registro de análisis activable desde el panel, persistente entre ejecuciones y guardado en JSONL.
- Interfaz y núcleo compartidos entre Windows y Linux.

Al arrancar por primera vez se muestra el dashboard. Clasificación y combustible pueden activarse desde el panel. En los siguientes arranques se restaura la última selección.

Cuando LMU se ejecuta con Proton, el acceso se hará dentro del mismo entorno Wine/Proton o mediante un pequeño puente local; esta decisión queda pendiente de validar en Linux.

## Ejecutar en desarrollo

Se necesitan Node.js LTS, Rust estable y las dependencias del sistema de Tauri 2.

```powershell
npm install
npm run tauri dev
```

En Windows también se requieren Microsoft C++ Build Tools con “Desktop development with C++” y WebView2. En Debian/Ubuntu:

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
npm install
npm run tauri dev
```

Para que los overlays aparezcan sobre el juego, LMU debe usarse en modo ventana o ventana sin bordes. En Linux se recomienda una sesión X11/XWayland para la primera versión; Wayland puro impone restricciones adicionales a las ventanas siempre visibles y al posicionamiento global.

## Diagnóstico de arranque

Cada inicio crea un nuevo `%APPDATA%\dev.lmuoverlay.desktop\startup.log` y conserva
la ejecución anterior como `startup.previous.log`. Estos registros incluyen las
etapas de Tauri, la ventana de control, el registro de atajos, el hilo de
telemetría, el cierre normal, cualquier error fatal o panic y las excepciones no
controladas del panel y los overlays. Los campos sensibles conocidos se ocultan.

## Arquitectura

```text
LMU / fuente simulada
        │
        ▼
TelemetrySource (Rust) ──► TelemetryFrame ──► eventos Tauri (20 Hz)
                                                    │
                       ┌────────────────────────────┼──────────────────────────┐
                       ▼                            ▼                          ▼
                  Dashboard                  Clasificación              Combustible
```

El panel crea los overlays bajo demanda y puede ocultarlos sin detener la fuente de telemetría. La estructura normalizada de `TelemetryFrame` evita que la interfaz dependa del formato binario del simulador. La calculadora de estrategia detecta la clase mediante el SDK: usa `mVirtualEnergy` en Hypercar y LMGT3, y `mFuel` en el resto.

El sistema de perfiles por distancia es una implementación propia en Rust inspirada funcionalmente en [TinyPedal](https://github.com/TinyPedal/TinyPedal). No incorpora su código GPL. El balance real sigue midiendo todo lo consumido, pero solo las vueltas válidas, sin boxes y completamente en bandera verde alimentan el promedio limpio. Las vueltas de entrada y salida se aprenden por separado y corrigen el recurso necesario según el número estimado de paradas.

## Registro para análisis

El interruptor **Registro para análisis** del panel abre un archivo `lmu-telemetry-<timestamp>.jsonl` en la carpeta de datos de la aplicación. Registra a 10 Hz los valores crudos de combustible y Energía Virtual, fase y tipo de sesión, progreso y validez de vuelta, estado de boxes, cargas realizadas, referencias aprendidas y resultados de estrategia. La clasificación completa se omite para mantener un tamaño razonable.

El mismo archivo incluye eventos `driver_rank_refresh` con el estado de la consulta agrupada de perfiles a RaceControl y eventos `driver_rank_lookup` con el nombre, DR/SR y resultado de cada consulta. El ticket temporal y el token de acceso nunca se escriben en el log. Estas entradas se deduplican y solo se repiten cuando cambia el resultado o se vuelve a activar el registro.

La preferencia se conserva al reiniciar. Al desactivarlo se vacía y cierra inmediatamente el archivo activo; al volver a activarlo se crea uno nuevo. El panel muestra el nombre del archivo y la ruta completa al mantener el cursor sobre él.

## Fuente de navegador para OBS

El apartado **OBS / Navegador local** del panel permite exponer los mismos overlays en
`http://127.0.0.1:47636`. Al activarlo aparecen URLs independientes para standings,
combustible, banderas, rejoin y dashboard. El servidor sólo escucha en el equipo local,
reutiliza la trama de telemetría existente y entrega los datos a 5 Hz mediante una única
conexión por fuente.

La opción está desactivada de forma predeterminada. Mientras permanece apagada no se abre
ningún puerto, no se mantiene un hilo HTTP y no se serializan tramas para el navegador.
Los HTML, la fuente Roboto Condensed, las banderas y los logotipos se incluyen en el paquete,
por lo que OBS no necesita acceder a Internet.

## Próximos hitos

1. Guardar opacidad, escala y preferencias de cálculo por overlay.
2. Añadir compatibilidad con la interfaz de LMU ejecutada bajo Proton.
3. Añadir clasificación relativa, banderas, neumáticos y cálculo avanzado de combustible.
4. Preparar instaladores de Windows y paquetes AppImage/deb para Linux mediante CI.
5. Añadir captura y reproducción de sesiones para probar sin arrancar el simulador.
