LMU Overlay 0.2.0 - Windows x64
================================

Instalacion
-----------
1. Ejecuta "LMU Overlay_0.2.0_x64-setup.exe".
2. Inicia Le Mans Ultimate.
3. Abre LMU Overlay y activa los paneles que quieras mostrar.

Cambios de esta version
-----------------------
- La aplicacion se inicia por defecto en modo juego, con los overlays sin
  capturar el cursor.
- La transparencia puede configurarse con un unico valor general o de forma
  independiente para cada overlay.
- El monitor puede seleccionarse de forma general o por overlay.
- Al volver al modo individual se recuperan los valores de transparencia y las
  asignaciones de monitor anteriores.
- La transparencia efectiva se mantiene tambien en las fuentes locales para OBS.

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
