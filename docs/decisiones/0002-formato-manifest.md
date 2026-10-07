# 0002 — Formato del manifest de versiones

**Fecha:** 2026-10-07. **Estado:** decidida.

## Contexto

El motor de actualización (`update_engine::manifest`, decisión 0001) necesita un formato concreto
para describir qué archivos componen una release válida del cliente WarCrafted: cliente base,
parches MPQ y addons obligatorios/opcionales. Investigado en
`docs/investigacion/autoactualizacion-y-manifest.md` (TUF, SteamPipe, Velopack, Sparkle,
Squirrel, Tauri Updater); ninguno de esos sistemas resuelve el manifest de archivos del juego por
sí solo, así que el formato es propio, inspirado en esos patrones sin reclamar implementar
ninguna especificación formal.

Principios de `AGENTS.md` que este formato debe cumplir: multi-reino desde el diseño (ningún dato
de reino como constante), addons obligatorios/opcionales nunca confundibles, confianza cero en lo
descargado (hash + firma, no solo nombre/fecha), protección contra path traversal.

## Formato

JSON. Un manifest describe **una release concreta de un canal de un reino**. Se publica
inmutable (nunca se edita un manifest ya publicado; una corrección es una versión nueva).

```json
{
  "schemaVersion": 1,
  "realm": "icetracks",
  "channel": "production",
  "clientBuild": 12340,
  "manifestVersion": 7,
  "publishedAt": "2026-10-07T12:00:00Z",
  "minLauncherVersion": "1.0.0",
  "files": [
    {
      "path": "Data/patch-W.MPQ",
      "role": "required",
      "kind": "clientPatch",
      "sizeBytes": 52428800,
      "sha256": "…",
      "source": {
        "url": "https://github.com/warcrafted-server/launcher-content/releases/download/v7/patch-W.MPQ",
        "compressedSizeBytes": 20971520,
        "compression": "none"
      }
    },
    {
      "path": "Interface/AddOns/WarCraftedUI/WarCraftedUI.toc",
      "role": "required",
      "kind": "addon",
      "addonGroup": "warcrafted-core-ui",
      "sizeBytes": 4096,
      "sha256": "…",
      "source": { "url": "…", "compressedSizeBytes": 1200, "compression": "none" }
    },
    {
      "path": "Interface/AddOns/Deadly BossMods/DBM.toc",
      "role": "optional",
      "kind": "addon",
      "addonGroup": "deadly-boss-mods",
      "sizeBytes": 10485760,
      "sha256": "…",
      "source": { "url": "…", "compressedSizeBytes": 3000000, "compression": "none" }
    }
  ],
  "signature": {
    "keyId": "warcrafted-manifest-2026",
    "algorithm": "ed25519",
    "value": "…"
  }
}
```

### Campos clave y motivo

| Campo | Motivo |
|---|---|
| `realm` + `channel` | Multi-reino desde el diseño: nunca se asume un reino único, aunque hoy solo exista `icetracks`/`production`. Evita mezclar archivos de reinos o canales (prod/test) distintos. |
| `clientBuild` | Rechazar manifests para un build de cliente distinto al detectado (3.3.5/12340 hoy, otro build de WotLK mañana sin tener que cambiar el esquema). |
| `manifestVersion` | Entero monotónico, no la fecha. Permite detectar y rechazar downgrade: el launcher nunca acepta un manifest con versión menor a la ya aplicada en ese canal. |
| `minLauncherVersion` | Si el formato evoluciona de forma incompatible, el manifest puede exigir una versión mínima del propio launcher en vez de que este intente interpretar campos que no entiende. |
| `files[].path` | Ruta relativa normalizada, siempre con `/`, resuelta bajo la raíz de instalación del cliente. El motor de integridad rechaza cualquier ruta con `..`, raíz absoluta, letra de unidad, UNC (`\\`) o que resuelva fuera de la raíz, antes de tocar el filesystem. |
| `files[].role` | `required` u `optional`, **nunca un booleano ambiguo ni inferido**: el modelo de datos distingue explícitamente para que la UI y el bloqueo de juego no puedan confundirlos. |
| `files[].kind` | `clientBase`, `clientPatch`, `addon`, `config` — permite UI distinta (sección addons opcionales vs. archivos de cliente) sin parsear la ruta. |
| `files[].addonGroup` | Solo en `kind: addon`. Agrupa los archivos de un mismo addon (un addon son varios archivos) para activar/desactivar y versionar como unidad, no archivo a archivo. |
| `files[].sha256` | Hash del archivo final descomprimido, el que se verifica tras escribir en disco. Es el campo que protege contra corrupción/sustitución; está cubierto por la firma. |
| `files[].sizeBytes` | Límite de recursos y verificación adicional antes de aceptar lo descargado. |
| `files[].source` | Separado del índice de integridad: el origen (GitHub Releases hoy) puede migrar a otro host sin cambiar `path`/`sha256`, que son la identidad real del archivo. Solo se aceptan `https://` y una lista de hosts permitidos fijada en el launcher, nunca un host arbitrario del propio manifest. |
| `signature` | Firma Ed25519 sobre el documento canónico (JSON con claves ordenadas, sin el campo `signature`). La clave pública raíz va embebida en el binario del launcher, no en un archivo descargable. Rotación de clave: nuevo `keyId`, el launcher acepta una lista corta de claves vigentes embebidas en cada release del launcher. |

### Reglas de validación (en `update_engine::integrity`, no en la UI)

1. Verificar firma antes de leer ningún otro campo.
2. Rechazar si `realm`/`channel`/`clientBuild` no coinciden con la configuración activa.
3. Rechazar si `manifestVersion` ≤ versión ya aplicada (anti-rollback).
4. Rechazar si `minLauncherVersion` > versión actual del launcher.
5. Para cada entrada de `files`, normalizar y validar `path` antes de resolver ruta en disco.
6. Solo tras descargar y verificar `sha256` de todos los `required` se puede desbloquear el juego;
   los `optional` nunca participan en esa comprobación.

### Qué queda fuera de este manifest (decisión explícita)

- **Deltas binarios**: no se incluyen en la v1 del formato. Se añadirá `source.delta` (hash base +
  algoritmo) en una `schemaVersion` posterior si el tamaño real de los parches lo justifica
  (pendiente de medir, ver incertidumbres de la investigación). No bloquea el diseño actual:
  `source` ya separa objeto de índice para permitir esa extensión sin romper `path`/`sha256`.
- **Credenciales o datos de cuenta de reino**: un manifest es contenido público versionado, nunca
  lleva secretos.
- **Noticias y enlaces**: tienen su propio feed remoto (más simple, sin necesidad de staging
  atómico ni bloqueo de juego) — no se mezclan con este manifest de integridad.

## Pendiente de validar en la práctica

- Tamaño real de un manifest completo de WarCrafted (depende del número de addons obligatorios y
  parches) — no medido todavía.
- Proceso operativo de firma (dónde vive la clave privada, cómo se firma en CI) — se decide junto
  con la estrategia de releases (punto 7 del plan), no aquí.
