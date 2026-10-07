# Estado del launcher

Mantén este archivo al día con cada cambio relevante, no al final de la tarea. Es lo primero que
lee una sesión nueva sobre `launcher/`. Última actualización: **2026-10-07**.

## Dónde estamos

Investigación completa (`docs/investigacion/`, 5 informes) y decisión de stack tomada:
**Tauri 2**, ver `docs/decisiones/0001-stack-tecnologico.md`. Esqueleto Tauri 2 generado y
compilando (`cargo check` verificado en Linux); backend Rust reestructurado en `ui_commands`
(comandos de UI) y `update_engine/` (submódulos `manifest`, `integrity`, `staging`, todavía solo
con su responsabilidad documentada, sin lógica). Próximo paso: diseñar el formato del manifest y
empezar a implementar el núcleo de `update_engine`.

## Decisiones tomadas

- **0001 — Stack tecnológico: Tauri 2.** Motor de actualización/integridad como módulo Rust propio,
  independiente del framework de UI. Pendiente de validar en la práctica: presencia real de
  WebView2 en equipos objetivo, tamaño/arranque del instalador final.
- **0002 — Formato del manifest.** JSON firmado (Ed25519), por reino/canal/build de cliente,
  versión monotónica anti-rollback, roles `required`/`optional` explícitos nunca confundibles,
  origen de descarga separado del hash de integridad. Pendiente de validar: tamaño real de un
  manifest completo, proceso operativo de firma.

## Próximo paso

Implementar el núcleo de `update_engine` sobre el formato de manifest ya decidido (0002):
empezar por `manifest` (parseo + validación de firma/versión/rutas), seguir por `integrity`
(verificación de hashes) y `staging` (descarga con reintentos/reanudación, aplicación atómica),
antes de tocar UI real. Buen candidato para delegar en tareas pequeñas por submódulo.

## No hacer

- No añadir lógica de actualización/parcheo mezclada con comandos de UI: el motor va aparte desde
  el principio (decisión 0001).
- No mezclar código del launcher con el servidor C++/SoD del resto del repo.
- No delegar al ejecutor decisiones de arquitectura, solo implementación ya decidida.
