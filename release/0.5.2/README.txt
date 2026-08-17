LMU Overlay 0.5.2 - Windows x64
================================

Instalacion
-----------
1. Ejecuta "LMU Overlay_0.5.2_x64-setup.exe".
2. Inicia Le Mans Ultimate.
3. Abre LMU Overlay y activa los paneles que quieras mostrar.

Cambios de esta version
-----------------------
- Los selectores de monitor muestran el nombre de modelo comunicado por Windows
  junto con su resolucion para facilitar la identificacion de cada pantalla.
- Las asignaciones individuales de monitor se conservan al mover y restablecer
  overlays, incluso despues de usar temporalmente un monitor general.
- En modo edicion, solo las superficies visibles de los paneles reciben el
  cursor; los huecos transparentes dejan interactuar con las aplicaciones que
  haya debajo, tambien en configuraciones con varios monitores.

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

No es necesario instalar Node.js, Rust ni Tauri. Los recursos web, iconos,
banderas, insignias, fuentes y recursos de overlays estan incrustados.

Este instalador no esta firmado digitalmente, por lo que Windows SmartScreen
puede mostrar una advertencia al ejecutarlo por primera vez.
