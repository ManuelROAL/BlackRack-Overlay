LMU Overlay 0.1.6 - Windows x64
================================

Instalacion
-----------
1. Ejecuta "LMU Overlay_0.1.6_x64-setup.exe".
2. Inicia Le Mans Ultimate.
3. Abre LMU Overlay y activa las ventanas que quieras mostrar.

Cambios de esta version
-----------------------
- Nuevo SOF en la cabecera de cada categoria, calculado con el DR del campo.
- Estimacion de ganancia o perdida de DR mejorada con telemetria y REST local.
- Colores de Hypercar, LMP2, LMP3 y LMGT3 alineados con los de LMU.
- Transparencia del fondo configurable de forma independiente para cada overlay.
- Las cabeceras muestran coches actuales/iniciales para identificar DNF y DQ.
- Deteccion de estados finales reforzada con FSTAT_FINISHED, FSTAT_DNF y FSTAT_DQ.

Configuracion de atajos
-----------------------
Abre "ATAJOS GLOBALES" en el panel, selecciona un campo y pulsa Ctrl o Alt,
opcionalmente Shift, junto con una letra, numero o F1-F12.

Log de arranque
---------------
  %APPDATA%\dev.lmuoverlay.desktop\startup.log

Requisitos
----------
- Windows 10 u 11 de 64 bits.
- Le Mans Ultimate instalado mediante Steam.
- Plugin oficial de telemetria de LMU en:
  Le Mans Ultimate\Plugins\LMU_SharedMemoryMapPlugin64.dll

No es necesario instalar Node.js, Rust ni Tauri. Los iconos de fabricantes,
banderas, insignias, la fuente y los recursos de los overlays estan incluidos.

Este instalador no esta firmado digitalmente, por lo que Windows SmartScreen
puede mostrar una advertencia al ejecutarlo por primera vez.
