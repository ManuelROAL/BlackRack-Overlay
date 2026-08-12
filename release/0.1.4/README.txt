LMU Overlay 0.1.4 - Windows x64
================================

Instalacion
-----------
1. Ejecuta "LMU Overlay_0.1.4_x64-setup.exe".
2. Inicia Le Mans Ultimate.
3. Abre LMU Overlay y activa las ventanas que quieras mostrar.

Cambios de esta version
-----------------------
- ESC ya no oculta ni muestra los overlays.
- Se ha eliminado la deteccion del menu basada en acelerador y freno.
- En modo juego, los clics atraviesan los overlays para poder usar los menus.
- Se mantienen los atajos globales configurables de la version anterior.
- Los overlays siguen ocultandose al salir de LMU, estar en garaje o abandonar
  el estado activo de la sesion.

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
