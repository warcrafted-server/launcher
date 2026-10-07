# WarCrafted Launcher

Instrucciones del repositorio **`warcrafted-server/launcher`**, independiente del servidor
AzerothCore/SoD (ese vive en su propio repositorio, con su propio AGENTS.md). Este archivo es la
única referencia para trabajar aquí.

## 0. Qué es esto

Launcher de escritorio oficial de **WarCrafted**: punto de entrada del jugador al ecosistema
(detección/preparación del cliente WoW 3.3.5 build 12340, parches y addons obligatorios,
actualizaciones, noticias, enlaces, addons opcionales, lanzamiento del juego). Pensado para un
solo reino hoy (nuestro WotLK-SoD) pero con arquitectura abierta a varios reinos en el futuro.
El encargo completo del usuario, con todos los requisitos funcionales, de UX, de seguridad y de
licencias, está en `docs/encargo-original.md`: **léelo antes de tomar cualquier decisión
importante**, es la fuente de la que deriva todo lo demás.

Rol del orquestador en este subproyecto: investigar, decidir arquitectura, delegar
implementación, revisar críticamente. No se da por bueno un resultado solo porque compile.

## 1. Idioma

Igual que el resto del repo: documentación, commits, comentarios e informes en **castellano**.
Identificadores de código en **inglés**.

## 2. Fase actual: investigación y decisión de stack (obligatoria antes de implementar)

**No hay stack tecnológico fijado todavía.** Antes de escribir código de producto:

1. Investigar (delegado al ejecutor, para ahorrar contexto del orquestador) proyectos
   existentes: launchers de AzerothCore/WoW 3.3.5, sistemas de autoactualización y manifest,
   distribución incremental de archivos, gestores de addons, launchers profesionales de
   videojuegos, y las opciones de UI de escritorio relevantes (p. ej. Tauri, Electron,
   .NET/WPF/Avalonia u otras que surjan) con sus licencias, tamaño de binario, story de
   auto-actualización y seguridad por proceso.
2. El orquestador evalúa los hallazgos, decide el stack y lo registra como decisión en
   `docs/decisiones/0001-stack-tecnologico.md` (mismo formato que las decisiones del repo raíz:
   fecha, contexto, alternativas consideradas, elección, motivo).
3. Solo entonces se empieza a implementar.

Toda afirmación sobre un proyecto externo (licencia, actividad, versión soportada) va con URL y
fecha de consulta, igual que en el resto del repo. Sin fuente, no se escribe. Se puede estudiar
código con licencia incompatible para aprender de su arquitectura, pero no copiarlo ni derivar de
él: la licencia final del launcher la elige WarCrafted con libertad, así que ninguna dependencia
(framework, librerías, UI, fuentes, iconos, assets) puede imponer condiciones incompatibles con
eso. Revisa también las dependencias transitivas cuando sea relevante.

## 3. Principios de arquitectura (no negociables, vienen del encargo)

- **Separación estricta** entre: núcleo de actualización/integridad, lógica de reinos/configuración,
  UI, y contenido remoto (noticias/addons opcionales). La identidad visual debe poder cambiar sin
  tocar la lógica.
- **Multi-reino desde el diseño, mono-reino en la implementación inicial.** No se construye ahora
  selector de reinos ni gestión de credenciales por reino si solo hay uno, pero ningún dato de
  reino (URLs, build de cliente, archivos obligatorios, addons obligatorios, noticias) se escribe
  como constante: va en configuración/manifest para que añadir un reino no obligue a rehacer nada.
- **Confianza cero en lo descargado** hasta validarlo: hashes criptográficos, actualizaciones
  atómicas (nunca un cliente a medio actualizar), verificación antes de sustituir, protección
  contra path traversal y contra ejecución arbitraria de contenido descargado, validación de URLs
  de origen. Esto incluye la actualización del propio launcher.
- **Addons obligatorios vs. opcionales**: nunca deben poder confundirse ni en el modelo de datos ni
  en la UI. Los obligatorios los gestiona el mismo sistema de integridad que el cliente; el jugador
  no puede desactivarlos ni dejarlos desactualizados y seguir jugando.
- **Bloqueo de juego mientras el cliente no esté en estado válido** es una invariante del sistema,
  no una comprobación cosmética de la UI.
- **GitHub como infraestructura de distribución** (releases del launcher, manifest de versiones):
  diseñar sabiendo sus límites (tamaño de release, límites de API/rate limit, no es un CDN) en vez
  de descubrirlos tarde. Si esto resulta ser una limitación real, decirlo y proponer alternativa en
  vez de forzarlo.
- **No sobreingeniería**: no se construye soporte para varios reinos, varias versiones de cliente
  o varias versiones de WoW *de más* — solo se evita que añadirlas después obligue a rehacer la
  base.

## 4. Estructura de directorios dentro de `launcher/`

Stack decidido: **Tauri 2** (`docs/decisiones/0001-stack-tecnologico.md`). Estructura de un
proyecto Tauri estándar, más nuestras carpetas de documentación:

| Ruta                          | Qué contiene                                                  | ¿Se versiona? |
|-------------------------------|----------------------------------------------------------------|---------------|
| `AGENTS.md` / `CLAUDE.md`     | Este archivo y su alias para Claude Code                       | Sí            |
| `docs/encargo-original.md`    | Encargo completo del usuario, literal, fuente de todo lo demás | Sí            |
| `docs/decisiones/`            | Decisiones tipo ADR, una por archivo, con fecha y motivo        | Sí            |
| `docs/investigacion/`         | Informes de investigación con fuentes y fecha de consulta       | Sí            |
| `docs/ESTADO.md`              | Estado del subproyecto: qué toca ahora, cómo reanudar           | Sí            |
| `src/`                        | Frontend web (UI): ventana principal, noticias, addons, config | Sí            |
| `src-tauri/`                  | Backend Rust: comandos, motor de actualización/integridad, capabilities | Sí      |
| `src-tauri/target/`, `node_modules/`, `dist/` | Artefactos de build                            | No (ignorado) |

