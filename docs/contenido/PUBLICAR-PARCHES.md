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
- `main` del launcher limpio e igual a `origin/main`: si hay commits sin subir, el script se niega
  a publicar para no subir trabajo ajeno sin aprobación.

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
