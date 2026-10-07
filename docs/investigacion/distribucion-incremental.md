# Distribución incremental de archivos

Consulta de fuentes: 2026-10-07. No se midieron los archivos reales de WarCrafted ni la demanda prevista; las recomendaciones son opciones para evaluar con esas medidas.

## Enfoques

| Técnica o canal | Cómo reduce la transferencia | Ventajas | Costes y límites |
|---|---|---|---|
| Descargar archivos completos por hash | Solo pide los archivos ausentes o cuyo hash no coincide. | Simple, reparable, fácil de auditar; bueno si hay pocos archivos pequeños o releases poco frecuentes. | Un cambio pequeño en un MPQ grande obliga a descargar el MPQ completo. |
| Delta binario por archivo (p. ej. bsdiff o Velopack/Zstandard) | Construye una transformación desde una versión base conocida a la nueva versión. | Puede ahorrar transferencia para ejecutables o archivos grandes con cambios pequeños. | Hay que distribuir el delta para versiones base compatibles; la creación/aplicación consume CPU y disco; fallos de base necesitan fallback completo. Siempre se valida hash del archivo final. |
| Troceado por bloques con deduplicación | Divide los archivos en chunks; cliente reutiliza bloques idénticos ya presentes o compartidos entre versiones. | Aprovecha cambios localizados en archivos grandes y da margen para reintentar por partes. SteamPipe describe bloques de alrededor de 1 MB, reuse entre builds y descarga de solo bloques nuevos. | Requiere índice por bloque, almacenamiento de objetos, GC y protocolo propio. Si el formato MPQ cambia internamente o se reordena mucho, puede producir deltas grandes. No equivale a un servicio SteamPipe reutilizable fuera de Steam. |
| Descarga por archivo con HTTP Range | Reanuda bytes faltantes de una representación estable, si el servidor y el cliente validan correctamente rangos/ETag. | Puede reanudar una descarga interrumpida grande sin dividir previamente el artefacto. | No reduce el total transferido; rangos no deben concatenarse si la entidad cambió. Debe validar tamaño, ETag/versión y hash final; no asumir que cualquier hosting conserva rangos. |

## Hosting: GitHub Releases frente a object storage/CDN

| Criterio | GitHub Releases | Almacenamiento de objetos + CDN |
|---|---|---|
| Uso natural | Releases versionadas para instaladores, herramientas y archivos publicados junto al código. | Distribución frecuente o masiva de payloads de juego, fragmentos y manifests públicos. |
| Límites publicados | Cada archivo de Release debe tener menos de 2 GiB; se permiten hasta 1000 assets por release. GitHub documenta que no limita tamaño total de release ni ancho de banda en esa página. | Depende del proveedor, la clase de almacenamiento, tamaño máximo, egress, requests, región y política de caché contratados. No se fija un límite genérico sin elegir proveedor. |
| API | REST API pública: 60 solicitudes/hora por IP sin autenticar; 5000/hora por usuario autenticado. Las rutas de búsqueda y límites secundarios pueden ser más restrictivos. El cliente no debe llevar un token PAT/secret en el binario. | El cliente puede consultar un objeto JSON estático directamente; evitar credenciales de escritura en clientes. API de control interna puede publicar metadatos al bucket. |
| Descarga y caché | Releases ofrecen URL de descarga y API de assets con tamaño, digest (cuando disponible) y conteo. La documentación consultada no da una garantía de CDN/SLA ni una cifra universal de throughput para este uso. | CDN distribuye objetos desde ubicaciones cercanas y usa cache-control/ETag; deben definirse invalidación, vida de caché y control de versión. Puede haber costes de salida y configuración operativa. |
| Publicación | Práctico para instaladores y manifests de tamaño pequeño/medio; assets son inmutables por versión en un proceso limpio de releases. | Mejor control de nombres content-addressed, Range, ciclo de vida, mirrors, métricas y políticas de cache. Hay que montar y vigilar la infraestructura. |

**Lectura para el producto:** GitHub Releases parece suficiente para el instalador, firmas y una pequeña metadata inicial; no se debe asumir que las descargas del cliente serán ilimitadas, con SLA o optimizadas para distribución global. La documentación consultada confirma que no hay límite de ancho de banda para Releases, pero eso no es una promesa de rendimiento, disponibilidad ni coste total. [Inferencia basada en el alcance de los límites documentados.] Evitar consultar la API por cada archivo: publicar un manifest estático versionado que apunte a objetos. Si la escala o el peso lo exige, mover el contenido grande a almacenamiento/CDN sin cambiar las identidades criptográficas del manifest.

## Notas prácticas para MPQ y addons

- Empezar por verificación de hashes del archivo completo y descarga selectiva. Medir el cambio medio de los parches entre versiones antes de implementar deltas.
- Distribuir primero paquete completo firmado de cada archivo obligatorio. Añadir deltas opcionales solo con hash de origen, hash de salida, presupuesto de espacio temporal y fallback completo.
- Para un cliente base limpio, separar descarga inicial de reparación incremental. Si WarCrafted no aloja el cliente original, el manifest solo debe cubrir los archivos que WarCrafted puede distribuir legalmente; el usuario aporta los demás.
- Mantener nombres content-addressed para cachear y compartir blobs sin confundir versiones. Un manifiesto firmado relaciona el hash final con ruta y función del archivo.
- Evitar descargar ejecutables o scripts remotos para invocarlos directamente. Parches MPQ y ZIP son contenido no confiable hasta validar integridad y rutas; los archivos Lua de addons sí son código que el cliente WoW ejecuta.
- La operación de update debe poder reanudar bytes, detectar respuesta parcial, descartar temporal corrupto y no borrar la versión activa hasta completar y verificar la nueva.

## Límites de GitHub verificados

La página oficial de Releases especifica hasta 1000 assets por release y menos de 2 GiB por archivo; dice que no hay límite total de tamaño de una release ni de ancho de banda. La API REST marca 60 requests/hora para requests no autenticadas (asociadas a IP) y 5000/hora para usuarios autenticados ordinarios. Hay límites adicionales secundarios y algunos endpoints más restrictivos. Es incorrecto convertir esas cifras en garantía de tasa de transferencia de archivos.

## Incertidumbres

- No se ha comprobado una garantía contractual de CDN, SLA, cuotas de descarga, disponibilidad o soporte para GitHub Releases como repositorio de parches. Las páginas oficiales citadas no especifican esas garantías. Confirmar condiciones vigentes antes de una distribución pública.
- No se ha elegido proveedor de object storage/CDN; precio, cuotas, tamaño máximo, regiones y reglas de reanudación dependen del proveedor elegido y pueden cambiar.
- Falta inventario de tamaños MPQ/addons, frecuencia de cambios, tasa de usuarios, países, concurrencia y presupuesto. Sin ello no es posible decidir delta por archivo frente a chunking ni estimar costes o tamaño de instalación.
- Los límites oficiales pueden cambiar; volver a consultarlos antes de definir assets de release o construir clientes.

## Fuentes

- [SteamPipe: sistema de contenido, bloques, deltas y HTTP](https://partner.steamgames.com/doc/sdk/uploading), consultado 2026-10-07.
- [Velopack: deltas y límite de archivo](https://docs.velopack.io/packaging/deltas), consultado 2026-10-07.
- [GitHub Docs: límites de archivos de Releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases), consultado 2026-10-07.
- [GitHub REST API: rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api), consultado 2026-10-07.
- [GitHub REST API: assets de releases y digest expuesto](https://docs.github.com/en/rest/releases/assets), consultado 2026-10-07.
