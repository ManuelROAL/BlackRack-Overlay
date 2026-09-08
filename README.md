# BlackRack Overlay

Overlays de telemetría configurables para **Le Mans Ultimate** en Windows.

[Descargar la última versión](https://github.com/ManuelROAL/BlackRack-Overlay/releases/latest) ·
[Ver cambios](https://github.com/ManuelROAL/BlackRack-Overlay/releases) ·
[Apoyar el proyecto](https://ko-fi.com/blackrack)

BlackRack Overlay muestra información de conducción, tiempos, clasificación,
neumáticos, daños, combustible, energía, boxes, banderas, mapa y condiciones de
pista sobre el simulador. Es gratuito y no necesita SimHub, Node.js, Rust ni
Tauri para funcionar.

## Requisitos

- Windows 10 u 11 de 64 bits.
- Le Mans Ultimate.
- El plugin oficial de LMU en
  `Plugins/LMU_SharedMemoryMapPlugin64.dll`.
- Microsoft Edge WebView2 Runtime. El instalador incluye su bootstrapper; puede
  necesitar conexión a Internet si WebView2 no está instalado.

## Instalación

1. Descarga el instalador desde la [última release](https://github.com/ManuelROAL/BlackRack-Overlay/releases/latest).
2. Ejecuta `BlackRack Overlay_<versión>_x64-setup.exe`.
3. Abre BlackRack Overlay y selecciona el idioma y el monitor en **General**.
4. Activa y configura los overlays que quieras usar.
5. Usa el modo **Edición** para mover o redimensionar los paneles.
6. Vuelve al modo **Juego** antes de conducir para que el ratón pueda pasar al simulador.

No copies ninguna DLL de BlackRack Overlay a la carpeta del juego. La telemetría
de LMU utiliza el plugin oficial del simulador.

## Actualizaciones

El panel de control comprueba automáticamente si hay una versión nueva al iniciar
y periódicamente mientras permanece abierto. También puedes comprobarla bajo
demanda desde **General > Actualizaciones**.

Cuando hay una versión disponible, la aplicación muestra sus cambios, descarga el
instalador y verifica su SHA-256 antes de ejecutarlo. La actualización es opcional
y no afecta al funcionamiento de la telemetría si el servidor no está disponible.

## OBS y fuente de navegador

La integración de OBS es opcional y solo escucha en el equipo local. Actívala en
**Integraciones > OBS / Navegador local** y añade las URLs mostradas como fuentes
de navegador en OBS.

## Atajos predeterminados

- `Ctrl+Shift+O`: alternar entre modo Juego y Edición.
- `Ctrl+Shift+M`: mostrar el panel de control.

Puedes cambiar los atajos en **General > Aplicación**.

## SmartScreen y verificación

El instalador actualmente no está firmado digitalmente, por lo que Windows
SmartScreen puede mostrar una advertencia la primera vez. Si quieres verificar la
descarga, usa el archivo `SHA256SUMS.txt` incluido en la release.

## Soporte

Desde **Integraciones > Soporte** puedes copiar un resumen de diagnóstico. Incluye
ese resumen, los pasos para reproducir el problema y una captura si es necesario.
No contiene tickets, tokens ni credenciales.

## English

BlackRack Overlay is a free, configurable telemetry overlay for **Le Mans
Ultimate** on 64-bit Windows. It provides driving, timing, standings, tyre,
damage, fuel, energy, pit, flag, track-map and weather information over the
simulator.

Download the [latest release](https://github.com/ManuelROAL/BlackRack-Overlay/releases/latest),
run the installer, select the language and monitor in **General**, then enable
the overlays you need. No SimHub, Node.js, Rust or Tauri installation is
required.

The control panel checks for updates automatically and on demand. Downloads are
verified with SHA-256 before installation. The installer is not digitally signed,
so Windows SmartScreen may display a warning on first run.

Optional project support: [Ko-fi](https://ko-fi.com/blackrack).
