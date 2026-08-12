# Instrucciones de proyecto · LMUOverlay

## Propósito y tecnología

LMUOverlay es una aplicación de escritorio de telemetría para **Le Mans
Ultimate**, centrada en carreras multiclase y resistencia. Debe ser compacta,
legible mientras se conduce, configurable y de bajo consumo.

- Escritorio: Tauri 2 con núcleo Rust.
- Interfaz: TypeScript, Vite y HTML/CSS sin framework.
- Telemetría en Windows: memoria compartida oficial de LMU mediante
  `src-tauri/src/telemetry/lmu_bridge.cpp`.
- Si el SDK no se encuentra al compilar, la aplicación usa telemetría simulada.
- Linux es un objetivo de interfaz; la telemetría real con Proton aún está por
  validar.

## Contexto obligatorio antes de trabajar

Lee siempre, en este orden:

1. `AGENTS.md`
2. `docs/PROJECT_CONTEXT.md`
3. `docs/DECISIONS.md`
4. `docs/TODO.md`

Lee después sólo el documento relacionado con la tarea:

- Arquitectura, Tauri, ventanas o flujo de datos: `docs/ARCHITECTURE.md`.
- Telemetría, REST, RaceControl, combustible, banderas o timing:
  `docs/TELEMETRY.md`.
- Diseño, CSS, panel u overlays: `docs/OVERLAYS.md`.
- Medición y optimización: `docs/PERFORMANCE.md`.

El código y los tests prevalecen si hay discrepancias. Actualiza el documento
afectado cuando cambies comportamiento, arquitectura o una decisión relevante.

## Arquitectura que debe preservarse

```text
Memoria compartida LMU ─┐
REST local de LMU ──────┼─> LmuTelemetrySource ─> TelemetryFrame
RaceControl/RaceOS ─────┘                               ├─ eventos Tauri
                                                        ├─ JSONL opcional
                                                        └─ SSE local para OBS
```

- La memoria compartida es la fuente crítica y autoritativa en tiempo real.
- REST local y RaceControl sólo enriquecen datos y deben fallar sin romper los
  overlays.
- `TelemetryFrame` es el contrato entre Rust y TypeScript: actualiza a la vez
  `src-tauri/src/telemetry/mod.rs` y `src/telemetry-types.ts` si cambia.
- No hagas llamadas HTTP bloqueantes dentro de `next_frame()`.
- El servidor para OBS es opcional, sólo escucha en `127.0.0.1:47636` y debe
  permanecer sin coste apreciable cuando está deshabilitado.

## Rendimiento

- El ciclo de telemetría, dashboard y alertas activas funciona a 20 Hz.
- Combustible se emite a 10 Hz.
- Standings completo se calcula y emite a 5 Hz **sólo** cuando la ventana de
  standings está visible o hay una fuente de navegador conectada.
- Banderas y rejoin usan el snapshot crudo a 20 Hz; nunca deben forzar la
  construcción de standings completo.
- Mantén caché de identidad de vehículo, nodos DOM y valores previamente
  renderizados. No reintroduzcas cálculos pesados en el ciclo de 20 Hz sin una
  medición que lo justifique.
- Para cambios de rendimiento, compara la misma repetición/carrera y revisa los
  eventos `performance_sample` y `source_stage_performance` del JSONL.

## Reglas funcionales clave

### Standings

- Sin scroll ni recorte: al redimensionar, escala el contenido completo.
- Agrupa por categoría usando los colores del juego: Hypercar es rojo.
- Por defecto muestra 10 coches de la categoría del jugador y 3 de cada otra;
  estos valores y las columnas son configurables.
- En la clase del jugador conserva los tres primeros y muestra coches alrededor
  del jugador hasta completar el número configurado.
- Usa dorsal asignado por la sala, nombre + un apellido, bandera y marca desde
  los recursos incluidos.
- GAP/INT no deben mostrar una vuelta de diferencia hasta que exista una vuelta
  física completa de diferencia.
- Mejor vuelta personal en verde; vuelta rápida de sesión en morado.
- La columna PIT muestra número de paradas; desde la entrada hasta terminar la
  vuelta de salida muestra tiempo de ciclo de boxes.
- La cabecera de categoría muestra SOF y coches actuales/iniciales.

### Combustible y energía

- Hypercar/LMGT3 usan Energía Virtual; el resto usa litros de combustible.
- Todos los cálculos de consumo pertenecen al jugador; las vueltas totales de
  carrera se estiman a partir del líder.
