# Changelog

Todos los cambios relevantes de este proyecto se documentan aquí. El formato sigue
[Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el versionado sigue
[SemVer](https://semver.org/lang/es/) (ver [`docs/VERSIONADO.md`](docs/VERSIONADO.md)).

## [Sin publicar]

### Añadido

- Selector de la carpeta del cliente; la ruta se guarda en los ajustes del launcher.
- Botón «Borrar caché», con confirmación.
- Publicación automática de parches y addon obligatorio (`scripts/publicar-parches.sh`, ver
  `docs/contenido/PUBLICAR-PARCHES.md`): assets versionados, verificación, firma del manifest y
  retención de las 5 últimas versiones.
- `scripts/setup.ps1` y `scripts/setup.sh`: instalan automáticamente las dependencias de
  desarrollo (Node.js, Rust, compilador C++, WebView2 o librerías de Linux) y ejecutan `npm install`.

### Cambiado

- Los comandos ya no reciben rutas ni el ejecutable desde la interfaz: el backend usa la carpeta
  guardada y lanza siempre `Wow.exe`.
- `TODO.md` con las tareas pendientes priorizadas.
- README: requisitos de Windows y paso obligatorio `npm install`.

### Eliminado

- Comando `greet` de ejemplo del scaffold de Tauri.

### Corregido

- El addon RuneEngraver se instala en `Interface/AddOns/` (antes, por error, en
  `Data/Interface/AddOns/`, donde WoW no lo carga). Se aplica en la próxima publicación de parches.
- La extracción de addons (`kind: archive`) sustituye solo las carpetas que trae el `.tar`; antes
  reemplazaba la carpeta destino entera y habría borrado los demás addons del jugador.
- `scripts/setup.ps1` se guarda con BOM UTF-8 para que PowerShell 5.1 muestre bien los acentos.

## [0.1.0] - 2026-10-08

Primera versión funcional de desarrollo. Todavía no hay instalador ni release publicada del
launcher: se ejecuta con `npm run tauri dev`.

### Añadido

- Núcleo de actualización (`update_engine`):
  - `manifest`: parseo y validación del manifest de contenido, firma Ed25519, protección
    anti-rollback, archivos fragmentados (`assembly.parts[]`) y addons empaquetados
    (`kind: archive`).
  - `integrity`: verificación de hashes SHA-256 de los archivos locales.
  - `staging`: descarga con reintentos y reanudación HTTP Range, ensamblado de fragmentos,
    extracción segura de `.tar` (rechaza path traversal y symlinks) y aplicación atómica con
    copia de seguridad y rollback.
- Comandos Tauri `check_client_status`, `update_client` (con evento `update-progress`) y
  `launch_game`, que no lanza el juego si algún archivo obligatorio no es válido.
- Primera pantalla del launcher: comprobar estado, actualizar con barra de progreso y jugar.
- Manifest de contenido real (`manifestVersion: 2`, 44 archivos) y script de regeneración y firma
  `docs/contenido/generar_manifest.py`.
- Decisiones de arquitectura 0001 a 0004 en `docs/decisiones/`.

### Limitaciones conocidas

- La carpeta de instalación es una ruta fija temporal (`./warcrafted-client`).
- Solo se soporta el cliente en español (`esES`).
- Validado únicamente en Linux; pendiente de probar en Windows.
