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
