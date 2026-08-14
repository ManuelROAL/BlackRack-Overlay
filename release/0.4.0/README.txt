LMU Overlay 0.4.0 - Windows x64
================================

Instalacion
-----------
1. Ejecuta "LMU Overlay_0.4.0_x64-setup.exe".
2. Inicia Le Mans Ultimate.
3. Abre LMU Overlay y activa los paneles que quieras mostrar.

Cambios de esta version
-----------------------
- Los recursos web de produccion estan incrustados en el ejecutable; el
  instalador ya no despliega una carpeta web separada.
- JavaScript de produccion minificado y ofuscado de forma moderada, sin mapas
  de fuentes, con DevTools desactivado en el panel y los hosts de overlays.
- Exportacion e importacion conjunta de configuracion, posiciones, tamanos,
  monitores y transparencia mediante una carpeta elegida por el usuario.
- Estrategia de combustible y energia trasladada a Rust.
- Modelos de Standings y Relative preparados en Rust.
- Aprendizaje, persistencia y prediccion del mapa del circuito trasladados a
  Rust, conservando la geometria oficial y el aprendizaje local.
- Compilacion release con LTO completo, una unidad de generacion de codigo y
  eliminacion de simbolos.
- Servidor local para OBS validado con las once rutas, recursos incrustados y
  telemetria SSE en tiempo real.

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
