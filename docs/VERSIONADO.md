# Versionado

El launcher usa [SemVer](https://semver.org/lang/es/) `X.Y.Z`. Mientras `X` sea `0`, el proyecto
está en desarrollo inicial y cualquier versión menor puede incluir cambios incompatibles.

| Parte | Cuándo sube |
|---|---|
| `X` (mayor) | Cambio incompatible: formato de manifest, configuración o comportamiento que obliga a actualizar a la vez launcher y contenido |
| `Y` (menor) | Funcionalidad nueva compatible |
| `Z` (parche) | Corrección de errores sin funcionalidad nueva |

## Dónde vive la versión

Una sola versión en cuatro archivos, que deben coincidir siempre:
`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` y `src-tauri/Cargo.lock`.
No se editan a mano: se usa el script.

```bash
node scripts/version.mjs 0.2.0     # fija la versión en los cuatro archivos
node scripts/version.mjs --check   # comprueba que coinciden
```

La versión del launcher no es la `manifestVersion` del manifest de contenido (esa sube cuando se
publica contenido nuevo, ver la decisión 0002).

## Publicar una versión

1. Mover las entradas de `[Sin publicar]` en `CHANGELOG.md` a una sección nueva `[X.Y.Z] - AAAA-MM-DD`
   (el workflow usa esa sección como notas de la release).
2. `node scripts/version.mjs X.Y.Z` y comprobar con `--check`.
3. Actualizar README y `docs/ESTADO.md` si cambian instalación, uso o estado.
4. `npm run build` y `cd src-tauri && cargo test` en verde.
5. Commit `Publica la versión X.Y.Z`, push del commit y, con la aprobación del usuario, tag anotado
   `vX.Y.Z` y push del tag.
6. El workflow `.github/workflows/release.yml` se dispara con el tag: comprueba que coincide con la
   versión, compila en Windows, firma con la clave del autoactualizador y crea la release `vX.Y.Z`
   con el instalador, su `.sig` y `latest.json` (marcada como «latest»). Los launcher ya instalados
   la detectan al arrancar.

Requisitos únicos: los secrets del repositorio `TAURI_SIGNING_PRIVATE_KEY` (contenido de la clave
privada del autoactualizador) y `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Las releases de contenido
(`patch`, `RuneEngraver`, `contenido-v1`) son pre-release para que nunca sean «latest»; cualquier
release de contenido nueva debe crearse también como pre-release.

## Entre versiones

Cada commit con un cambio sustancial añade su línea en `[Sin publicar]` del `CHANGELOG.md`
(Añadido, Cambiado, Corregido, Eliminado, Seguridad) y actualiza README y documentación de
instalación y uso si procede. La versión de los archivos solo cambia al publicar.
