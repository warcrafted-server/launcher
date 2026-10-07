# Estado del launcher

Mantén este archivo al día con cada cambio relevante, no al final de la tarea. Es lo primero que
lee una sesión nueva sobre `launcher/`. Última actualización: **2026-10-07**.

## Dónde estamos

Investigación completa (`docs/investigacion/`, 5 informes) y decisión de stack tomada:
**Tauri 2**, ver `docs/decisiones/0001-stack-tecnologico.md`. Esqueleto Tauri 2 generado y
compilando. `update_engine::manifest` implementado y aceptado (tipos, verificación de firma
Ed25519, anti-rollback, normalización de rutas, tests) — ver commit `f153125`. `integrity` y
`staging` siguen vacíos.

Se investigaron y descartaron como origen de descarga TeraBox, Hugging Face Hub (Datasets) y
DigiStorage (sin API estable, sin garantía contractual, o atados a una cuenta personal). El
cliente completo ronda 20 GB (cifra del usuario, sin medir con precisión), por encima del límite
de 2 GiB/archivo de GitHub Releases: se decidió fragmentarlo (decisión 0003) en vez de cambiar de
infraestructura de distribución.

## Decisiones tomadas

- **0001 — Stack tecnológico: Tauri 2.** Motor de actualización/integridad como módulo Rust propio,
  independiente del framework de UI. Pendiente de validar en la práctica: presencia real de
  WebView2 en equipos objetivo, tamaño/arranque del instalador final.
- **0002 — Formato del manifest.** JSON firmado (Ed25519), por reino/canal/build de cliente,
  versión monotónica anti-rollback, roles `required`/`optional` explícitos nunca confundibles,
  origen de descarga separado del hash de integridad. Pendiente de validar: tamaño real de un
  manifest completo, proceso operativo de firma.
- **0003 — Fragmentación de archivos grandes.** Un archivo del manifest puede declararse ensamblado
  a partir de fragmentos (`assembly.parts[]`, cada uno con su propio hash/origen) para sortear el
  límite de 2 GiB de GitHub Releases sin cambiar de infraestructura de distribución. Pendiente de
  validar: tamaño real del cliente, tamaño óptimo de fragmento.

## Próximo paso

Implementar `update_engine::integrity` (verificación de hashes contra el filesystem real) y
`update_engine::staging` (descarga con reintentos/reanudación, ensamblado de fragmentos según
decisión 0003, aplicación atómica), sobre el manifest ya implementado. Buen candidato para
delegar en tareas pequeñas por submódulo.

## No hacer

- No añadir lógica de actualización/parcheo mezclada con comandos de UI: el motor va aparte desde
  el principio (decisión 0001).
- No mezclar código del launcher con el servidor C++/SoD del resto del repo.
- No delegar al ejecutor decisiones de arquitectura, solo implementación ya decidida.
