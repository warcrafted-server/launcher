# Publicar addons opcionales

Los addons opcionales viven en un catálogo propio y firmado, `docs/contenido/addons.json`
(decisión [0005](../decisiones/0005-addons-opcionales.md)), en la misma ubicación que el manifest y
firmado con la misma clave. El catálogo **nunca** contiene addons obligatorios: esos van en el
manifest de contenido. La URL que consume el launcher es
`https://raw.githubusercontent.com/warcrafted-server/launcher/main/docs/contenido/addons.json`, y
solo instala addons que aparezcan en un catálogo con firma válida.

## Flujo

1. Prepara la entrada del script:
   - `--source-root`: una carpeta por versión de addon llamada `<id>-<version>` (por ejemplo
     `questhelper-1.2.0`) y, dentro, una carpeta por cada carpeta que el addon instala en
     `Interface/AddOns` (por ejemplo `QuestHelper/`).
   - `--metadata`: un JSON con la ficha de cada addon, indexado por `id`, con `name`,
     `description`, `author`, `license` y `homepage` (HTTPS). Un addon sin ficha completa se
     rechaza: no se inventan metadatos.
2. Elige la release de contenido donde vivirán los `.tar` y anota su URL. Márcala **pre-release**
   mientras el catálogo no esté publicado: es el catálogo firmado que apunta a los assets lo que
   los hace públicos de facto.
3. Ejecuta `generar_catalogo_addons.py`. Empaqueta cada addon en un `.tar` determinista (mismo
   contenido, mismo hash), comprueba que el `.tar` tiene exactamente las carpetas declaradas en
   `folders`, calcula `sha256` y `sizeBytes`, incrementa `catalogVersion` respecto al `addons.json`
   anterior y escribe el catálogo firmado. Los `.tar` exactos que hay que subir quedan en
   `--assets-output`.
4. Sube esos `.tar` a la release de contenido con el nombre que les dio el script
   (`<id>-<version>.tar`).
5. Haz commit de `docs/contenido/addons.json` y push a `main`.

## Comandos

```bash
python3 docs/contenido/generar_catalogo_addons.py \
    --source-root /ruta/addons \
    --metadata /ruta/addons-metadata.json \
    --output docs/contenido/addons.json \
    --assets-output /ruta/salida-release \
    --release-base https://github.com/warcrafted-server/launcher/releases/download/addons-AAAAMMDD

python3 -m unittest docs/contenido/test_generar_catalogo_addons.py
```

`--signing-key` es opcional: por defecto firma con `~/.warcrafted/manifest-signing-key.pem`, la
misma clave que el manifest. `--unsigned-output` deja además el documento canónico sin firmar para
revisarlo antes de publicar; `--catalog-version` y `--published-at` fuerzan valores concretos (por
defecto se incrementa la versión anterior y se usa la hora actual).

## Reglas que aplica el script

- `id`: `[a-z0-9-]{1,48}` y único; `version` SemVer; `sha256` hex de 64; `sizeBytes` > 0.
- `folders`: nombre de una sola componente (sin separadores, `..` ni rutas absolutas), que no empiece
  por `Blizzard_`, sin duplicados dentro del addon ni entre addons distintos.
- `homepage` y `source.url` HTTPS, y el host de descarga debe estar en la lista permitida (GitHub).
- El `.tar` contiene exactamente las carpetas declaradas, ni una más.
- `catalogVersion` nunca baja (anti-rollback): el launcher rechaza un catálogo anterior al visto.

El launcher nunca instala un opcional cuya carpeta coincida con la de un addon obligatorio
(comparándolo con el manifest de contenido) ni borra carpetas que no haya registrado él.

## Requisitos (este Debian)

- `openssl` en `PATH`.
- Clave de firma del manifest: `~/.warcrafted/manifest-signing-key.pem`.

## Notas

- Los `.tar` **no** se versionan en git: van a la release de contenido. Solo se commitea
  `addons.json`.
- Este repositorio solo lleva el script y el catálogo ya firmado; los addons reales y sus fichas los
  aporta el equipo de contenido.
