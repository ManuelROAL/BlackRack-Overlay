LMU Overlay 0.3.0 - Windows x64
================================

Instalacion
-----------
1. Ejecuta "LMU Overlay_0.3.0_x64-setup.exe".
2. Inicia Le Mans Ultimate.
3. Abre LMU Overlay y activa los paneles que quieras mostrar.

Cambios de esta version
-----------------------
- Cada overlay permite restaurar por separado su configuracion o su posicion y
  tamano predeterminados.
- Las confirmaciones de restauracion se muestran dentro del panel de control y
  ya no exponen el origen localhost del navegador.
- Los selectores y demas controles del panel vuelven a aceptar clics con
  normalidad.
- El contenido de los overlays queda inerte: en modo edicion el raton solo
  permite mover el panel o cambiar su tamano desde el tirador.
- Se bloquean el menu contextual, la seleccion, el arrastre nativo, el foco y
  los clics auxiliares dentro de los overlays.

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
