# 0001 — Stack tecnológico del launcher: Tauri 2

**Fecha:** 2026-10-07. **Estado:** decidida.

## Contexto

El launcher necesita una UI de escritorio Windows-first con aspecto de producto comercial
(animaciones, jerarquía visual, estados de descarga bien diseñados), un motor de
actualización/integridad que trate todo contenido descargado como no confiable, y libertad total
de licencia para el proyecto. No había código previo ni decisión de stack; ver la investigación en
`docs/investigacion/` (consultada 2026-10-07) para el detalle de cada alternativa.

## Alternativas consideradas

- **Tauri 2** (Rust + webview del sistema): binario pequeño porque no empaqueta Chromium/Node,
  modelo de permisos explícito por comando (capabilities), licencia MIT/Apache-2.0. Depende de
  WebView2 en Windows (ya presente en Windows 10 1803+ según su propia documentación, a confirmar
  en los equipos objetivo reales). Updater oficial firmado para el propio launcher.
- **Electron**: ecosistema web más maduro y extendido, pero empaqueta Chromium+Node completos
  (binario e instalación mucho mayores, más superficie de ataque), y exige disciplina estricta de
  `contextIsolation`/sandbox para no dejar que el renderer web escale a APIs del sistema.
- **WinUI 3 / WPF (.NET)**: nativo de Windows. El launcher de AzerothCore más serio encontrado en la
  investigación (`nickk02/azerothcore-launcher`, activo, MIT) usa WinUI 3, que es además la
  recomendación actual de Microsoft frente a WPF para apps nuevas. Cierra la puerta a soporte futuro
  de otros sistemas operativos y su ecosistema de componentes visuales es más limitado para lograr
  la identidad visual "de juego comercial" que pide el encargo.
- **Avalonia (.NET)**: multiplataforma desde el mismo código XAML, MIT, con Velopack como updater
  documentado. Buena alternativa de respaldo, pero sin la ventaja de permisos por capability de
  Tauri y con un ecosistema de componentes UI web-like menos maduro que CSS/HTML para lograr
  diseño visual cuidado.

Comparativa completa, con fuentes y fecha de consulta, en
`docs/investigacion/stacks-ui-escritorio.md`.

## Decisión

**Tauri 2** para la aplicación de escritorio (ventana, UI, lanzamiento del juego, orquestación
visual de noticias/addons/configuración).

Motivos:

1. **Seguridad por diseño, no por disciplina.** El principio no negociable del encargo es
   "confianza cero en lo descargado". El modelo de capabilities de Tauri obliga a declarar
   explícitamente qué puede hacer cada comando (filesystem, red, procesos); el frontend web no
   tiene acceso directo a nada salvo lo concedido. Esto encaja mejor con ese principio que Electron
   (donde hay que mantener activamente `contextIsolation`/sandbox para lograr el mismo aislamiento)
   o que un stack .NET sin separación de proceso por defecto.
2. **Tamaño e impresión de producto.** Un launcher de juego comercial no debe sentirse pesado al
   arrancar. Tauri no empaqueta un runtime de navegador completo; Electron sí.
3. **Libertad de licencia.** MIT/Apache-2.0 sin condiciones restrictivas, igual que Avalonia y
   WinUI/WPF en su parte de código; a diferencia de WowUp (GPL-3.0, descartado como dependencia de
   código por este motivo en `docs/investigacion/gestores-de-addons.md`).
4. **UX visual.** HTML/CSS da más libertad para construir la identidad visual propia que pide el
   encargo (animaciones, transiciones, jerarquía visual) que los controles XAML nativos.
5. **Coste aceptado:** exige escribir el núcleo de actualización/integridad en Rust (o con bindings
   desde Rust), y depende de WebView2 estar presente en el equipo del jugador. Ambos son riesgos
   gestionables y no bloqueantes.

**El motor de actualización/integridad (manifest, hashes, staging atómico, validación) es un
componente propio, independiente del framework de UI elegido.** Ningún stack lo resuelve
(`docs/investigacion/autoactualizacion-y-manifest.md`): ni el updater de Tauri, ni Velopack, ni
Squirrel cubren el estado de los MPQ, el build del cliente o los addons obligatorios. Se diseñará
con metadatos firmados, hash SHA-256 por archivo, operación por staging+journal y fallback a
descarga completa, inspirado en los patrones de TUF y SteamPipe sin reclamar cumplir su
especificación formal salvo que se implemente de verdad.

## Pendiente de validar en la práctica

- Presencia real de WebView2 en los equipos objetivo de los jugadores (no solo en la documentación
  de Tauri).
- Tamaño final del instalador y arranque con una pantalla real, no solo con el "Hola mundo" de
  Tauri.

Si cualquiera de estos resulta problemático, se reevalúa esta decisión antes de avanzar mucho en la
implementación — no después.
