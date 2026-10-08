# WarCrafted Launcher

Launcher de escritorio oficial del ecosistema **WarCrafted**, un servidor privado de World of
Warcraft: Wrath of the Lich King (build 3.3.5a / 12340). Es el punto de entrada del jugador:
detecta y prepara el cliente, descarga y verifica parches y addons obligatorios, repara archivos
dañados y lanza el juego, sin que el jugador tenga que gestionar nada a mano.

> **Estado del proyecto: en desarrollo activo (versión 0.1.0), sin instalador ni release todavía.** Este README
> describe lo que existe hoy, no un objetivo final. El estado detallado y los próximos pasos
> viven en [`docs/ESTADO.md`](docs/ESTADO.md).

## Qué hace (objetivo)

- Detecta si el cliente de WoW está instalado y en qué estado se encuentra.
- Verifica cada archivo del cliente, parches y addons obligatorios contra sus hashes SHA-256
  firmados, para detectar archivos ausentes, modificados o corruptos.
- Descarga solo lo que falta o no coincide, con reintentos y reanudación.
- Bloquea el botón de Jugar mientras el cliente no esté en un estado válido.
- Corrige el `realmlist` automáticamente para apuntar al reino correcto.
- Distingue claramente entre archivos/addons **obligatorios** (gestionados por el sistema de
  integridad, sin posibilidad de desactivarlos) y **opcionales** (addons que el jugador elige).
- Sección de noticias y enlaces oficiales, actualizables sin recompilar el launcher.

## Qué hay implementado ahora mismo

| Pieza | Estado |
|---|---|
| `update_engine::manifest` — formato, parseo y validación del manifest de contenido (firma Ed25519, anti-rollback, protección contra path traversal) | ✅ Implementado y con tests |
| `update_engine::integrity` — verificación de hashes SHA-256 de archivos locales | ✅ Implementado y con tests |
| `update_engine::staging` — descarga con reintentos/reanudación, ensamblado de archivos fragmentados, extracción segura y aplicación atómica | ✅ Implementado y con tests |
| Comandos Tauri `check_client_status`, `update_client`, `launch_game` | ✅ Implementado |
| Interfaz: primera pantalla (comprobar, actualizar con progreso, jugar) | 🚧 Primera versión; faltan selector de carpeta, noticias, addons opcionales y configuración |
| Primer manifest de contenido real (cliente base en español) | ✅ Publicado, ver `docs/contenido/manifest.json` |

## Stack técnico

