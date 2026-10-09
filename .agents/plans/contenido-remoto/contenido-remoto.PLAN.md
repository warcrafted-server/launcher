# Plan: contenido remoto, carrusel, características y catálogo de addons

Diseño en `docs/decisiones/0006-contenido-remoto-y-alojamiento-de-addons.md` (leer primero; no
reabrir lo decidido allí). Orquestador recomendado: Sonnet, esfuerzo medio; delegar con
`agentrelay run`. Tras cada paso: versión con `node scripts/version.mjs X.Y.Z`, entrada de
CHANGELOG, TODO al día, commit; push y tags solo con aprobación. Marcar `[x]` aquí. Punto de
partida: 0.11.1.

**Bloqueado hasta que el usuario responda** las decisiones pendientes de la ADR (clave separada,
web, hosts, características reales, primeros addons). El paso 1 puede ir ya.

## Pasos

### [ ] 1. Compilador de contenido y firma — 0.12.0 (menor)
- Archivos: `scripts/compilar_contenido.py` (+ test), `scripts/publicar-contenido.sh`,
  `contenido/` con ejemplos, `docs/contenido/PUBLICAR-CONTENIDO.md`.
- Valida y compila Markdown con cabecera + YAML a `contenido.json`, firma con la clave de contenido
  (`~/.warcrafted/content-signing-key.pem`, nueva; generarla y documentar su copia), calcula
  `sha256` de imágenes, sube a la release fija `contenido-remoto`. Avisos: slides caducadas, 30 días
  sin novedades, imágenes fuera de especificación.
- Terminado: tests del compilador; `contenido.json` firmado verificable con openssl.
- Ejecutor: AgentRelay, esfuerzo alto.

### [ ] 2. Cliente de contenido remoto en el launcher — 0.13.0 (menor)
- Archivos: `src-tauri/src/remote_content.rs`, `lib.rs`, `ui_commands.rs`, `src/news.ts`.
- Descarga con ETag al arrancar y cada 6 h, firma con la clave de contenido embebida, anti-rollback
  (`contentVersion`), hosts permitidos, validación estricta, caché local y respaldo embebido,
  imágenes con `sha256` en caché. Sin red o firma inválida: caché, nunca error visible.
- Terminado: tests de firma inválida, rollback, caducidad, caché y respaldo.
- Ejecutor: AgentRelay, esfuerzo alto (seguridad).

### [ ] 3. Carrusel y características en la UI — 0.13.1 (parche) o 0.14.0
- Archivos: `src/news.ts`, `src/styles.css`.
- Carrusel con prioridad, caducidad y evergreen, imagen 16:9 con carga progresiva, etiqueta
  «Solo en WarCrafted», sección de características, estado sin conexión.
- Ejecutor: AgentRelay, esfuerzo medio.

### [ ] 4. Contenido inicial (con el usuario)
- El usuario aporta lista de características y capturas; se escriben 5 slides y las características.

### [ ] 5. Catálogo de addons v2 y política — 0.14.0 (menor)
- Esquema 2 compatible (`category`, `tags`, `recommended`, `screenshot`, `requires`, `conflicts`),
  validación en `optional_addons.rs`, script del catálogo, UI con recomendados, categorías y
  buscador, `docs/contenido/POLITICA-ADDONS.md` (licencia, revisión, 3.3.5a). Añadir el host nuevo
  si se decide.
- Ejecutor: AgentRelay, esfuerzo alto.

### [ ] 6. Primeros addons reales
- Revisión de licencia y código de cada addon, empaquetado y publicación en la release `addons`.

## Bloqueos

(ninguno)
