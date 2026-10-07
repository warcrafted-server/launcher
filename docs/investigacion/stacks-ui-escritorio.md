# Stacks de UI de escritorio para WarCrafted Launcher

Consulta de fuentes: 2026-10-07. Este documento compara alternativas y sus consecuencias. La elección del stack corresponde al orquestador; no fija arquitectura ni estructura de código.

## Necesidades del producto

- Windows primero; distribución sencilla para el jugador y una ventana visual cuidada, con escalado DPI y controles de actualización/descarga.
- Mucho trabajo de filesystem y red: inventariar, verificar y reparar archivos grandes sin bloquear la UI.
- La UI presenta noticias y catálogo remoto, pero el renderer no debe recibir permisos arbitrarios para escribir archivos o ejecutar contenido.
- Autoactualización segura del launcher y una interfaz estable para un motor de actualización independiente.
- Mantener abierta una salida a otros sistemas en el futuro, sin pagar hoy con soporte multiplataforma innecesario.
- Libertad para escoger licencia del producto. Auditar dependencias transitivas, assets, fuentes, iconos y herramientas, no solo el framework principal.

## Alternativas

| Criterio | Tauri 2 | Electron | .NET: WPF | .NET: Avalonia |
|---|---|---|---|---|
| Modelo de UI y lógica | UI HTML/CSS/JS con backend nativo Rust; la webview usa motor del sistema. | HTML/CSS/JS sobre Chromium y Node.js; procesos principal, preload y renderer. | UI XAML/controles nativos de Windows sobre .NET; interfaz y lógica pueden separarse con MVVM. | UI XAML/controles de Avalonia sobre .NET; renderer propio multiplataforma. |
| Windows-first y evolución | Windows y otros sistemas; exige Rust y WebView2. | Windows/macOS/Linux con un código web común y tooling muy extendido. | Solo Windows; WPF existe desde hace años y es maduro para escritorio y herramientas. | Multiplataforma desde el mismo código .NET/XAML; no obliga a publicar otras plataformas ahora. |
| UX visual | CSS ofrece libertad para componer identidad visual; WebView2 puede dar diferencias/versiones de runtime entre equipos. | Máxima familiaridad para diseño web y biblioteca de componentes; exige mantener la UI accesible y adaptativa. | XAML, estilos, plantillas, gráficos vectoriales, animación, enlace de datos y DPI independientes de resolución documentados. Permite interfaz muy cuidada con componentes propios. | XAML, estilos y temas customizables; facilita compartir diseño multiplataforma, con riesgo de que controles no coincidan exactamente con cada sistema. |
| Actualización del launcher | Plugin oficial firmado; feed estático o servidor. En Windows instala paquete/instalador y cierra la app. | AutoUpdater integrado con Squirrel.Windows y Squirrel.Mac; soporte integrado actual de Windows/macOS, sin Linux. Se requiere backend/feed y firma según plataforma. | Framework no incluye estrategia de updater de producto. Velopack ofrece instalador, delta y auto-update para apps .NET; WPF puede integrarlo. MSIX/App Installer es otra opción Windows con requisitos de identidad/paquete. | Mismo Velopack disponible para C#/Avalonia; desacoplar updater de UI. Avalonia no impone una solución de actualización. |
| Tamaño de instalación | Puede evitar incluir un runtime de navegador completo porque usa el del sistema. WebView2 debe estar presente o instalarse; incluir Evergreen/offline agrega tamaño. Binario final depende de Rust, frontend y paquetes. | Empaqueta el binario de Electron junto con la app; Chromium/Node amplían el contenido instalado y la memoria en ejecución frente a una UI nativa, aunque las páginas oficiales no dan una cifra comparable por app. | Framework-dependent deja el runtime Desktop instalado como requisito; self-contained incluye runtime y suele ocupar más. Single-file y compresión cambian tamaño final y arranque. | Similar a .NET: publish self-contained o framework-dependent; depende del runtime .NET, backend gráfico y assets incluidos. No se midió un artefacto real. |
| Seguridad por proceso | Comandos Rust/capabilities con permisos y scopes explicitados por ventana. IPC tiene fronteras, pero XSS puede invocar permisos concedidos: mantener UI local y alcance mínimo. WebView2 renderer es proceso de navegador sandboxed. | Separación fuerte main/renderer si se activa context isolation, sandbox, nodeIntegration desactivado y bridge IPC mínimo. En caso contrario, XSS puede escalar a APIs del sistema. | WPF nativo no introduce renderer web por defecto. UI y lógica comparten proceso salvo que se dividan explícitamente; trabajos privilegiados y updater pueden ir a proceso auxiliar. | UI nativa en proceso .NET por defecto. Para aislar tareas, separar proceso/servicio. No asumir aislamiento de sandbox de navegador por usar Avalonia. |
| Licencia del framework y principales dependencias | Tauri: MIT o Apache-2.0, según componente. API/plugins siguen avisos propios; dependencias Rust requieren inventario. WebView2 es runtime de Microsoft instalado en Windows, no una dependencia MIT del proyecto. | Electron: MIT. Electron distribuye binarios que incorporan Chromium y Node; esas piezas incluyen sus propias licencias/avisos de terceros. Cada paquete npm adicional conserva su licencia. | WPF moderno y .NET: código fuente MIT; distribución Windows de .NET se licencia bajo Microsoft .NET Library License según la política oficial de .NET. Framework dependiente frente a self-contained altera lo distribuido, no la identidad de licencia del proyecto. | Avalonia UI framework: MIT. Algunas herramientas/UI comerciales recientes tienen licencias separadas; usar solo framework y herramientas open source evita dependencia de licencia comercial. .NET mantiene las condiciones citadas para distribución Windows. |
| Autoactualización por sí sola | Tiene API integrada; comprobar firma siempre, proteger clave privada. No soluciona manifest de archivos del cliente. | Integrada para el ejecutable, con feed y publicación separados; no cubre patcher del cliente. | Requiere escoger herramienta y diseñar firma/recovery. Velopack es alternativa activa y transversal; Squirrel.Windows es legado útil como referencia, pero su guía declara ausencia de rollback incorporado. | Requiere herramienta externa al framework de UI; Velopack ofrece integración documentada tanto para WPF como Avalonia. |

