# Plan: distribución, autoactualización y robustez de descargas

Hoja de ruta ejecutable por una sesión nueva sin contexto. Antes de empezar, lee `AGENTS.md`,
`docs/ESTADO.md` y `TODO.md`. Orquestador recomendado: **Sonnet, esfuerzo medio**; delega cada
paso con `agentrelay run` (el ejecutor no tiene red: las dependencias nuevas las añade el
orquestador con `cargo add` / `npm install` y las comita antes de delegar). Revisa cada diff,
ejecuta tú `cd src-tauri && cargo test` y `npm run build` (el sandbox del ejecutor no abre
sockets: 3 tests de `staging` fallan solo ahí). Al terminar cada paso: versión con
`node scripts/version.mjs X.Y.Z`, entrada en `CHANGELOG.md` (fecha real), quitar la tarea de
`TODO.md`, commit. **Push y tags solo con aprobación explícita del usuario.** Marca `[x]` aquí.
Si aparece un imprevisto de diseño, para y anótalo en «Bloqueos» en vez de improvisar.

Punto de partida: versión 0.2.1 (licencia WNCL-TP-1.0 ya aplicada, titular WarCrafted).

## Decisiones ya tomadas (no reabrir)

- **Instalador**: NSIS de Tauri, instalación por usuario (sin permisos de administrador),
  WebView2 con `embedBootstrapper`. Solo Windows x64.
- **Autoactualización**: `tauri-plugin-updater` con clave minisign propia, distinta de la del
  manifest. Pública (va en `tauri.conf.json` → `plugins.updater.pubkey`):
  `dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDA1MDM5NTlCMTE2RUU0NjQKUldSazVHNFJtNVVEQmFoWHNMalE1T0lTRjRsaU1EdXhrYWJlcWc3MmJPakVHcGkwTm9YbVBLU0UK`.
  Privada en `~/.warcrafted/updater.key`, contraseña en `~/.warcrafted/updater-key.password`
  (este Debian; nunca en el repo ni en la salida). Endpoint:
  `https://github.com/warcrafted-server/launcher/releases/latest/download/latest.json`.
- **Releases**: las de contenido (`patch`, `RuneEngraver`, `contenido-v1`) se marcan como
  pre-release para que nunca sean «latest» (hoy `latest` = `RuneEngraver`, comprobado
  2026-10-09). Cada release del launcher es `vX.Y.Z`, creada por CI al subir el tag, con
  `make_latest: true`.
- **Licencia**: WNCL-TP-1.0 aplicada (`LICENSE`, `CREDITS.md`, `THIRD-PARTY-NOTICES.md`). No tocar
  nada de licencias desde este plan. El logotipo está excluido y pendiente de verificar (ver
  `TODO.md`): no publicar instalador sin resolverlo.

## Pasos

### [x] 1. CI (hecho; falta verificar que pase en GitHub tras el push aprobado) de tests en GitHub Actions — 0.2.2 (parche: interno)
- Archivos: `.github/workflows/ci.yml`.
- En push a `main` y PR: job `ubuntu-latest` (deps de sistema de Tauri, `npm ci`, `npm run build`,
  `cargo test` en `src-tauri`, `python3 -m unittest docs/contenido/test_publicar_parches.py`) y job
  `windows-latest` (`npm ci`, `npm run build`, `cargo build` y `cargo test`). Caché de cargo.
- Terminado: el workflow pasa en GitHub tras el push (requiere push aprobado).
- Ejecutor: AgentRelay, esfuerzo medio. Verifica la licencia de cada acción usada.
- CHANGELOG: «Corregido/Cambiado: tests automáticos en GitHub Actions».

### [x] 2. Instalador NSIS (hecho; la compilación en Windows queda al usuario) — 0.3.0 (menor)
- Archivos: `src-tauri/tauri.conf.json` (bundle: `targets: ["nsis"]`, `windows.nsis.installMode:
  "currentUser"`, `windows.webviewInstallMode: { type: "embedBootstrapper" }`, `publisher`,
  `shortDescription`, `copyright: "Copyright (C) 2026 WarCrafted"`, y `bundle.resources` con `LICENSE`,
  `CREDITS.md` y `THIRD-PARTY-NOTICES.md` incluidos en la instalación; regenera este último con
  `python3 scripts/third-party.py` si cambian dependencias), iconos generados desde el logo
  con `npx tauri icon src/assets/logo-warcrafted.jpg` (el orquestador, no el ejecutor),
  `docs/INSTALACION.md` (instalar como jugador y compilar el instalador), README resumido.
- Terminado: `npm run tauri build` en Windows genera el `-setup.exe` y el usuario lo instala,
  abre y desinstala sin admin.
- Ejecutor: AgentRelay, esfuerzo medio. Compilación en Windows: la hace el usuario (o el CI del
  paso 4).

### [x] 3. Autoactualización del launcher (hecho; prueba extremo a extremo pendiente hasta el paso 4) — 0.4.0 (menor)
- Orquestador: `cargo add tauri-plugin-updater tauri-plugin-process` y
  `npm install @tauri-apps/plugin-updater @tauri-apps/plugin-process`; verifica licencias
  (deben ser MIT/Apache).
- Archivos: `src-tauri/src/lib.rs` (registrar plugins), `tauri.conf.json`
  (`bundle.createUpdaterArtifacts: true`, `plugins.updater.pubkey`, `endpoints`,
  `windows.installMode: "passive"`), `capabilities/default.json` (`updater:default`,
  `process:allow-restart`), `src/updater.ts` + aviso en la UI: al arrancar comprueba, muestra
  «Nueva versión X disponible» con notas y botón «Actualizar launcher» (descarga con progreso,
  instala y reinicia). Nunca actualiza el launcher en mitad de una descarga del cliente.
