LMU Overlay 0.1.2 - Windows x64
================================

Instalacion
-----------
1. Ejecuta "LMU Overlay_0.1.2_x64-setup.exe".
2. Inicia Le Mans Ultimate.
3. Abre LMU Overlay y activa las ventanas que quieras mostrar.

Correccion de esta version
--------------------------
- La aplicacion ya no se cierra si Ctrl+Shift+O o Ctrl+Shift+M esta ocupado
  por otro programa. El atajo afectado se omite y el resto sigue funcionando.
- El conflicto queda anotado como advertencia en el log de arranque.

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
