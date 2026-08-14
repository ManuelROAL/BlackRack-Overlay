LMU Overlay 0.3.2 - Windows x64
================================

Instalacion
-----------
1. Ejecuta "LMU Overlay_0.3.2_x64-setup.exe".
2. Inicia Le Mans Ultimate.
3. Abre LMU Overlay y activa los paneles que quieras mostrar.

Cambios de esta version
-----------------------
- El redimensionado de todos los overlays conserva ahora la proporcion del
  contenido, evitando marcos grandes con espacio vacio.
- Las geometrías antiguas desproporcionadas se corrigen conservando el tamano
  visual del overlay y eliminando solamente el area sobrante.
- Standings y Relative adaptan automaticamente el marco al cambiar columnas o
  filas, manteniendo la escala visual elegida por el usuario.
- La posicion, el tamano proporcional y la asignacion de monitor continúan
  guardandose entre ejecuciones.

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
