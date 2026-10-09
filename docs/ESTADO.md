# Estado del launcher

Mantén este archivo al día con cada cambio relevante, no al final de la tarea. Es lo primero que
lee una sesión nueva sobre `launcher/`. Última actualización: **2026-10-08** (versión 0.2.1).

## Dónde estamos

Investigación completa (`docs/investigacion/`, 5 informes) y decisión de stack tomada:
**Tauri 2**, ver `docs/decisiones/0001-stack-tecnologico.md`. Esqueleto Tauri 2 generado y
compilando.

**Núcleo de actualización completo**: `update_engine::manifest` (parseo, firma Ed25519,
anti-rollback, `assembly` para fragmentos, `kind: archive`), `update_engine::integrity`
(verificación de hashes) y `update_engine::staging` (descarga con reintentos/reanudación HTTP
Range, ensamblado de fragmentos, extracción segura de `.tar`, aplicación atómica) — los tres
implementados, aceptados y con tests (23 tests en total, incluidos casos de ataque: path
traversal y symlinks dentro de un `.tar`, hash incorrecto, servidor sin soporte de Range).

**Probado de extremo a extremo contra GitHub real**: se montó un programa de prueba
(`cargo run --example`, no forma parte del repo) que usa `manifest`+`integrity`+`staging` para
descargar el cliente completo (~18,5 GB, 44 archivos) desde las releases reales de GitHub a una
carpeta local, verificando hashes. Resultado: **44/44 archivos correctos**, estructura de
carpetas reconstruida automáticamente y coincidente con la instalación real de Windows
(ejecutables/DLLs en la raíz del cliente, `Data/` como subcarpeta con `esES/` dentro).

**Manifest de contenido real publicado (`manifestVersion: 2`)**, generado con
`docs/contenido/generar_manifest.py` a partir del árbol real del cliente: incluye el cliente base
completo, los tres archivos fragmentados (`common.MPQ`, `lichking.MPQ`, `patch.MPQ`) con su
`assembly.parts[]`, los parches del mod SoD (`patch-Z.MPQ`, `patch-esES-Z.MPQ`, en la release
`patch`) y el addon obligatorio RuneEngraver (`kind: archive`, release `RuneEngraver`). Validado
contra el parser Rust real antes de publicar.

Se investigaron y descartaron como origen de descarga TeraBox, Hugging Face Hub (Datasets) y
DigiStorage (sin API estable, sin garantía contractual, o atados a una cuenta personal). El
cliente completo ronda 20 GB; tres archivos superan el límite de 2 GiB/archivo de GitHub Releases
(`patch.MPQ` ~3,73 GiB, `common.MPQ` ~2,69 GiB, `lichking.MPQ` ~2,40 GiB) y se fragmentaron en
partes de 1900 MiB con `docs/split-archivos-grandes.ps1` (decisión 0003). El resto del cliente se
sube entero, archivo por archivo.

**Alcance de contenido de la primera release (sin ADR propio, es alcance de producto, no
arquitectura):**
- Se excluye `Data/*/Documentation/**` (ambos idiomas): documentación del instalador original de
  Blizzard, no necesaria para jugar.
- Se excluye `Data/enUS/**` completo en esta primera versión: el cliente WarCrafted permite
  cambiar de locale, pero solo se soporta oficialmente `esES` por ahora. Cambiar a `enUS` sin este
  contenido dejará el cliente incompleto — limitación conocida, no un bug. Se puede añadir `enUS`
  después sin romper el formato del manifest (0002/0003 ya soportan assets con nombre distinto en
  origen que el `path` de instalación, útil para el conflicto de nombres duplicados entre locales
  en GitHub Releases, p. ej. los `.avi` de `Interface/Cinematics/`).
- Se excluyen también los archivos legales/enlaces sueltos de `esES/` (`AccountBilling.url`,
  `TechSupport.url`, `Credits*.html`, `eula.html`, `termination.html`, `tos.html`,
  `connection-help.html`): contenido legal/soporte de Blizzard, mismo criterio que
  `Documentation/`.
- Se excluye `patch-A.MPQ` (71 KB, en la raíz de `Data/`): investigado y confirmado (con fuentes,
  no oficial de Blizzard) como el patrón de parche de AzerothCore para el módulo ARAC (All Races
  All Classes). WarCrafted no usa ese módulo, así que no hace falta distribuirlo.
