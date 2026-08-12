LMU Overlay 0.1.3 - Windows x64
================================

Instalacion
-----------
1. Ejecuta "LMU Overlay_0.1.3_x64-setup.exe".
2. Inicia Le Mans Ultimate.
3. Abre LMU Overlay y activa las ventanas que quieras mostrar.

Novedades de esta version
-------------------------
- Los atajos globales de modo juego/edicion y mostrar panel son configurables.
- La aplicacion comprueba si una combinacion esta ocupada antes de guardarla.
- Si el nuevo atajo falla, conserva el anterior para no perder el acceso.
- Los atajos se guardan y se registran automaticamente al iniciar.
- Un conflicto con otra aplicacion ya no provoca el cierre de LMU Overlay.

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
banderas, insignias y recursos de los overlays estan incluidos.

Este instalador no esta firmado digitalmente, por lo que Windows SmartScreen
puede mostrar una advertencia al ejecutarlo por primera vez.
