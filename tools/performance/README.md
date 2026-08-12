# Comparación de rendimiento

Este recolector mide LMU Overlay y TinyPedal con el mismo criterio e incluye
los procesos secundarios WebView2 de LMU Overlay.

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
principal, porque añade una pequeña carga de escritura a LMU Overlay.

Para aislar posibles interferencias, completa además dos pasadas de cinco
minutos: una solo con LMU Overlay y otra solo con TinyPedal. Mantén el mismo
fragmento de repetición y la misma configuración visual.
