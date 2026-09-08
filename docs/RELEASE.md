# Publicación del instalador

BlackRack Overlay se comparte como instalador de Windows. El código fuente y los archivos
de desarrollo no forman parte de la entrega.

## Contenido de cada entrega

Crear `release/<version>/` con estos tres archivos:

- `BlackRack Overlay_<version>_x64-setup.exe`
- `README.txt`, con requisitos, instalación, cambios y aviso de SmartScreen
- `SHA256SUMS.txt`, con el hash SHA-256 del instalador

La publicación que ya incluya el auto-actualizador debe servir también un
`manifest.json` en la URL fija compilada en `src-tauri/src/updater.rs`. Debe usar
el esquema siguiente y publicar el instalador con el mismo hash:

```json
{
  "schemaVersion": 1,
  "version": "0.7.9",
  "packageUrl": "https://github.com/BlackRack/LMUOverlay/releases/download/v0.7.9/BlackRack%20Overlay_0.7.9_x64-setup.exe",
  "sha256": "<64 hex characters>",
  "fileName": "BlackRack Overlay_0.7.9_x64-setup.exe",
  "releasePageUrl": "https://github.com/BlackRack/LMUOverlay/releases/tag/v0.7.9",
  "updateTitle": "BlackRack Overlay 0.7.9",
  "fullTitle": "BlackRack Overlay 0.7.9",
  "changelog": ["Cambio visible para jugadores"]
}
```

El primer instalador que contenga esta funcionalidad debe publicarse antes de
que se anuncie una versión posterior en el manifiesto. El instalador sigue sin
firma digital por ahora; HTTPS y la doble comprobación SHA-256 reducen el riesgo
de una descarga incompleta o alterada, pero no sustituyen Authenticode.

El README de usuario puede incluir `https://ko-fi.com/blackrack`; la donación debe
seguir siendo opcional y nunca un requisito de instalación o funcionamiento.

## Novedades para el usuario

En cada versión, revisar los cambios desde el commit que fijó la versión anterior y
guardar una lista en `docs/releases/<version>.md`. Debe estar escrita para jugadores
e incluir cada cambio visible, sin nombres de commits, archivos, fuentes de datos ni
detalles de implementación. No mencionar cambios que se hayan revertido antes de
publicar. Separar los cambios por overlay y dejar los ajustes del panel de control,
estabilidad o rendimiento en una sección general. Usar exactamente los nombres que
aparecen en las tarjetas del panel de control.

Ese texto es la fuente para el apartado de cambios de `README.txt` y para el anuncio
de la versión. Antes de cerrar la entrega, contrastarlo con el estado final de la
aplicación y actualizarlo si la preparación de la versión añade o retira funciones.

## Comprobaciones antes de publicar

1. Sincronizar la versión en `package.json`, `src-tauri/Cargo.toml` y
   `src-tauri/tauri.conf.json`.
2. Ejecutar las comprobaciones indicadas en `AGENTS.md` y completar la validación
   funcional/performance pendiente de `docs/TODO.md`.
3. Confirmar que la compilación detecta el SDK oficial de LMU y no usa la fuente mock.
4. Crear el instalador con `npm.cmd run tauri build` solamente cuando se haya decidido
   publicar esa versión.
5. Instalar en una cuenta de Windows limpia o una máquina de prueba y comprobar inicio,
   WebView2, plugin oficial, overlays, atajos, OBS, importación/exportación y el botón
   de Ko-fi. En la pantalla final, comprobar también que las casillas para crear el
   acceso directo e iniciar la aplicación siguen disponibles, y que **Apoyar el
   proyecto en Ko-fi** aparece marcada inicialmente, puede desmarcarse y solo abre
   el navegador cuando permanece seleccionada al finalizar.
6. Generar el hash desde la carpeta de entrega:

   ```powershell
   Get-FileHash -Algorithm SHA256 '.\BlackRack Overlay_<version>_x64-setup.exe' |
     ForEach-Object { "$($_.Hash.ToLower())  BlackRack Overlay_<version>_x64-setup.exe" } |
     Set-Content -Encoding ascii SHA256SUMS.txt
   ```

7. Descargar de nuevo los tres archivos publicados y verificar que el hash coincide.

## Privacidad y seguridad

- No publicar logs de telemetría o diagnóstico, configuraciones exportadas ni bases
  de datos aprendidas.
- No incluir tickets, tokens, claves o datos de sesión.
- El botón de apoyo abre una URL constante en el navegador del sistema; no aceptar
  una URL suministrada por el frontend.
- La plantilla `src-tauri/nsis-installer.nsi` está fijada a Tauri CLI 2.11.4 para
  poder conservar las dos opciones finales de Tauri y añadir una tercera casilla.
  Compararla con la plantilla oficial antes de actualizar Tauri CLI.
- Indicar claramente si el instalador continúa sin firma digital.