- **[Tauri 2](https://v2.tauri.app/)** (Rust + WebView del sistema): binario ligero, modelo de
  permisos explícito por comando, sin empaquetar un motor de navegador completo. Justificación
  completa en [`docs/decisiones/0001-stack-tecnologico.md`](docs/decisiones/0001-stack-tecnologico.md).
- **Backend (`src-tauri/`)**: Rust. El núcleo de actualización/integridad
  (`update_engine/`) es un módulo propio, independiente de los comandos de interfaz —
  nunca se mezcla lógica de actualización con código de UI.
- **Frontend (`src/`)**: HTML/TypeScript sin framework. Primera pantalla funcional; la identidad
  visual definitiva está pendiente.

## Arquitectura del contenido

Todo archivo que el launcher descarga (cliente, parches, addons) se describe en un **manifest
JSON firmado** (formato completo en
[`docs/decisiones/0002-formato-manifest.md`](docs/decisiones/0002-formato-manifest.md)):

- Cada archivo lleva su ruta de instalación, su hash SHA-256, su rol (`required`/`optional`) y su
  origen de descarga, como campos separados — el origen puede cambiar de proveedor sin afectar a
  la identidad real del archivo (su hash).
- El launcher nunca confía en nada por su nombre o su fecha: todo archivo descargado se verifica
  por hash antes de sustituir al anterior, y la sustitución es atómica.
- Los archivos que superan el límite de 2 GiB de GitHub Releases se reparten en fragmentos
  verificados individualmente y se reensamblan en el destino
  ([`docs/decisiones/0003-fragmentacion-archivos-grandes.md`](docs/decisiones/0003-fragmentacion-archivos-grandes.md)).
- El manifest se regenera y firma con un script
  ([`docs/contenido/generar_manifest.py`](docs/contenido/generar_manifest.py)) que calcula los
  hashes automáticamente; la clave de firma nunca vive en este repositorio.

## Infraestructura de distribución

El contenido (cliente, parches, addons) se distribuye mediante **GitHub Releases** de este mismo
repositorio, en varias releases independientes según su ciclo de vida:

- Cliente base: actualizaciones poco frecuentes.
- Parches del mod SoD: actualizaciones frecuentes, independientes del cliente base.
- Addons obligatorios (p. ej. RuneEngraver): empaquetados como archivo comprimido.

Se investigaron y descartaron como origen de descarga TeraBox, Hugging Face Hub (Datasets) y
DigiStorage: ninguno ofrece una API de descarga estable, sin ataduras a una cuenta personal y con
garantías contractuales razonables para distribuir contenido a miles de jugadores. El detalle de
cada investigación, con fuentes, queda reflejado en el historial de decisiones del proyecto.

## Desarrollo

### Preparar el entorno (automático)

Un solo comando instala lo que falte (Node.js, Rust, compilador C++, WebView2 o librerías de
sistema) y ejecuta `npm install`:

```powershell
# Windows (PowerShell, desde la raíz del repo; usa winget)
powershell -ExecutionPolicy Bypass -File scripts\setup.ps1
```

```bash
# Debian/Ubuntu (pide sudo para apt)
bash scripts/setup.sh
```

Después, abre una terminal nueva para que el PATH incluya `cargo` y `node`. Si prefieres instalar
a mano: Node.js y `npm`; Rust con `rustup`; en Windows, *Build Tools for Visual Studio* con
«Desarrollo para el escritorio con C++» y WebView2; en Linux, `webkit2gtk-4.1`, `libsoup-3.0`,
`libayatana-appindicator3` y `librsvg2`.

### Comandos

```bash
npm install                  # dependencias del frontend
npm run tauri dev            # arranca el launcher en modo desarrollo
npm run build                # compila el frontend (TypeScript + Vite)
cd src-tauri && cargo test   # tests del backend Rust
```

## Uso

Con `npm run tauri dev` se abre la ventana del launcher (hace falta un entorno gráfico):

1. **Comprobar estado**: descarga el manifest firmado, verifica su firma y compara cada archivo
   del cliente con su hash. Lista los archivos correctos, ausentes o modificados.
2. **Actualizar**: descarga y repara solo lo que falta o no coincide, con barra de progreso.
3. **Jugar**: se habilita tras la primera comprobación y lanza `Wow.exe` únicamente si todos los
   archivos obligatorios son válidos.

Por ahora el cliente se busca en la ruta fija `./warcrafted-client` (relativa a `src-tauri/` en
modo desarrollo). La primera actualización completa descarga unos 18,5 GB: conviene apuntar a un
disco con espacio. Un selector de carpeta llegará en una versión posterior.

## Versionado y cambios

El proyecto usa SemVer `X.Y.Z`; el procedimiento está en [`docs/VERSIONADO.md`](docs/VERSIONADO.md)
y el historial en [`CHANGELOG.md`](CHANGELOG.md).

## Documentación del proyecto

- [`docs/encargo-original.md`](docs/encargo-original.md) — el encargo completo, origen de todas
  las decisiones de producto y arquitectura.
- [`docs/decisiones/`](docs/decisiones/) — decisiones de arquitectura (ADR), una por archivo, con
  fecha, alternativas consideradas y motivo.
- [`docs/investigacion/`](docs/investigacion/) — informes de investigación con fuentes citadas.
- [`CHANGELOG.md`](CHANGELOG.md) y [`docs/VERSIONADO.md`](docs/VERSIONADO.md) — historial de
  versiones y cómo se publican.
- [`TODO.md`](TODO.md) — lo que falta, por orden de prioridad.
- [`docs/ESTADO.md`](docs/ESTADO.md) — estado actual, qué toca ahora y cómo retomar el trabajo.

## Licencia

Pendiente de decidir. Todas las dependencias se auditan (incluidas las transitivas) para no
imponer condiciones que limiten la elección final de licencia del proyecto.