- Terminado: con un `latest.json` de prueba firmado con la clave real apuntando a una versión
  mayor, la UI lo ofrece; una firma incorrecta se rechaza.
- Ejecutor: AgentRelay, esfuerzo alto (seguridad).

### [x] 4. Workflow de release (hecho; faltan los secrets del usuario y un tag aprobado para probarlo) — 0.4.1 (parche)
- Antes (con aprobación del usuario): marcar como pre-release las releases `patch`,
  `RuneEngraver` y `contenido-v1` vía API (token en `~/.warcrafted/github-token`); el usuario
  crea en GitHub los secrets `TAURI_SIGNING_PRIVATE_KEY` (contenido de `updater.key`) y
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- Archivos: `.github/workflows/release.yml`: en tag `v*` sobre `windows-latest`, comprueba que el
  tag coincide con `node scripts/version.mjs --check`, compila con los secrets, crea la release
  `vX.Y.Z` con el instalador, su `.sig` y `latest.json` (notas = sección del CHANGELOG), marcada
  latest. `docs/VERSIONADO.md`: procedimiento de publicación actualizado.
- Terminado: un tag aprobado produce la release y una instalación anterior se autoactualiza.
- Ejecutor: AgentRelay, esfuerzo alto.

### [x] 5. Espacio en disco — 0.5.0 (menor)
- Orquestador: `cargo add fs4` (o equivalente; verificar licencia y que funcione en Windows).
- Archivos: `ui_commands.rs` (antes de descargar: suma de `sizeBytes` pendientes + margen del 10 %
  para staging frente al espacio libre del volumen del cliente; error claro con GB necesarios y
  libres), comando `get_disk_space`, UI: espacio necesario/libre en el asistente y en Ajustes.
- Terminado: test de la función de decisión (pura) y error visible si no cabe.
- Ejecutor: AgentRelay, esfuerzo medio.

### [x] 6. Progreso total de descarga — 0.6.0 (menor)
- Archivos: `update_engine/staging.rs` (callback de bytes descargados por bloque, sin romper la
  API actual), `ui_commands.rs` (evento `download-progress` limitado a 1 cada 200 ms con bytes
  totales hechos/total, velocidad media de los últimos 5 s y ETA), UI (barra global con
  «4,2 GB de 18,5 GB · 12 MB/s · 20 min»).
- Terminado: tests del cálculo de velocidad/ETA; progreso visible en una descarga real.
- Ejecutor: AgentRelay, esfuerzo medio.

### [x] 7. Reanudación entre ejecuciones — 0.7.0 (menor)
- Archivos: `ui_commands.rs`, `update_engine/staging.rs`. Staging persistente en
  `<cliente>/.warcrafted-staging/downloads/<sha256>.part` (la clave es el hash esperado: un
  `.part` de otra versión nunca se mezcla); al reanudar usa Range desde su tamaño; el hash final
  sigue siendo obligatorio. Al empezar, borra `.part` cuyo hash no esté en el manifest y
  directorios `run-*` huérfanos. Cancelar a mitad de archivo conserva el `.part`.
- Terminado: tests con servidor local (cortar, reanudar, hash correcto; `.part` ajeno borrado).
- Ejecutor: AgentRelay, esfuerzo alto.

### [ ] 8. Detectar clientes instalados y validar 3.3.5a — 0.8.0 (menor)
- Archivos: nuevo `src-tauri/src/client_detect.rs`, `ui_commands.rs`, UI (asistente y Ajustes).
- Versión: leer `VS_FIXEDFILEINFO` de `Wow.exe` (buscar la firma `0xFEEF04BD` y leer
  `dwFileVersionMS/LS`) y exigir 3.3.5.12340; sin dependencias nuevas. Detección: carpetas
  habituales (`C:\`, `D:\`… raíz y un nivel, `Program Files*`, `Games`, escritorio y documentos
  del usuario) con un límite de profundidad y tiempo; cancelable.
- Al elegir carpeta: si `Wow.exe` existe y no es 3.3.5.12340, aviso claro (no es el cliente
  correcto) antes de guardar.
- Terminado: tests con un PE mínimo sintético con VS_FIXEDFILEINFO; lista de candidatos en UI.
- Ejecutor: AgentRelay, esfuerzo medio.

### [ ] 9. Addons opcionales: catálogo y backend — 0.9.0 (menor)
- Diseño (ADR `docs/decisiones/0005-addons-opcionales.md`, lo escribe el orquestador): catálogo
  `docs/contenido/addons.json` firmado igual que el manifest (misma clave), entradas con `id`,
  `name`, `description`, `author`, `version`, `license`, `homepage`, `sha256`, `sizeBytes`,
  `source.url` (`.tar` con una carpeta de primer nivel), `folders` (carpetas que instala).
  Nunca contiene addons obligatorios. Instalados registrados en ajustes (`id`, `version`,
  `folders`); desinstalar borra solo esas carpetas; actualizar si cambia la versión. Reutiliza
  `staging` y `extract_archive`.
- Archivos: `src-tauri/src/optional_addons.rs`, `ui_commands.rs` (comandos list/install/update/
  uninstall), script de publicación equivalente a `publicar_parches.py` o ampliación de este.
- Terminado: tests de instalar, actualizar y desinstalar sin tocar otros addons.
- Ejecutor: AgentRelay, esfuerzo alto.

### [ ] 10. Addons opcionales: UI — 0.10.0 (menor)
- Pestaña Addons: catálogo con nombre, autor, versión, licencia, descripción; instalar /
  actualizar / desinstalar con progreso; separado visualmente de los obligatorios.
- Ejecutor: AgentRelay, esfuerzo medio.

## Bloqueos

(ninguno)
