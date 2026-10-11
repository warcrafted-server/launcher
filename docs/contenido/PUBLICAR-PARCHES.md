# Publicar parches del mod SoD

Los parches (`patch-Z.MPQ`, `patch-esES-Z.MPQ`) y el addon obligatorio RuneEngraver se publican
con `scripts/publicar-parches.sh`, sin pasos manuales.

## Flujo

1. El proyecto del servidor (`~/Repos/acore-sod`) deja una carpeta nueva
   `datos/parche-cliente/AAAAMMDD-sufijo/` con `patch-z.mpq`, `esES/patch-esES-z.mpq`,
   `RuneEngraver/` y `LEEME.txt`, crea dentro un archivo vacío `LISTO` y ejecuta el script.
2. El script toma la carpeta más reciente con `LISTO` y sin `PUBLICADO`, valida los MPQ y el
   addon, empaqueta RuneEngraver en un `.tar` determinista (mismo contenido = mismo hash) y lo
   compara todo con el manifest. Si nada cambió, marca la carpeta y termina.
3. Sube solo lo cambiado con nombre versionado (`patch-Z-<carpeta>.MPQ`,
   `RuneEngraver-<carpeta>.tar`), lo descarga para comprobar el hash, firma el manifest nuevo
   (`manifestVersion` + 1), hace commit y push a `main` y escribe `PUBLICADO` en la carpeta.
4. Conserva en GitHub los 5 últimos assets versionados de cada tipo (`--keep N`) y borra los
   anteriores; nunca el que usa el manifest vigente.

En el cliente del jugador los archivos conservan su nombre de siempre: el nombre versionado solo
existe en GitHub (el manifest separa la ruta de instalación de la URL de origen).

## Requisitos (este Debian)

- Clave de firma del manifest: `~/.warcrafted/manifest-signing-key.pem`.
- Token de GitHub fine-grained (repo `launcher`, permiso *Contents: read and write*):
  `~/.warcrafted/github-token`, permisos 600. Caduca al año: renovarlo igual y sustituir el archivo.
- Acceso SSH a GitHub (alias del remoto `origin`) para el clon de publicación; no hace falta que el
  árbol de desarrollo del launcher esté limpio ni subido.

## Clon propio de publicación

El script no toca el árbol donde se desarrolla el launcher. Firma y sube el manifest desde un clon
suyo, `~/.warcrafted/publicador-launcher` (`--workdir`), que crea la primera vez con el remoto
`origin` de este repositorio (`--origin`). En cada ejecución, bajo el bloqueo exclusivo, lo deja
idéntico a `origin/main` (`fetch`, `reset --hard`, `clean`): es un clon que solo usa el script, así
que descartar restos de una ejecución interrumpida es seguro. El commit del manifest se hace y se
sube desde ese clon; el trabajo en curso del launcher nunca se publica por error. El código del
script que se ejecuta es el del árbol de desarrollo desde el que se lanza.

El bloqueo (`~/.warcrafted/publicar-parches.lock`) espera hasta 10 min a otra publicación y se
libera siempre al terminar, con éxito o con error.

## Contrato con el servidor (`acore-sod`)

- **Lee** `~/Repos/acore-sod/datos/parche-cliente/` (`--source-root`) y nunca escribe en ella salvo
  el marcador `PUBLICADO`. Solo considera carpetas `AAAAMMDD-sufijo` que no sean enlaces simbólicos.
- **`LISTO`** (archivo vacío, lo crea el servidor): la carpeta está completa y puede publicarse. Se
  toma la de nombre más alto con `LISTO` y sin `PUBLICADO`.
- **`PUBLICADO`** (JSON, lo escribe el script): la carpeta ya se procesó. Contiene `resultado`
  (`publicado` con `manifestVersion`, `commit`, `fecha` y `assets`, o `sin cambios`). Una carpeta con
  `PUBLICADO` se ignora.
- **Salida:** código 0 si publicó o no había nada que publicar; distinto de 0 con un `Error: ...` en
  stderr si falló. Ante un fallo no se escribe `PUBLICADO` y el servidor puede reintentar con la
  misma carpeta: es idempotente (los assets ya subidos con el mismo tamaño se reutilizan). Antes del
  commit del manifest no se publica nada y los jugadores siguen con la versión anterior.
- Funciona sin intervención: no pide credenciales ni depende de otros procesos (necesita el token,
  la clave de firma y el SSH de arriba).

## Comandos

```bash
scripts/publicar-parches.sh --dry-run          # qué publicaría, sin tocar nada
scripts/publicar-parches.sh                    # publica
scripts/publicar-parches.sh --folder AAAAMMDD-sufijo   # fuerza una carpeta concreta
python3 -m unittest docs/contenido/test_publicar_parches.py
```

Ante cualquier error antes del commit no se publica nada y los jugadores siguen con la versión
anterior, que sigue siendo válida. Los assets ya subidos sin manifest que los use se limpian solos
con la retención.

## Seguridad

La firma solo se hace cuando una sesión de trabajo lanza el script; no hay ningún proceso que
vigile la carpeta y publique solo, porque cualquiera que pudiera escribir en ella publicaría
parches a todos los jugadores.
