# Comparación de rendimiento

Este recolector mide BlackRack Overlay y TinyPedal con el mismo criterio e incluye
los procesos secundarios WebView2 de BlackRack Overlay.

## Preparación

1. Abre Le Mans Ultimate y entra en una sesión estable, preferiblemente una
   repetición para poder repetir exactamente el mismo escenario.
2. Configura en ambas aplicaciones información equivalente. Para la comparación
   principal usa standings y calculadora de combustible; desactiva el resto.
3. Usa tamaños y frecuencia de actualización comparables.
4. Cierra aplicaciones pesadas y deja pasar 30 segundos antes de medir.

## Ejecución

Desde PowerShell, en la raíz del proyecto:

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\performance\compare-overlays.ps1 -DurationSeconds 300
```

El script genera las muestras y un resumen en `tools/performance/results/`.
Las métricas principales son CPU media/P95, memoria privada media/máxima y GPU
media/P95. No actives el registro detallado de telemetría durante la prueba
principal, porque añade una pequeña carga de escritura a BlackRack Overlay.

Para aislar posibles interferencias, completa además dos pasadas de cinco
minutos: una solo con BlackRack Overlay y otra solo con TinyPedal. Mantén el mismo
fragmento de repetición y la misma configuración visual.

## Calentamiento antes de leer la memoria

La memoria sube durante unos cuatro minutos hasta un régimen estable y ahí se
queda. Una captura de 300 s termina dentro de esa rampa, así que su cifra de
memoria es un punto arbitrario de la subida y depende de cuánto llevaba abierta
la aplicación. Arranca la captura con la aplicación ya caliente, o descarta sus
primeros cinco minutos antes de leer cualquier valor de memoria. La CPU no
necesita ese margen y es válida desde la primera muestra.

Comprueba también que la captura sea homogénea: si la media de CPU se separa de
la mediana más de un 50 %, la pasada mezcló estados distintos —la sesión
terminó, los overlays se auto-ocultaron— y el resumen promedia situaciones que
nunca coexistieron.

Para leer la memoria sin la rampa, pasa `-MemoryWarmupSeconds`. Las cifras de
memoria se calculan solo con las muestras posteriores a ese instante; la CPU
sigue usando la captura entera porque no necesita margen. El resumen registra el
valor usado y cuántas muestras quedaron:

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\performance\compare-overlays.ps1 -DurationSeconds 1260 -MemoryWarmupSeconds 300
```

## Atribución por proceso

WebView2 se reparte en un proceso navegador, uno de GPU, un renderer por ventana
y varias utilidades. Sumarlos en una sola cifra dice cuánto cuesta la aplicación
pero nunca qué parte lo retiene, así que cada proceso se clasifica además por su
conmutador `--type=` de Chromium:

- `app`: el proceso Tauri/Rust.
- `browser`: el proceso navegador de WebView2.
- `gpu`: el proceso de GPU.
- `renderer`: uno por ventana. El host de overlays y el panel de control son
  renderers distintos, así que `renderer_count` y `renderer_max_mb` separan el
  mayor del resto.
- `utility`: red, almacenamiento, audio.
- `other`: crashpad y cualquier proceso cuya línea de comandos no se pueda leer.

El recolector genera tres archivos. `overlay-performance-<sello>.csv` mantiene
las columnas de siempre y añade una `private_*_mb` por rol; sus valores suman
`private_memory_mb`. `-summary.csv` añade la media de cada rol.
`-processes.csv` es la atribución propiamente dicha: una fila por proceso con su
PID, su rol, su media y su máximo de memoria privada, y el intervalo en que
estuvo vivo.

Lee `-processes.csv` primero. Ordena por memoria privada media, así que la
primera fila nombra el proceso que hay que atacar; el resto del trabajo de
memoria sale de ahí.
