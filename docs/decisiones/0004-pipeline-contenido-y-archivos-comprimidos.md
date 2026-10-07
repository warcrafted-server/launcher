# 0004 — Pipeline de generación del manifest y soporte de archivos comprimidos

**Fecha:** 2026-10-08. **Estado:** decidida.

## Contexto

El contenido del cliente (parches del mod SoD como `patch-Z.MPQ`/`patch-esES-Z.MPQ`, y addons
obligatorios como RuneEngraver) se actualiza con frecuencia, de forma independiente del cliente
base. Recalcular hashes y reescribir `docs/contenido/manifest.json` a mano cada vez (como se hizo
para la primera versión) no escala: el flujo real de trabajo es que una sesión de Claude aplica un
cambio al mod/addon en su propio repositorio, y debe poder regenerar el manifest de este repositorio
sin que el usuario tenga que repetir manualmente el proceso de cálculo de hashes.

Además, RuneEngraver se distribuye como un único archivo `.tar` (no archivo por archivo como el
resto del contenido), lo cual no estaba previsto en la decisión 0002.

## Decisión

### Script de generación del manifest

Un script Python (`docs/contenido/generar_manifest.py`, sin dependencias fuera de la librería
estándar, para que corra igual en Linux/Windows) que:

1. Recibe una carpeta raíz del cliente y la lista de releases de GitHub (nombre de release → URL
   base) donde viven los assets.
2. Recorre la carpeta aplicando las mismas reglas de exclusión ya decididas en esta conversación
   (`enUS/`, `Documentation/`, archivos legales/URL de Blizzard, `patch-A.MPQ`).
3. Calcula SHA-256 de cada archivo en streaming (igual que `update_engine::integrity`, por
   coherencia, aunque es un script aparte en Python, no Rust).
4. Para los archivos declarados como fragmentados (decisión 0003), construye la entrada
   `assembly.parts[]` a partir de los `.part-NNN` presentes.
5. Genera el documento del manifest sin firma, incrementando `manifestVersion` en 1 respecto al
   manifest anterior (lee el actual de `docs/contenido/manifest.json` para saber cuál era).
6. Imprime el documento canónico (JSON con claves ordenadas, sin `signature`) listo para firmar.

**La firma NO la hace el script.** Firmar exige la clave privada, que vive fuera del repositorio
por diseño (ver decisión 0002: "la clave pública raíz va embebida en el binario del launcher"; la
privada no se automatiza con el mismo criterio). El flujo pasa por: ejecutar el script → revisar el
documento generado → firmarlo aparte (manualmente con `openssl`, o con una herramienta que solo
tenga acceso quien gestione la clave) → sustituir `docs/contenido/manifest.json`.

Este es el único punto manual que queda en el pipeline, y es intencional: automatizar la firma
significaría que cualquier sesión con acceso al repositorio podría publicar un manifest válido sin
supervisión, lo que rompe la garantía de "confianza cero en lo descargado" del propio AGENTS.md.

### Soporte de archivos comprimidos (`kind: "archive"`)

Se añade un nuevo valor a `FileKind`: `archive`. Un archivo `kind: archive` se trata exactamente
igual que cualquier otro archivo del manifest a efectos de descarga e integridad (una URL, un
hash SHA-256 del `.tar`/`.zip` completo, sin campo `assembly` salvo que además supere 2 GiB). La
única diferencia es en `staging`: tras descargar y verificar el archivo comprimido, se extrae bajo
la ruta indicada en `path` (que para este `kind` es un directorio, no un archivo final).

**Validación de seguridad al extraer (obligatoria, no opcional):** cada entrada interna del
archivo comprimido se normaliza y valida con la misma función ya existente
(`manifest::normalize_manifest_path`/`resolve_manifest_path`) antes de escribirse en disco,
exactamente como se hace con cualquier `path` del manifest. Se rechaza cualquier entrada con `..`,
ruta absoluta, o que resuelva fuera del directorio de destino — igual que ya se exige para rutas
normales. No se introduce ninguna lista aparte de "rutas esperadas": reutilizar la validación que
ya existe es suficiente y evita duplicar lógica de seguridad.

Formato soportado en la primera versión: `.tar` sin compresión (es lo que ya se usa para
RuneEngraver). Si en el futuro hace falta `.tar.gz`/`.zip`, se añade entonces, no por adelantado.

## Qué no cambia

- El formato de `assembly.parts[]` (0003) sigue igual; un archivo `archive` grande podría en teoría
  combinarse con `assembly`, pero no es el caso de ningún contenido actual y no se diseña hasta que
  haga falta.
- Sigue habiendo un único manifest (no se separan "cliente base" y "mod SoD" en documentos
  distintos, decisión ya tomada en esta conversación): el pipeline simplemente regenera el mismo
  documento completo cada vez.
- GitHub Releases sigue siendo el origen; el script solo necesita conocer la URL base de cada
  release, no cambia nada de infraestructura.

## Pendiente de validar en la práctica

- Probar el script contra un cambio real de `patch-Z.MPQ`/`patch-esES-Z.MPQ` y confirmar que el
  flujo completo (regenerar → firmar → publicar → el launcher detecta la actualización) funciona
  de punta a punta una vez `staging` esté implementado.
