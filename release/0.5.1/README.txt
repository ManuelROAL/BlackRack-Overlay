LMU Overlay 0.5.1 - Windows x64
================================

Instalacion
-----------
1. Ejecuta "LMU Overlay_0.5.1_x64-setup.exe".
2. Inicia Le Mans Ultimate.
3. Abre LMU Overlay y activa los paneles que quieras mostrar.

Cambios de esta version
-----------------------
- Estrategia de combustible mas clara y compacta, con objetivos de ahorro
  limitados a ritmos alcanzables y mejor distincion visual de los escenarios.
- Standings calcula mejor las vueltas restantes por clase, usa el tiempo maximo
  oficial de la sesion y estabiliza las vueltas invalidas detectadas por LMU.
- El contador de paradas de Standings se confirma con el dato oficial de LMU y
  el tiempo de parada se presenta redondeado.
- Recuperacion de nacionalidades ausentes desde el roster del evento y manejo
  seguro de banderas de pais que no se puedan cargar en Relative.
- Nuevos formatos configurables para mostrar los nombres de pilotos en los
  overlays de carrera.
- Mejoras visuales de legibilidad en Dashboard, Fuel, Standings, Track Map,
  Pit Stop y los overlays de danos y neumaticos.

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
