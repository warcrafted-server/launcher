# Changelog

Todos los cambios relevantes de este proyecto se documentan aquí. El formato sigue
[Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el versionado sigue
[SemVer](https://semver.org/lang/es/) (ver [`docs/VERSIONADO.md`](docs/VERSIONADO.md)).

## [Sin publicar]

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