## Licencias y cadena de suministro

- **Tauri:** el proyecto publica MIT y Apache-2.0 «donde corresponda». Los plugins oficiales y crates usados deben comprobarse por versión en el lockfile. La webview Windows depende de WebView2 de Microsoft; documentación Tauri dice que Windows 10 1803+ normalmente ya lo trae, pero esta experiencia y distribución fuera de esa base necesitan comprobarse en los equipos objetivo.
- **Electron:** framework MIT y tooling Forge para empaquetar/distribuir. El empaquetado incluye el binario Electron. Deben distribuirse y conservarse los avisos de Electron/Chromium/Node y los de npm; no basta con etiquetar toda la aplicación «MIT». La guía oficial de seguridad pide actualizar Electron y evaluar dependencias.
- **WPF/.NET:** WPF es técnicamente completo para escritorio, pero se limita a Windows. El código de .NET es MIT; Microsoft distingue que las distribuciones de producto Windows tienen licencia Microsoft .NET Library. Si se publica self-contained, mantener avisos de runtime y componentes empaquetados.
- **Avalonia:** framework MIT y soporta varias plataformas. FAQ oficial aclara que su framework sigue siendo MIT, mientras que nueva herramienta profesional y algunos componentes UI tienen otra licencia; los instrumentos heredados siguen publicados bajo MIT. Elegir controles externos uno por uno y no asumir que el framework hace MIT a toda dependencia.
- **Velopack:** licencia MIT y empaqueta/actualiza aplicaciones cross-platform, genera deltas y admite integraciones C#, WPF y Avalonia documentadas. La firma de Authenticode y gestión de claves siguen siendo responsabilidad operativa.
- **Activos:** fuentes web, fuentes del sistema, iconos, ilustraciones y cada paquete npm/NuGet/crate son una cadena separada. Añadir SBOM/inventario de licencias al proceso de release antes de distribuir.

## Autoactualización no es patching del cliente

Tauri Updater, Electron autoUpdater y Velopack actualizan la aplicación de escritorio. Ninguno resuelve el estado de MPQ, el build del cliente, addons obligatorios, las descargas parciales o la regla de bloquear Jugar. Ese motor debe tener contrato propio, autoridad de manifest, validación hash/firma y operación recuperable, con independencia de la UI elegida.

## Comparación sintética para discusión

| Prioridad dominante | Alternativa que la favorece | Coste principal |
|---|---|---|
| Control de UI web con binario de host pequeño y permisos acotados | Tauri | Rust, WebView2 y más responsabilidad de diseño de IPC/permisos. |
| Ecosistema web más amplio y APIs desktop maduras | Electron | Empaqueta Chromium/Node, mayor artefacto base y obliga a mantener disciplina de seguridad del renderer. |
| Windows nativo, C# y XAML con estabilidad de plataforma | WPF | Solo Windows y no es la ruta que Microsoft recomienda para nuevas apps nativas (su guía actual recomienda WinUI 3); hay que justificar WPF frente a WinUI 3. |
| C# / XAML con opción real de publicar fuera de Windows | Avalonia | Dependencia del roadmap del framework y de componentes propios; experiencia nativa no idéntica entre plataformas. |

