# 0005 — Addons opcionales: catálogo firmado e instalación

**Fecha:** 2026-10-09. **Estado:** decidida.

## Contexto

El launcher distingue addons **obligatorios** (van en el manifest de contenido, los gestiona el
sistema de integridad y el jugador no puede quitarlos) de **opcionales** (el jugador los elige).
Nunca deben confundirse en el modelo de datos ni en la UI (`AGENTS.md`, sección 3). Hasta ahora
solo existían los obligatorios.

## Decisión

Los opcionales viven en un **catálogo propio**, `docs/contenido/addons.json`, firmado con la misma
clave Ed25519 y el mismo mecanismo que el manifest (JSON canónico sin `signature`, mismo `keyId`),
publicado en la misma ubicación que el manifest. El catálogo **nunca contiene addons
obligatorios** y el manifest nunca contiene opcionales: son documentos distintos con tipos
distintos en el código.

```json
{
  "schemaVersion": 1,
  "catalogVersion": 1,
  "publishedAt": "2026-10-09T12:00:00Z",
  "addons": [
    {
      "id": "ejemplo-addon",
      "name": "Ejemplo",
      "description": "Qué hace, en una frase.",
      "author": "Autor",
      "version": "1.2.0",
      "license": "MIT",
      "homepage": "https://github.com/…",
      "sha256": "<64 hex del .tar>",
      "sizeBytes": 123456,
      "source": { "url": "https://github.com/…/ejemplo-1.2.0.tar" },
      "folders": ["Ejemplo", "Ejemplo_Opciones"]
    }
  ],
  "signature": { "keyId": "warcrafted-manifest-2026", "algorithm": "ed25519", "value": "…" }
}
```

Reglas de validación (todas obligatorias, el catálogo entero se rechaza si falla una):

- Firma válida; `catalogVersion` no menor que la última vista (anti-rollback, igual que el manifest).
- `id`: `[a-z0-9-]{1,48}`, único. `version` SemVer. `sha256` hex de 64. `sizeBytes` > 0.
- `source.url` HTTPS y de un host de la lista permitida del manifest (`validate_source_hosts`).
- `folders`: nombres de una sola componente (sin separadores, `..`, ni rutas absolutas), no vacíos,
  sin duplicados dentro del addon ni entre addons distintos del catálogo.
- Ninguna carpeta puede coincidir con una carpeta de addon obligatorio del manifest ni con
  `Blizzard_*`: así un opcional no puede pisar uno obligatorio.
- El `.tar` tiene una carpeta de primer nivel por cada entrada de `folders`, y solo esas
  (`extract_archive` ya rechaza traversal, enlaces simbólicos y archivos en la raíz).

## Instalación

- Destino: `<cliente>/Interface/AddOns/<carpeta>`. Se descarga con `staging` (hash obligatorio,
  reanudable) y se extrae con `extract_archive` en la misma operación atómica de reemplazo de
  directorios; nunca se escribe fuera de `folders`.
- **Registro** en los ajustes del launcher (`settings.json`, clave `optionalAddons`): lista de
  `{ id, version, folders }`. Es lo único que dice qué instaló el launcher.
- **Desinstalar** borra solo las carpetas registradas del addon y su entrada del registro; nunca
  toca otras carpetas de `AddOns`. **Actualizar** = instalar cuando la versión del catálogo difiere
  de la registrada.
- Un addon registrado cuyas carpetas han desaparecido se muestra como «no instalado» y se puede
  reinstalar. El launcher no modifica ni borra carpetas que no estén en el registro.
- Los opcionales **no** bloquean el botón Jugar ni entran en la verificación del cliente.
- Fallo al obtener el catálogo (red, firma): la pestaña lo indica y los obligatorios y el juego no
  se ven afectados.

## Consecuencias

- Un segundo documento firmado que mantener; se publica con un script gemelo de
  `publicar_parches.py` (o ampliándolo).
- Añadir un addon opcional es publicar su `.tar` y regenerar/firmar el catálogo, sin recompilar el
  launcher.
- Un opcional con el mismo nombre de carpeta que uno ya instalado a mano por el jugador lo
  reemplaza solo si el jugador lo confirma (la UI avisa); el launcher no lo borra en silencio.
