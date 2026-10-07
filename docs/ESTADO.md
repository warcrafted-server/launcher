# Estado del launcher

Mantén este archivo al día con cada cambio relevante, no al final de la tarea. Es lo primero que
lee una sesión nueva sobre `launcher/`. Última actualización: **2026-10-07**.

## Dónde estamos

Investigación completa (`docs/investigacion/`, 5 informes) y decisión de stack tomada:
**Tauri 2**, ver `docs/decisiones/0001-stack-tecnologico.md`. Esqueleto Tauri 2 generado y
compilando. `update_engine::manifest` (commit `5756d42`, con soporte de `assembly`) y
`update_engine::integrity` (commit `2b97668`) implementados y aceptados, con tests. `staging`
sigue vacío.

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
- **`patch-Z.MPQ`/`patch-esES-Z.MPQ` (contenido del mod SoD, se actualiza con frecuencia,
  independiente del cliente base) van en una Release de GitHub separada** de la del cliente base
  (`contenido-v1`). Esto no cambia el formato del manifest: sigue siendo un único manifest con
  todos los archivos, pero cada entrada tiene su propio `source.url`, que puede apuntar a
  cualquier release — no hace falta que todas las URLs vivan en la misma release. Pendiente:
  hashear estos dos archivos (no estaban incluidos en los cálculos anteriores) y decidir el
  nombre/tag de esa release de parches.
- Subida de la release de contenido base (`contenido-v1`) hecha vía interfaz web de GitHub (sin
  `gh` disponible en el entorno de desarrollo); limpieza de assets sobrantes ya realizada por el
  usuario.
- RuneEngraver (addon obligatorio) y el flujo completo de arranque (comprobar realmlist →
  comprobar integridad de parches/addons → reparar si falta algo → arrancar) quedan descritos y
  acordados, pendientes de traducir a entradas de manifest + implementación de `staging` y del
  comando de lanzamiento del juego (todavía no existe ningún comando Tauri de UI para esto, solo
  el `greet` de ejemplo del scaffold).

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
  límite de 2 GiB de GitHub Releases sin cambiar de infraestructura de distribución. Pendiente de
  validar: tamaño real del cliente, tamaño óptimo de fragmento.

## Próximo paso

1. Calcular hash SHA-256 de todos los archivos del cliente base ya subidos a `contenido-v1`
   (pendiente: ejecutar el script de hashes sobre la lista limpia tras quitar documentación).
2. Decidir tag/release para `patch-Z.MPQ`/`patch-esES-Z.MPQ` y hashearlos aparte.
3. Construir el `manifest.json` real con todas las URLs (de `contenido-v1` y de la release de
   parches) y hashes.
4. Implementar `update_engine::staging` (descarga con reintentos/reanudación, ensamblado de
   fragmentos según decisión 0003, aplicación atómica) sobre `manifest`/`integrity` ya
   implementados.
5. Diseñar e implementar el comando de lanzamiento del juego (comprobar realmlist, bloquear si el
   cliente no es válido, arrancar `Wow.exe`) — todavía no existe ningún comando Tauri real, solo
   el `greet` de ejemplo del scaffold.

## No hacer

- No añadir lógica de actualización/parcheo mezclada con comandos de UI: el motor va aparte desde
  el principio (decisión 0001).
- No mezclar código del launcher con el servidor C++/SoD del resto del repo.
- No delegar al ejecutor decisiones de arquitectura, solo implementación ya decidida.
