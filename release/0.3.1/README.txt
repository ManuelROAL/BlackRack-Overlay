LMU Overlay 0.3.1 - Windows x64
================================

Instalacion
-----------
1. Ejecuta "LMU Overlay_0.3.1_x64-setup.exe".
2. Inicia Le Mans Ultimate.
3. Abre LMU Overlay y activa los paneles que quieras mostrar.

Cambios de esta version
-----------------------
- Standings y Relative refinan su presentacion compacta, los acentos por
  categoria y la identificacion del jugador.
- Standings recupera al arrancar el orden de parrilla, el tamano inicial de la
  clase y las vueltas validas anteriores usadas por AVG 5.
- Se estabiliza la reconstruccion de vueltas invalidas para evitar tiempos
  parciales falsos durante transiciones de vuelta, garaje o reinicios.
- RaceControl reintenta perfiles ausentes y mejora la deteccion del split del
  evento mediante rutas de respaldo.
- El mapa de circuito identifica al lider general de carrera con una estrella
  dorada independiente del marcador del jugador.

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