## Aspectos que requieren medir antes de elegir

No hay comparación pública homogénea de tamaño de instalador entre frameworks que incluya la misma pantalla, assets, updater, runtime, WebView2 y modo self-contained. No se deben repetir cifras de marketing o repositorios ajenos como resultado de WarCrafted. Construir un prototipo equivalente en cada opción finalista y medir tamaño comprimido, memoria, arranque, calidad DPI, accesibilidad y tiempo de trabajo sería más representativo; la prueba puede dejarse para después de una decisión inicial.

Evaluar como mínimo Windows 10/11 64-bit, Windows sin conexión durante primer inicio, WebView2 ausente/viejo, escalados 100–200 %, monitor secundario, arranque de actualización, firma Authenticode y una descarga grande concurrente a UI.

## Incertidumbres

- No se han medido tamaño binario, memoria o startup de WarCrafted en ninguno de los stacks. Las comparaciones de tamaño arriba describen qué runtime se distribuye, no un número.
- La lista transitiva exacta de Tauri, Electron, .NET y Avalonia depende del conjunto de paquetes/versión elegido. Ejecutar auditoría de licencias sobre los lockfiles reales y revisar avisos redistribuidos antes de publicar.
- No se revisaron condiciones comerciales, cláusulas de marca, certificados de firma ni requisitos de Microsoft Store para un eventual canal MSIX. La licencia de runtime WebView2 y de Windows App SDK debe verificarse según modelo de distribución concreto.
- La recomendación Microsoft actual de WinUI 3 aparece como contexto pertinente a Windows-first, pero esta comparativa se centra en WPF y Avalonia como variantes .NET solicitadas. No se hizo prueba práctica de WinUI 3 ni evaluación de su ciclo de distribución.
- El updater elegido debe revisarse por versión fijada, soporte de instaladores y salida ante pérdida/rotación de clave; las páginas de documentación son vivas.

## Fuentes

- [Tauri: repositorio y licencia](https://github.com/tauri-apps/tauri), consultado 2026-10-07; [prerrequisitos y WebView2](https://v2.tauri.app/start/prerequisites/), [permisos](https://v2.tauri.app/security/permissions/), [updater y firma](https://v2.tauri.app/plugin/updater/), consultados 2026-10-07.
- [Electron: licencia y arquitectura declarada](https://github.com/electron/electron), consultado 2026-10-07; [seguridad](https://www.electronjs.org/docs/latest/tutorial/security/), [context isolation](https://www.electronjs.org/docs/latest/tutorial/context-isolation), [autoUpdater](https://www.electronjs.org/docs/latest/api/auto-updater), [empaquetado](https://www.electronjs.org/docs/latest/tutorial/tutorial-packaging), consultados 2026-10-07.
- [Microsoft: WPF overview y solo Windows](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/overview/), consultado 2026-10-07; [licencia de .NET](https://github.com/dotnet/core/blob/main/license-information.md), [despliegue runtime Windows](https://learn.microsoft.com/en-us/dotnet/core/install/windows), consultados 2026-10-07.
- [Microsoft: WinUI 3 para nuevas apps Windows](https://learn.microsoft.com/en-us/windows/apps/get-started/), consultado 2026-10-07.
- [Avalonia: repositorio/licencia](https://github.com/AvaloniaUI/Avalonia), [FAQ de licencias de framework y tooling](https://docs.avaloniaui.net/tools/faq), consultados 2026-10-07.
- [Velopack: repositorio/licencia](https://github.com/velopack/velopack), [deltas](https://docs.velopack.io/packaging/deltas), [integraciones WPF/Avalonia](https://docs.velopack.io/category/sample-apps), consultados 2026-10-07.
- [Sparkle: plataforma, licencia y funcionalidades](https://sparkle-project.org/), consultado 2026-10-07; [Squirrel.Windows: proyecto y proceso de actualización](https://github.com/Squirrel/Squirrel.Windows/blob/develop/docs/using/update-process.md), consultados 2026-10-07.
- [Microsoft: WebView2 runtime, procesos y actualizaciones](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/end-user-faq), consultado 2026-10-07; [renderer con Low Integrity](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/measures), consultado 2026-10-07.
