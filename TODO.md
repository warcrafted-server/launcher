# TODO del launcher

Propuesta de prioridades, pendiente de revisión del usuario. Deriva de
[`docs/encargo-original.md`](docs/encargo-original.md). Es la lista viva de lo que falta: al
completar algo se marca aquí y se anota en el [`CHANGELOG.md`](CHANGELOG.md). El estado técnico
detallado está en [`docs/ESTADO.md`](docs/ESTADO.md).

Leyenda: `[x]` hecho · `[ ]` pendiente · **(decisión)** requiere que el usuario decida.

## Hecho (v0.1.0)

- [x] Manifest firmado (Ed25519), anti-rollback, archivos fragmentados y addons `.tar`
- [x] Verificación de integridad SHA-256, descarga con reintentos/reanudación, extracción segura y
      aplicación atómica con rollback
- [x] Comandos `check_client_status`, `update_client` (progreso) y `launch_game`
- [x] Bloqueo de juego si falta o falla un archivo obligatorio (invariante en el backend)
- [x] Reparación de archivos dañados (es la misma ruta que actualizar)
- [x] Descarga del cliente completo desde cero (probado de extremo a extremo en Linux, 44/44)
- [x] `realmlist.wtf` gestionado como archivo del manifest
- [x] Publicación automática de parches y addon desde `acore-sod` (`scripts/publicar-parches.sh`)
- [x] Primera pantalla funcional, CHANGELOG, versionado SemVer y scripts de preparación del entorno

## Prioridad 1 — que un jugador pueda usarlo de verdad

- [x] Ruta del cliente configurable y persistente (selector de carpeta, guardada en ajustes)
- [x] Build 12340 garantizado por la integridad: el hash de `Wow.exe` está en el manifest
- [ ] Espacio libre en disco comprobado antes de descargar y aviso del tamaño (~18,5 GB)
- [ ] Progreso de descarga en bytes, con velocidad y tiempo restante (la verificación ya muestra
      bytes; la descarga aún va por archivos)
- [ ] Verificación rápida: recordar tamaño y fecha de los archivos ya verificados para no volver a
      calcular 18,5 GB de hashes en cada comprobación
- [ ] Cancelar y reanudar una actualización desde la UI; mensajes claros ante fallos de red
- [ ] Conservar las descargas parciales entre ejecuciones (hoy un archivo cortado empieza de cero
      al volver a abrir el launcher) y limpiar carpetas temporales huérfanas
- [x] Opción de borrar caché
- [ ] Logging a archivo, útil para soporte
- [ ] Probar el recorrido completo en Windows: descarga, reparación, `Wow.exe` arrancando con los
      parches y addon aplicados
- [x] Quitar el comando `greet` sobrante del scaffold

## Prioridad 2 — producto distribuible

- [ ] Instalador de Windows (MSI/NSIS) con WebView2 incluido o bootstrapper
- [ ] Autoactualización del propio launcher (updater de Tauri con firma propia)
- [ ] CI/CD en GitHub Actions: build, tests y publicación de releases por tag `vX.Y.Z`
- [ ] **(decisión)** Firma de código de Windows: sin ella SmartScreen avisará al instalar
- [ ] **(decisión)** Clave de firma del manifest en más equipos (hoy solo en el Debian de casa)
- [ ] Mover el origen de descarga si GitHub se queda corto (límites de ancho de banda no medidos
      todavía) **(decisión)** dónde alojar el cliente a largo plazo
- [ ] Descarga en paralelo, para acortar la primera instalación

## Prioridad 3 — experiencia y contenido

- [ ] Navegación: Jugar, Noticias, Addons, Configuración
- [ ] Noticias remotas (imagen, título, resumen, fecha, enlace; varias y una destacada)
- [ ] Enlaces oficiales (web, foro, base de datos), ampliables sin recompilar
- [ ] Addons opcionales: descubrir, instalar y actualizar, separados de los obligatorios en datos y
      en UI
- [ ] **(decisión)** Identidad visual: logo, paleta, tipografía, artwork (sin copiar a Blizzard);
      revisar la licencia de fuentes e iconos
- [ ] Animaciones, estados de error cuidados y escalado DPI de Windows
- [ ] Idioma: hoy solo `esES`; valorar `enUS`

## Prioridad 4 — preparación a futuro

- [ ] Configuración de reinos en el manifest/config remoto (selector solo cuando haya más de uno)
- [ ] Integridad del contenido ya extraído de un `.tar` (hoy solo se verifica el hash del archivo)
- [ ] Tests de integración del flujo de actualización y de la UI

## Pendientes administrativos

- [ ] **(decisión)** Licencia del proyecto (usar la Skill `licencias`; nada se aplica sin tu orden)
- [ ] **(decisión)** Crear y subir el tag `v0.1.0`
- [ ] Añadir la regla global de versionado y changelog al repo `dotclaude` (no está clonado en este
      equipo)