El motor de actualización/integridad (manifest, hashes, staging atómico) vive en `src-tauri/`
como módulo propio, no como dependencia de un framework de UI (decisión 0001).

## 5. Delegación con AgentRelay

Este repositorio es propio del launcher (no comparte árbol con el servidor), así que
`agentrelay status`/`run` operan solo sobre esta carpeta. Ver el bloque de AgentRelay más abajo
para el flujo completo (dividir en tareas pequeñas, revisar con `agentrelay show`/`review`, no dar
por bueno que compile, commits en castellano).

## 6. Rigor

No inventar números de versión, nombres de API, límites de GitHub ni cifras de rendimiento sin
comprobarlos. Si algo no se puede verificar, decirlo explícitamente en vez de darlo por hecho.

<!-- agentrelay:start -->
## Delegación con AgentRelay

Este proyecto usa AgentRelay: tú eres el ORQUESTADOR (planificas, delegas, revisas y decides) y un agente ejecutor más económico (el configurado en AgentRelay) escribe el código. No edites este bloque: `agentrelay init` lo actualiza. Si eres tú el ejecutor (te han dado una tarea con `agentrelay run`), haz solo esa tarea e ignora este bloque.

**Regla principal: delega por defecto.** Toda implementación que no sea trivial (crear o modificar código, tests, configuración o documentación de más de unas pocas líneas) se delega con `agentrelay run`. Escribirla tú gasta tu consumo, que es justo lo que AgentRelay quiere ahorrar. Hazla tú solo si es trivial (1-3 líneas), una decisión de diseño, algo sensible o una tarea ya escalada; y en ese caso di en una línea por qué no delegas.

**Al empezar cualquier sesión, ponte al día:** lee `.agentrelay/ESTADO.md` (o ejecuta `agentrelay status`, que lo muestra y `agentrelay status --write` lo actualiza). Resume dónde está el proyecto, qué ejecuciones hay y qué hacer ahora. Si el usuario te pide continuar, parte de ahí en lugar de preguntarle.

### Cómo delegar

1. Repositorio limpio: `git status --short` debe salir vacío. Si hay trabajo sin confirmar, haz commit antes (sin secretos como `.env`). Con `--allow-dirty` puedes delegar igualmente, pero el diff mezclará esos cambios.
2. Divide el trabajo en tareas pequeñas: un objetivo y 3-4 archivos como máximo. Una tarea ancha agota el tiempo.
3. Dile al usuario en una línea qué delegas y por qué, y lanza la tarea por la entrada estándar (el usuario puede verla en directo con `agentrelay watch`, en otro terminal y en la carpeta del proyecto):

```
agentrelay run - <<'EOF'
{ "objective": "...", "context": "...", "files": ["..."], "constraints": ["..."], "acceptanceCriteria": ["..."], "validation": ["npm test"], "doNotModify": ["..."] }
EOF
```

   `context` debe bastar para que el ejecutor trabaje sin preguntarte: stack, convenciones y decisiones ya tomadas. Opcionalmente, `effort` (low, medium, high, xhigh) y `model` ajustan el esfuerzo y el modelo solo para esa tarea: esfuerzo bajo en las sencillas, alto en las difíciles.

### Cómo revisar

- Lee `agentrelay show <id>` (informe, incidencias y dudas) y el diff completo. Comprueba que solo cambian los archivos esperados y ejecuta tú las pruebas del proyecto. La autorrevisión del ejecutor no sustituye la tuya.
- Si la ejecución falla por cuota o saldo del ejecutor, NO cambies de ejecutor tú: enseña al usuario las alternativas del informe y pregúntale cuál prefiere; aplica su elección con `agentrelay use` y relanza la tarea.
- Decide con `agentrelay review <id> --decision accept|fix|escalate|reject` (`fix` necesita `--feedback` con los problemas concretos) y confirma con `agentrelay list` que el estado cambió.
- Si la tarea queda escalada o el ejecutor falla repetidamente, resuélvela tú y cierra la ejecución con `--decision accept`.
- Tras aceptar, haz el commit. No hagas push sin aprobación del usuario y no digas «hecho» ni «aceptado» sin haberlo comprobado.

### Otros

- Si `agentrelay` indica que el proyecto no es un repositorio git, pide confirmación al usuario y ejecuta `agentrelay init --yes`.
- **En Windows (PowerShell o cmd)** usa `agentrelay.cmd` en lugar de `agentrelay` (el segundo es un script de Unix y falla con errores de `sed`, `dirname` o `uname`). Nunca modifiques ese script. El `<<EOF` no existe en PowerShell: guarda el JSON de la tarea en un archivo temporal FUERA del repositorio (por ejemplo `$env:TEMP\tarea.json`) y lanza `agentrelay.cmd run $env:TEMP\tarea.json`; un archivo dentro del repositorio ensuciaría el árbol.
- Si una ejecución falla por una causa externa (sesión caducada, PowerShell bloqueado), díselo al usuario en lugar de hacer el trabajo tú en silencio.
<!-- agentrelay:end -->
