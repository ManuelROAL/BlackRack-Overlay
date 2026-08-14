LMU Overlay 0.5.0 - Windows x64
================================

Instalacion
-----------
1. Ejecuta "LMU Overlay_0.5.0_x64-setup.exe".
2. Inicia Le Mans Ultimate.
3. Abre LMU Overlay y activa los paneles que quieras mostrar.

Cambios de esta version
-----------------------
- Reduccion importante del consumo de CPU agrupando la telemetria nativa por
  tipo de contenido y enviando a cada overlay solo los campos que utiliza.
- Los hosts transparentes mantienen un unico WebView por monitor y reutilizan
  objetos de telemetria para reducir serializaciones, copias y recoleccion de
  memoria en WebView2.
- Dashboard, Trailing + Pedal y Danos + Neumaticos evitan escrituras de DOM y
  estilos cuando el valor no cambia.
- Trailing + Pedal conserva las muestras de entrada a 50 Hz y pinta su grafica
  a 25 Hz, con bloques configurables para grafica, pedales, volante, FFB,
  velocidad y marcha.
- Track Map actualiza sus marcadores directamente a unos 30 Hz sin transiciones
  continuas que mantengan activo el compositor.
- Mejoras en Standings y Relative: modelos preparados en Rust, recuperacion de
  historico, estabilizacion de vueltas invalidas y correcciones de perfiles,
  posiciones iniciales, energia y limites de pista.
- Mejoras en Danos + Neumaticos para temperaturas, desgaste, suspension,
  pinchazos, ruedas desprendidas y deteccion del aleron trasero.
- Mejoras en RaceOS para split de evento, reintentos de perfiles y porcentaje
  estimado de progreso DR.

Rendimiento medido
------------------
Con los once overlays visibles en la misma escena estacionaria, la CPU media
bajo de 10,298% en la 0.4.0 instalada a 5,888% de media en las dos ultimas
mediciones de la 0.5.0. Una repeticion o carrera en movimiento sigue siendo la
prueba recomendada antes de comparar equipos diferentes.

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