- Árbol completo del cliente volcado en `docs/arbol-cliente.csv` (no versionado, es dato de una
  sesión concreta — si hace falta regenerarlo, el comando PowerShell está en el historial de esta
  conversación, no en un script del repo).
- También se excluyen del cliente base, tras limpiar la release: manuales/PDF/HTML/JS de
  documentación de todos los idiomas (el usuario los borró de la release manualmente); se
  mantienen las cinemáticas `.avi`.
- Se añaden al manifest, no contemplados en el alcance inicial: los ejecutables/DLLs del
  directorio raíz del cliente (`Wow.exe`, `WowError.exe`, `Repair.exe`, `Battle.net.dll`,
  `DivxDecoder.dll`, `Scan.dll`, `dbghelp.dll`, `ijl15.dll`, `msvcr80.dll`, `unicows.dll`) como
  `role: required`, `kind: clientBase` — el juego no arranca sin ellos.
- `realmlist.wtf` se trata como una entrada más del manifest (`kind: config`, `role: required`,
  contenido fijo `set realmlist logon.warcrafted.com`), no como una comprobación aparte: si el
  jugador lo modifica o usa un cliente limpio sin configurar, `integrity` ya lo detecta como
  `Corrupt`/`Missing` igual que cualquier otro archivo, y `staging` lo repara igual que un parche.
  No hace falta lógica nueva.
- **`patch-Z.MPQ`/`patch-esES-Z.MPQ`** (contenido del mod SoD, se actualiza con frecuencia,
  independiente del cliente base) viven en la release de GitHub `patch`, separada de la del
  cliente base (`contenido-v1`) — ya incluidos en el manifest real, confirmado funcionando: GitHub
  Releases no distingue mayúsculas/minúsculas en las URLs de descarga de assets, verificado
  empíricamente.
- **RuneEngraver** (addon obligatorio) vive en la release `RuneEngraver`, como un único `.tar`
  (`kind: archive`) — ya incluido en el manifest real. Su estructura interna ya trae la carpeta
  `RuneEngraver/` envolvente, así que se extrae directamente en `Data/Interface/AddOns/`.
- La release de contenido base (`contenido-v1`) se subió vía interfaz web de GitHub (sin `gh`
  disponible en el entorno de desarrollo); limpieza de assets sobrantes ya realizada por el
  usuario (documentación, legales, `enUS/`, `patch-A.MPQ`).

## Decisiones tomadas

- **0001 — Stack tecnológico: Tauri 2.** Motor de actualización/integridad como módulo Rust propio,
  independiente del framework de UI. Pendiente de validar en la práctica: presencia real de
  WebView2 en equipos objetivo, tamaño/arranque del instalador final.
- **0002 — Formato del manifest.** JSON firmado (Ed25519), por reino/canal/build de cliente,
  versión monotónica anti-rollback, roles `required`/`optional` explícitos nunca confundibles,
  origen de descarga separado del hash de integridad. Pendiente de validar: tamaño real de un
  manifest completo, proceso operativo de firma.
- **0003 — Fragmentación de archivos grandes.** Un archivo del manifest puede declararse ensamblado
  a partir de fragmentos (`assembly.parts[]`, cada uno con su propio hash/origen) para sortear el
  límite de 2 GiB de GitHub Releases sin cambiar de infraestructura de distribución. **Validado**:
  funciona de punta a punta contra GitHub real.
- **0004 — Pipeline de generación del manifest y `kind: archive`.** Script Python
  (`docs/contenido/generar_manifest.py`) que recalcula hashes y regenera el manifest
  automáticamente; la firma queda como único paso manual deliberado (clave privada fuera del
  repositorio). Nuevo `FileKind::Archive` para addons distribuidos como un único `.tar`,
  extraído reutilizando la misma validación de rutas que el resto del manifest. **Validado**:
  usado para generar el manifest real v2 y probado con RuneEngraver.

## Próximo paso

La versión actual es la 0.2.1 (ver `CHANGELOG.md` y `docs/VERSIONADO.md`). Ya existen los comandos
Tauri (`check_client_status`, `update_client`, `launch_game`) y una primera pantalla funcional.

Las tareas pendientes, por orden de prioridad, viven en [`TODO.md`](../TODO.md); no se duplican
aquí.

## No hacer

- No añadir lógica de actualización/parcheo mezclada con comandos de UI: el motor va aparte desde
  el principio (decisión 0001).
- No mezclar código del launcher con el servidor C++/SoD del resto del repo.
- No delegar al ejecutor decisiones de arquitectura, solo implementación ya decidida.
