# Changelog

Todos los cambios relevantes de este proyecto se documentan aquí. El formato sigue
[Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el versionado sigue
[SemVer](https://semver.org/lang/es/) (ver [`docs/VERSIONADO.md`](docs/VERSIONADO.md)).

## [Sin publicar]

## [0.4.0] - 2026-10-09

Autoactualización del launcher (menor: funcionalidad nueva compatible).

### Añadido

- Al arrancar, el launcher consulta `latest.json` de la última release y, si hay una versión
  mayor, muestra un aviso con las notas y el botón «Actualizar launcher» (descarga con progreso,
  instala y reinicia). No actualiza mientras hay una comprobación, descarga o lanzamiento del
  cliente en curso.
- `tauri-plugin-updater` (firma minisign propia, distinta de la del manifest; rechaza firmas
  inválidas) y `tauri-plugin-process`, ambos MIT/Apache-2.0; instalación `passive`.

## [0.3.0] - 2026-10-09

Instalador de Windows (menor: funcionalidad nueva compatible).

### Añadido

- Instalador NSIS por usuario (sin administrador) con WebView2 mediante `embedBootstrapper`,
  editor, descripción y copyright; incluye `LICENSE`, `CREDITS.md` y `THIRD-PARTY-NOTICES.md`.
- `docs/INSTALACION.md`: instalación para jugadores y compilación del instalador.

### Cambiado

- Iconos de la aplicación regenerados desde el logotipo (`--fit contain`).
- Objetivo de empaquetado reducido a `nsis`.

## [0.2.2] - 2026-10-09

Cambio interno (parche): tests automáticos en CI.

### Cambiado

- Tests automáticos en GitHub Actions (`.github/workflows/ci.yml`): en push a `main` y en PR,
  Linux (`npm run build`, `cargo test`, tests de publicación de parches) y Windows (`npm run build`,
  `cargo build`, `cargo test`).

## [0.2.1] - 2026-10-09

Licencia y avisos de terceros; sin cambios de comportamiento.

### Añadido

- Licencia WNCL-TP-1.0 (`LICENSE`): uso, estudio, modificación y redistribución gratuitos no
  comerciales; prohíbe vender o monetizar sin permiso por escrito.
- `CREDITS.md` y `THIRD-PARTY-NOTICES.md` (generado con `scripts/third-party.py`) con el inventario
  de 509 componentes de terceros y sus licencias.
- Sección de créditos y licencia en el README.

## [0.2.0] - 2026-10-09

Interfaz completa en pestañas, verificación rápida y publicación automática de parches.

### Añadido

- Interfaz en pestañas (Inicio, Noticias, Addons, Ajustes) con barra de ventana propia (arrastrar,
  minimizar, cerrar) y barra de juego fija abajo.
- Inicio: noticia destacada con rotación, índice de últimas noticias, tarjetas de características y
  enlaces oficiales; asistente de primera instalación si no hay carpeta.
- Noticias con lector integrado y filtro por categoría (contenido de ejemplo en
  `src/content/content.sample.json`).
- Addons: obligatorios (gestionados, sin opción de desactivar) separados de los opcionales.
- Verificación rápida con caché: «Actualizar» y «Jugar» solo releen archivos nuevos o modificados;
  «Verificación completa» en Ajustes lo lee todo.
- El launcher detecta cuándo se cierra el juego que lanzó, no permite lanzarlo dos veces y no deja
  actualizar con el juego abierto (los clientes abiertos a mano no se bloquean).
- Botón «Cancelar» durante la comprobación y la actualización.
- «Nueva instalación…»: crea la carpeta «WarCrafted WotLK» en la ubicación elegida e instala el
  cliente completo; si la carpeta no tiene cliente, el botón principal pasa a «Instalar».
- Detalles de archivos con los problemáticos primero y el motivo de cada uno.
- Nuevo diseño con el logo de WarCrafted: cabecera con logo, tarjeta de carpeta del cliente,
  resumen de estado con detalles desplegables y barra inferior con progreso y botón JUGAR.
- Progreso en tiempo real de la verificación de archivos (archivo actual y bytes).
- Selector de la carpeta del cliente; la ruta se guarda en los ajustes del launcher.
- Botón «Borrar caché», con confirmación.
- Publicación automática de parches y addon obligatorio (`scripts/publicar-parches.sh`, ver
  `docs/contenido/PUBLICAR-PARCHES.md`): assets versionados, verificación, firma del manifest y
  retención de las 5 últimas versiones.
- `scripts/setup.ps1` y `scripts/setup.sh`: instalan automáticamente las dependencias de
  desarrollo (Node.js, Rust, compilador C++, WebView2 o librerías de Linux) y ejecutan `npm install`.

### Cambiado

- La verificación descarta primero por tamaño, sin calcular el hash de archivos que no coinciden.
- Ventana de 1100×720 (mínimo 960×640).
- Los comandos ya no reciben rutas ni el ejecutable desde la interfaz: el backend usa la carpeta
  guardada y lanza siempre `Wow.exe`.
- `TODO.md` con las tareas pendientes priorizadas.
- README: requisitos de Windows y paso obligatorio `npm install`.

### Eliminado

- Comando `greet` de ejemplo del scaffold de Tauri.

### Corregido

- `setup.ps1` añade `%USERPROFILE%\.cargo\bin` al PATH de usuario permanente; antes una terminal
  nueva podía no encontrar `cargo`.
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
