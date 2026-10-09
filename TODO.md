# TODO del launcher

Propuesta de prioridades, pendiente de revisión del usuario. Deriva de
[`docs/encargo-original.md`](docs/encargo-original.md). Es la lista viva de lo que falta: al
completar algo se quita de aquí y se anota en el [`CHANGELOG.md`](CHANGELOG.md). El estado técnico
detallado está en [`docs/ESTADO.md`](docs/ESTADO.md).

Leyenda: **(decisión)** requiere que el usuario decida.

Plan de ejecución de distribución, autoactualización y descargas:
[`.agents/plans/distribucion/distribucion.PLAN.md`](.agents/plans/distribucion/distribucion.PLAN.md).

## Prioridad 1 — que un jugador pueda usarlo de verdad

- [ ] Mensajes claros ante fallos de red durante la descarga
- [ ] Logging a archivo, útil para soporte
- [ ] Probar el recorrido completo en Windows: descarga, reparación, `Wow.exe` arrancando con los
      parches y addon aplicados

## Prioridad 2 — producto distribuible

- [ ] Icono de la aplicación: usar el emblema cuadrado «WarCrafted Universe» (1024×1024, fondo de
      piedra oscura) que el usuario ha enseñado en la sesión. Falta el archivo en el repo: pedirlo
      o que lo copie a `src/assets/`, y luego `npx tauri icon <archivo>` (hoy los iconos salen del
      logo apaisado con `--fit contain`). Misma duda de licencia que el logotipo (ver abajo).

- [ ] **(decisión)** Firma de código de Windows: sin ella SmartScreen avisará al instalar
- [ ] **(decisión)** Clave de firma del manifest en más equipos (hoy solo en el Debian de casa)
- [ ] Mover el origen de descarga si GitHub se queda corto (límites de ancho de banda no medidos
      todavía) **(decisión)** dónde alojar el cliente a largo plazo
- [ ] Descarga en paralelo, para acortar la primera instalación

## Prioridad 3 — experiencia y contenido

- [ ] Noticias remotas: `noticias.json` firmado publicado como el manifest, en lugar del ejemplo
- [ ] Enlaces oficiales (web, foro, base de datos), ampliables sin recompilar
- [ ] Publicar los primeros addons opcionales reales en el catálogo (`docs/contenido/PUBLICAR-ADDONS.md`)
- [ ] **(decisión)** Identidad visual: logo, paleta, tipografía, artwork (sin copiar a Blizzard);
      revisar la licencia de fuentes e iconos
- [ ] Animaciones, estados de error cuidados y escalado DPI de Windows
- [ ] Música de fondo opcional con botón de silencio (pista con licencia utilizable)
- [ ] Idioma: hoy solo `esES`; valorar `enUS`

## Prioridad 4 — preparación a futuro

- [ ] Configuración de reinos en el manifest/config remoto (selector solo cuando haya más de uno)
- [ ] Integridad del contenido ya extraído de un `.tar` (hoy solo se verifica el hash del archivo)
- [ ] Tests de integración del flujo de actualización y de la UI

## Pendientes administrativos

- [ ] Crear en GitHub (Settings → Secrets → Actions) `TAURI_SIGNING_PRIVATE_KEY` (contenido de
      `~/.warcrafted/updater.key`) y `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: sin ellos el workflow de
      release falla al firmar. Los secrets solo los puede crear el usuario.
- [ ] **(decisión)** Logotipo: averiguar qué herramienta de IA lo generó y si sus condiciones
      permiten el uso y la redistribución; hasta entonces está excluido de la licencia
      (`CREDITS.md`). Resolverlo antes de publicar el instalador.
- [ ] **(decisión)** Tags `vX.Y.Z`: los dispara el usuario (o con su aprobación) y el workflow de release publica
      la versión
