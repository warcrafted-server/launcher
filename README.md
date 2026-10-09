# WarCrafted Launcher

Launcher de escritorio oficial del ecosistema **WarCrafted**, un servidor privado de World of
Warcraft: Wrath of the Lich King (build 3.3.5a / 12340). Es el punto de entrada del jugador:
detecta y prepara el cliente, descarga y verifica parches y addons obligatorios, repara archivos
dañados y lanza el juego, sin que el jugador tenga que gestionar nada a mano.

> **Estado del proyecto: en desarrollo activo (versión 0.3.0), instalador configurado pero sin release publicada todavía.** Este README
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

### Instalador de Windows

`npm run tauri build` en Windows genera un instalador NSIS por usuario (sin administrador). Detalle
para jugadores y para compilarlo en [`docs/INSTALACION.md`](docs/INSTALACION.md).

## Uso

Con `npm run tauri dev` se abre la ventana del launcher (hace falta un entorno gráfico):

1. **Elegir carpeta…**: indica dónde está (o dónde se instalará) el cliente de WoW. Se guarda en
   los ajustes del launcher y se recuerda en la próxima ejecución. Si la carpeta está vacía, la
   primera actualización descarga el cliente completo (unos 18,5 GB).
2. **Comprobar estado**: descarga el manifest firmado, verifica su firma y compara cada archivo
   del cliente con su hash (incluido `Wow.exe`, lo que garantiza el build 12340). Lista los
   archivos correctos, ausentes o modificados.
3. **Actualizar**: descarga y repara solo lo que falta o no coincide, con barra de progreso.
4. **Borrar caché**: elimina la carpeta `Cache` del cliente (con el juego cerrado).
5. **Jugar**: se habilita tras la primera comprobación y lanza `Wow.exe` únicamente si todos los
   archivos obligatorios son válidos.

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
- [`docs/contenido/PUBLICAR-PARCHES.md`](docs/contenido/PUBLICAR-PARCHES.md) — cómo se publican parches y addons.
- [`docs/ESTADO.md`](docs/ESTADO.md) — estado actual, qué toca ahora y cómo retomar el trabajo.

## Créditos y licencia

**WarCrafted Launcher** está desarrollado y mantenido por **WarCrafted**.
Copyright (C) 2026 WarCrafted.

Licencia [WNCL-TP-1.0](LICENSE): puedes usarlo, estudiarlo, modificarlo y compartirlo gratis con fines no comerciales. Venderlo o cualquier uso comercial requiere permiso por escrito. Solo cubre el material propio; los componentes de terceros conservan su licencia.

Las copias y los forks deben conservar esta atribución y enlazar a https://github.com/warcrafted-server/launcher.

### Material de terceros

Ver [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md): 506 componentes de Rust y 3 de JavaScript, todos con licencias permisivas (MIT, Apache-2.0 y similares; cinco de Tauri bajo MPL-2.0, sin modificar). El logotipo y el icono quedan fuera del alcance de la licencia; ver [`CREDITS.md`](CREDITS.md).

Créditos completos: [CREDITS.md](CREDITS.md).
