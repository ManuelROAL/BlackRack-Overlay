LMU Overlay 0.1.1 - Windows x64
================================

Instalacion
-----------
1. Ejecuta "LMU Overlay_0.1.1_x64-setup.exe".
2. Inicia Le Mans Ultimate.
3. Abre LMU Overlay y activa las ventanas que quieras mostrar.

Novedades de esta version
-------------------------
- La columna PIT muestra el tiempo acumulado desde la entrada al pit lane,
  incluida la parada, hasta finalizar la vuelta de salida.
- Se ha anadido un log de arranque para diagnosticar cierres inesperados.

Log de arranque
---------------
El archivo se crea en:
  %APPDATA%\dev.lmuoverlay.desktop\startup.log

Si la aplicacion se cierra al abrirla, comparte este archivo para localizar
el punto exacto del fallo.

Requisitos
----------
- Windows 10 u 11 de 64 bits.
- Le Mans Ultimate instalado mediante Steam.
- El plugin oficial de telemetria de LMU debe existir en:
  Le Mans Ultimate\Plugins\LMU_SharedMemoryMapPlugin64.dll

No es necesario instalar Node.js, Rust, Tauri ni copiar archivos de LMU
junto al instalador. WebView2 se comprobara durante la instalacion y se
descargara si Windows no lo tiene disponible.

Recursos incluidos
------------------
Los iconos de fabricantes, banderas de paises, insignias y los archivos
HTML/CSS/JavaScript de los overlays estan incluidos dentro de la aplicacion.

Notas
-----
- Este instalador no esta firmado digitalmente. Windows SmartScreen puede
  mostrar una advertencia al ejecutarlo por primera vez.
- El DLL de telemetria pertenece a Le Mans Ultimate y no se redistribuye.
  La aplicacion muestra "FALTA PLUGIN LMU" si no lo encuentra.