- No añadas una vuelta artificial: usa proyección por fracción de vuelta y la
  reserva configurada.
- El consumo de qualy procede de la vuelta válida más rápida y se conserva para
  la carrera; además representa el máximo de consumo permitido para el objetivo
  automático.
- Media limpia y última vuelta se reinician por sesión. Formación, neutralización
  y boxes no contaminan la media limpia, pero su gasto sí cuenta en la estrategia.
- Aprende y persiste por coche/circuito los consumos de entrada y salida de boxes.

### Banderas y rejoin

- A cuadros tiene máxima prioridad.
- Standings muestra siempre el probable causante de amarilla. El overlay de
  banderas sólo aparece si LMU confirma amarilla en el sector y el incidente es
  relevante por distancia.
- La causa amarilla es una inferencia estabilizada: el campo exacto no está
  expuesto en los datos usados. No la presentes como un dato oficial exacto.
- Rejoin sólo se muestra cuando el jugador provoca el incidente o sale de boxes,
  no por cualquier salida de pista.

### Ventanas y diseño

- Las ventanas son overlays independientes controlados desde el panel.
- El modo juego es click-through. No captures Escape: debe quedar libre para el
  menú de LMU.
- La visibilidad automática depende de estado de LMU; se ocultan fuera de foco,
  en garaje, sin tiempo real o al terminar sesión.
- Usa Roboto Condensed incluido en el proyecto, igual que la interfaz de LMU. Conserva logos, banderas e insignias
  en los builds y en el navegador local.
- Mantén CSS específico por overlay; sólo reglas realmente compartidas van a
  `src/styles.css` o `src/fonts.css`.

## Seguridad, dependencias y referencias

- No registres ni persistas tickets de LMU, tokens de RaceControl ni secretos.
- Autentica RaceControl con el ticket local de LMU de corta duración; no añadas
  claves de servidor/Nakama ni claves tomadas de terceros.
- RaceOS es un endpoint de cliente observado, no una API pública garantizada:
  implementa reintentos, caché y degradación segura.
- TinyPedal es referencia funcional y de rendimiento, pero no se puede copiar
  código GPL. Dox y Go Fast son referencias visuales/funcionales, no dependencias.
- Los usuarios no deben instalar una DLL propia en LMU. La aplicación detecta el
  plugin oficial `LMU_SharedMemoryMapPlugin64.dll` del juego.

## Comandos de trabajo (Windows)

Ejecuta desde la raíz. Usa `npm.cmd`: PowerShell puede bloquear `npm.ps1`.

```powershell
npm.cmd install
npm.cmd run dev
npm.cmd run tauri dev
npm.cmd run build
cargo fmt --manifest-path src-tauri\Cargo.toml
cargo test --manifest-path src-tauri\Cargo.toml --lib
npm.cmd run tauri build
```

- `npm.cmd run dev`: vista web en `http://localhost:1420`.
- `npm.cmd run tauri dev`: aplicación completa.
- `npm.cmd run build`: comprueba TypeScript y genera los recursos web.
- `cargo test ... --lib`: tests Rust habituales.
- `npm.cmd run tauri build`: genera instalador NSIS. Sólo ejecútalo cuando el
  usuario pida explícitamente una compilación de distribución.

## Flujo de trabajo y validación

1. Conserva cambios ajenos: el árbol puede estar sucio.
2. Implementa el cambio más pequeño que cumpla el objetivo.
3. Añade tests dirigidos si cambias semántica de telemetría, vueltas, boxes,
   gaps, banderas, sesiones o aprendizaje de consumo.
4. Para backend ejecuta:

   ```powershell
   cargo fmt --manifest-path src-tauri\Cargo.toml
   cargo test --manifest-path src-tauri\Cargo.toml --lib
   ```

5. Si afecta a datos serializados o frontend, ejecuta también:

   ```powershell
   npm.cmd run build
   ```

6. Para rendimiento, una compilación correcta no demuestra una mejora: recoge
   una medición comparable siguiendo `docs/PERFORMANCE.md`.

## Estado inmediato

La siguiente tarea prioritaria es medir en una carrera o repetición comparable la
optimización reciente de standings. Antes se construía el standings completo a
20 Hz aunque sólo se emitía a 5 Hz; ahora se construye bajo demanda a 5 Hz. No
hagas otra optimización especulativa antes de revisar esas métricas.
