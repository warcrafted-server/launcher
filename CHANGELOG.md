# Changelog

Todos los cambios relevantes de este proyecto se documentan aquí. El formato sigue
[Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el versionado sigue
[SemVer](https://semver.org/lang/es/) (ver [`docs/VERSIONADO.md`](docs/VERSIONADO.md)).

## [Sin publicar]

## [0.10.3] - 2026-10-09

Créditos y documentación (parche).

### Cambiado

- `CREDITS.md`: el logotipo y el icono constan como generados con Nano Banana (Gemini, de Google),
  con la fuente y fecha de consulta de sus términos; sin copyright exclusivo reclamado.
- `docs/INSTALACION.md`: cómo continuar ante el aviso de SmartScreen y comprobar el SHA-256.
- El workflow de release añade el SHA-256 del instalador a las notas de la release.

## [0.10.2] - 2026-10-09

Limpieza interna (parche).

### Corregido

- Advertencia de compilación por código no usado (`DownloadProgress::bytes_done`, solo se usa en tests).

## [0.10.1] - 2026-10-09

Cambio de icono (parche: solo recursos gráficos).

### Cambiado

- El icono de la aplicación y del instalador pasa a ser el emblema cuadrado «WarCrafted Universe»
  (`src/assets/icono-warcrafted.jpg`), regenerado con `npx tauri icon`.

## [0.10.0] - 2026-10-09

Addons opcionales: interfaz (menor: funcionalidad nueva compatible).

### Añadido

- La sección «Opcionales» de la pestaña Addons carga el catálogo firmado al abrirla y lista cada
  addon con autor, versión, licencia, descripción y enlace, con Instalar, Actualizar y Desinstalar
  según su estado, confirmación antes de desinstalar o reemplazar una carpeta existente, progreso
  con velocidad y tiempo restante, y cancelación.
- Estados de carga, sin carpeta de cliente, error de catálogo con Reintentar y catálogo vacío. Los
  textos del catálogo se insertan siempre como texto. Marcados como «Opcional» y distintos de los
  obligatorios; no influyen en Jugar. Se bloquean con el cliente ocupado o el juego abierto.

## [0.9.0] - 2026-10-09

Addons opcionales: catálogo y backend (menor: funcionalidad nueva compatible; sin interfaz todavía).

### Añadido

- Decisión 0005 y módulo `optional_addons`: catálogo `addons.json` firmado con la clave del manifest
  (anti-rollback, hosts permitidos, carpetas de una sola componente, sin colisión con addons
  obligatorios ni `Blizzard_*`). Separado del manifest de contenido en datos y en código.
- Comandos `list_optional_addons`, `install_optional_addon`, `update_optional_addon` y
  `uninstall_optional_addon`: descarga verificada y reanudable, extracción atómica solo de las
  carpetas declaradas y registro en los ajustes (`optionalAddons`). Desinstalar borra únicamente las
  carpetas registradas. No bloquean Jugar ni entran en la verificación del cliente.
- `docs/contenido/generar_catalogo_addons.py` (con tests) y `docs/contenido/PUBLICAR-ADDONS.md` para
  generar, firmar y publicar el catálogo.

## [0.8.0] - 2026-10-09

Detección de clientes instalados y validación de la versión (menor: funcionalidad nueva compatible).

### Añadido

- Módulo `client_detect`: lee la versión de `Wow.exe` (`VS_FIXEDFILEINFO`, sin dependencias) y exige
  3.3.5.12340; comprobado con tres `Wow.exe` reales (3.3.5.12340).
- Búsqueda de instalaciones existentes (raíz de unidades y un nivel, Program Files, `Games`,
  escritorio y documentos) con límite de profundidad y de tiempo, cancelable y sin seguir
  enlaces simbólicos. Comandos `detect_clients` y `check_client_folder`.
- Botón «Buscar instalaciones» en el asistente y en Ajustes, con la lista de candidatos y su
  validez.
- Al elegir una carpeta cuyo `Wow.exe` no es 3.3.5.12340, aviso claro y confirmación antes de guardar.

## [0.7.0] - 2026-10-09

Reanudación de descargas entre ejecuciones (menor: funcionalidad nueva compatible).

### Añadido

- Las descargas parciales se conservan en `<cliente>/.warcrafted-staging/downloads/<sha256>.part`
  (la clave es el hash esperado, así que un `.part` de otra versión nunca se mezcla). Al volver a
  abrir el launcher, un archivo cortado o cancelado continúa con Range desde donde se quedó; el hash
  final sigue siendo obligatorio.
- El progreso global cuenta los bytes ya descargados al reanudar.
- Al empezar una actualización se borran los `.part` ajenos al manifest pendiente y los
  directorios `run-*` huérfanos de ejecuciones anteriores.

## [0.6.0] - 2026-10-09

Progreso total de descarga (menor: funcionalidad nueva compatible).

### Añadido

- Evento `download-progress` (máximo cada 200 ms) con bytes hechos y totales, velocidad media de los
  últimos 5 s y tiempo restante; la barra del dock muestra «4,2 GB de 18,5 GB · 12 MB/s · 20 min».
- `update_engine::staging::stage_manifest_file_with_progress`, con callback de bytes por bloque
  (también para los fragmentos de archivos ensamblados); la API anterior no cambia.
- Módulo `download_progress` con el cálculo de velocidad y ETA, con tests.

## [0.5.0] - 2026-10-09

Comprobación de espacio en disco (menor: funcionalidad nueva compatible).

### Añadido

- Antes de descargar, se compara lo pendiente más un 10 % de margen para el staging con el espacio
  libre del volumen del cliente; si no cabe, error claro con los GB necesarios y libres.
- Comando `get_disk_space`; el estado del cliente muestra lo pendiente y el espacio libre, y
  Ajustes muestra el espacio libre del disco de la carpeta elegida.
- Dependencia `fs4` (MIT OR Apache-2.0).

## [0.4.1] - 2026-10-09

Workflow de release (parche: infraestructura de publicación, sin cambios en la aplicación).

### Añadido

- `.github/workflows/release.yml`: al subir un tag `vX.Y.Z` compila en Windows, firma el instalador
  y publica la release con el instalador, su `.sig` y `latest.json`, con las notas tomadas del
  CHANGELOG. Falla si el tag no coincide con la versión del proyecto.

### Cambiado

- Las releases de contenido `patch`, `RuneEngraver` y `contenido-v1` pasan a pre-release para que
  `latest` apunte siempre a una release del launcher.
- `docs/VERSIONADO.md`: procedimiento de publicación con el workflow.

## [0.4.0] - 2026-10-09

Autoactualización del launcher (menor: funcionalidad nueva compatible).

### Añadido

- Al arrancar, el launcher consulta `latest.json` de la última release y, si hay una versión
  mayor, muestra un aviso con las notas y el botón «Actualizar launcher» (descarga con progreso,
  instala y reinicia). No actualiza mientras hay una comprobación, descarga o lanzamiento del
  cliente en curso.
- `tauri-plugin-updater` (firma minisign propia, distinta de la del manifest; rechaza firmas
  inválidas) y `tauri-plugin-process`, ambos MIT/Apache-2.0; instalación `passive`.

### Corregido

- Tres tests fallaban en Windows (rutas con prefijo `\\?\`/nombres 8.3 y cambio de fecha sin abrir en
  escritura); detectado por el CI. Solo afecta a los tests.

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
