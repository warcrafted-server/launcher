# Autoactualización y manifests de archivos

Consulta de fuentes: 2026-10-07. Este informe describe patrones observables y controles recomendables; no fija el formato definitivo del manifest de WarCrafted.

## Qué debe resolver un manifest

Un manifest es un índice de estado deseado: identifica los archivos que componen una release y la evidencia usada para aceptar cada descarga. Para reparar el cliente, tiene que compararse con el contenido local y verificar lo que realmente se instaló, no limitarse a confiar en el nombre, la extensión o la fecha de modificación.

Campos que conviene evaluar en un formato propio:

| Campo | Propósito |
|---|---|
| Identificador de reino/canal y build de cliente | Evita combinar archivos de perfiles o clientes incompatibles. |
| Versión monotónica y fecha de publicación | Permite detectar versiones nuevas y rechazar downgrades no autorizados. Una fecha sola no evita rollback. |
| Ruta relativa normalizada | Localiza el destino sin permitir rutas absolutas, segmentos .., unidades, UNC o escapes de enlace simbólico. |
| Tamaño y SHA-256 o SHA-512 del archivo final | Comprueba integridad después de descargar y después de aplicar deltas. El hash debe estar cubierto por una firma autenticada. |
| URL o identificador de objeto, tamaño comprimido y algoritmo | Permite alojar el contenido aparte del índice y establecer límites de recursos. Solo aceptar orígenes y esquemas previstos. |
| Rol: obligatorio u opcional, grupo y versión de addon | Impide que una opción del usuario desactive contenido requerido. |
| Requisitos y hash/base del delta | Evita aplicar un delta sobre una versión incorrecta; conservar descarga completa como alternativa. |
| Firma, versión de metadatos, expiración y clave identificada | Autentica la lista, hace visibles metadatos caducados y habilita rotación de claves. |

Un checksum publicado junto al archivo permite detectar corrupción accidental si ambos se obtienen por un canal confiable, pero no autentica al editor cuando el atacante puede sustituir ambos. HTTPS protege el canal y reduce manipulaciones en tránsito; la firma digital del manifest protege la autoridad del contenido incluso si el almacenamiento/CDN se compromete.

## Patrones de la industria

| Sistema | Metadatos y verificación | Atomicidad, recuperación y límite |
|---|---|---|
| [The Update Framework (TUF)](https://theupdateframework.io/docs/metadata/) | Metadatos firmados por roles separados. Timestamp enlaza hash/tamaño del snapshot; snapshot enlaza una vista consistente; targets describe archivos. Metadatos expiran y se versionan. El cliente puede detectar rollback, freeze y mezcla de vistas. | TUF define confianza y selección/validación de targets, no la sustitución transaccional de todo el árbol de archivos. La aplicación debe implementar staging, journal y rollback local. La separación de roles permite mantener claves de targets/root fuera de línea y una clave timestamp rotada con frecuencia. |
| [SteamPipe](https://partner.steamgames.com/doc/sdk/uploading) | Divide archivos en bloques de aproximadamente 1 MB; reutiliza bloques iguales de la versión anterior, y comprime/cifra los bloques nuevos. Los manifiestos identifican depots/versiones. Usa HTTP y puede aprovechar cachés/CDN. | Mantiene disponible la versión anterior mientras prepara la nueva y evita copiar bloques sin cambios en el cliente. Es un sistema de plataforma de Valve, no una librería independiente para integrar directamente en un launcher propio. El empaquetado de recursos grandes puede reducir la eficiencia si los cambios desplazan grandes regiones del archivo. |
| [Velopack](https://docs.velopack.io/packaging/deltas) | Construye deltas binarios con Zstandard por archivo. Puede elegir deltas secuenciales o paquete completo según tamaño/heurísticas; limita cada archivo a menos de 2 GB para sus deltas. | Reconstruye el paquete final antes de aplicarlo y, si no hay delta o falla la reconstrucción, permite descargar el paquete completo. El gestor usa un bloqueo global por aplicación para una sola operación de actualización concurrente. La documentación no sustituye un diseño transaccional para el árbol WoW. Licencia MIT según su repositorio. |
| [Sparkle](https://sparkle-project.org/documentation/) (macOS) | EdDSA/Ed25519 autentica los archivos de actualización; firma Apple puede complementar. Appcast publica versiones, tamaños, firmas y URLs. `generate_appcast` genera feed y deltas. | Los deltas se descartan si el estado base/hash no coincide y se intenta el archivo completo. Su proyecto declara instalaciones seguras y atómicas de bundles macOS. Solo aplica a apps macOS, no al launcher Windows objetivo. Licencia MIT. |
| [Squirrel.Windows](https://github.com/Squirrel/Squirrel.Windows) | Paquetes NuGet, archivo RELEASES con SHA-1 para comparación/validación de paquetes y paquetes delta. | Instala cada versión en su propio directorio local y actualiza accesos directos. El proceso documentado conserva la versión anterior durante la limpieza, pero declara explícitamente que no incluye rollback integrado. El proyecto pide mantenedores; valorar esa señal antes de elegirlo. Revisar COPYING y las licencias de dependencias en cualquier evaluación legal. |
| [Tauri Updater](https://v2.tauri.app/plugin/updater/) | Requiere firma del artefacto con clave privada/publica; el cliente verifica antes de instalar. Admite feed estático JSON o servidor dinámico y reporta eventos de descarga. | El plugin instala paquetes de aplicación y delega detalles al instalador/plataforma. No cubre el manifest de assets del juego. La clave privada embebida solo en CI/almacén protegido es crítica: si se pierde, no se firman actualizaciones aceptables por instalaciones existentes. |
| [Electron autoUpdater](https://www.electronjs.org/docs/latest/tutorial/updates) | Usa feeds/metadatos de Squirrel en Windows y Squirrel.Mac en macOS. | Es autoactualización del launcher, no un verificador de client files. Electron documenta que la app debe estar firmada para actualizaciones macOS. El empaquetado/servidor/feed y el flujo Windows deben evaluarse por separado. |

## Estrategia de instalación recuperable para los archivos del juego

Una secuencia prudente, como recomendación de diseño derivada de los patrones anteriores:

1. Descargar manifest y metadatos a una zona temporal; verificar firma, expiración, versión, canal, tamaño máximo y campos obligatorios antes de obedecer URLs o rutas.
2. Calcular plan comparando la versión local con la versión objetivo. Descargar a nombres temporales, con reintentos y reanudación solo si se valida el rango recibido y el hash final.
3. Verificar archivo completo antes de extraerlo o instalarlo. Rechazar rutas absolutas, `..`, ADS de NTFS, enlaces y entradas duplicadas/ambiguas; resolver rutas bajo el directorio raíz permitido. Aplicar límites de bytes, cantidad de entradas y expansión de archivos comprimidos.
4. Aplicar deltas solo si el archivo de origen coincide con el hash/versión de base; validar el hash del resultado. Si falla, borrar el staging del delta y descargar el paquete completo.
5. Preparar todos los archivos requeridos en staging fuera de uso por el juego. Conservar respaldos o un journal durable con el estado previo y las operaciones pendientes.
6. Cerrar el juego y confirmar que los destinos no estén en uso. Sustituir mediante renombrados en el mismo volumen cuando sea posible; registrar cada operación y revertir desde el journal si el proceso se interrumpe.
7. Volver a calcular hash de cada destino y solo entonces marcar el conjunto como válido. Si no se puede garantizar una transición en bloque del directorio completo, guardar releases por versión y conmutar un puntero/estado de versión en el último paso.

El renombrado de un único archivo puede ser atómico en ciertos sistemas de archivos, pero actualizar muchos archivos no equivale a una única operación atómica. En Windows, permisos, antivirus, procesos que mantienen archivos abiertos, espacio en disco y cruces de volumen afectan las operaciones. La alternativa de versiones en directorios separados evita tocar parcialmente la release activa, a costa de más espacio; la decisión debe medirse con el tamaño real del cliente y los parches.

## Actualización del propio launcher

Separar la actualización de la UI en un proceso/instalador externo reduce conflictos con archivos ejecutables abiertos. El proceso debe validar firma/hash antes de ejecutar el instalador; el instalador no debe aceptar como autoridad una URL cualquiera descargada del feed. Mantener una versión anterior utilizable y hacer health check tras reinicio permite recovery operacional, pero no todos los frameworks lo hacen automáticamente. TUF puede proporcionar el marco de metadatos seguros; Velopack, Sparkle, Tauri Updater y Electron autoUpdater resuelven partes distintas del ciclo de entrega y no son sustitutos entre sí.

## Recomendaciones sujetas a decisión

- Mantener una clave pública raíz separada del servidor que aloja parches. Proteger las claves de firma fuera de línea o en un servicio CI restringido; preparar rotación y recuperación antes del primer cliente publicado.
- SHA-256 sirve para integridad de contenido frente al hash firmado. Una firma digital verifica procedencia; la firma no reemplaza la verificación hash del payload.
- Firmar metadatos con campos cubiertos de forma canónica y limitar cada manifiesto por tamaño/cantidad. Para el alcance inicial podría estudiarse un diseño TUF reducido solo tras evaluación experta, pero no llamarlo TUF si no sigue su especificación.
- Producir manifest desde una fuente versionada, publicar la release inmutable y exponer una referencia de canal pequeña. Separar metadatos, control y objetos mejora cacheabilidad y migración de hosting.
- Registrar el resultado de reparación y permitir al usuario reintentar; nunca dejar habilitado Jugar por la mera existencia de un archivo o por haber descargado bytes.

## Incertidumbres

- Ninguna fuente aquí demuestra la atomicidad de un conjunto de MPQ/addons instalado sobre todas las variantes de NTFS, FAT/exFAT, discos externos y antivirus. El diseño requiere pruebas en Windows con fallos forzados.
- Los detalles de firmas y rollback varían por versión de los frameworks citados. Verificar versiones fijadas y su documentación al elegir implementación.
- No se midieron tamaños de manifests, deltas, coste de disco temporal ni tasa de cambios de los MPQ de WarCrafted. No hay base aún para elegir chunking, bsdiff/Zstd o tamaño de bloques.

## Fuentes

- [Roles y metadatos TUF](https://theupdateframework.io/docs/metadata/), consultado 2026-10-07; [especificación TUF](https://github.com/theupdateframework/specification/blob/master/tuf-spec.md), consultado 2026-10-07.
- [SteamPipe: sistema de contenido y carga](https://partner.steamgames.com/doc/sdk/uploading), consultado 2026-10-07.
- [Velopack: deltas](https://docs.velopack.io/packaging/deltas), [integración y fallback](https://docs.velopack.io/integrating/overview), [repositorio/licencia](https://github.com/velopack/velopack), consultados 2026-10-07.
- [Sparkle: configuración, firmas y publicación](https://sparkle-project.org/documentation/), [deltas](https://sparkle-project.github.io/documentation/delta-updates/), [repositorio/licencia](https://github.com/sparkle-project/Sparkle), consultados 2026-10-07.
- [Squirrel.Windows: proyecto](https://github.com/Squirrel/Squirrel.Windows), [proceso de actualización](https://github.com/Squirrel/Squirrel.Windows/blob/develop/docs/using/update-process.md), consultados 2026-10-07.
- [Tauri Updater v2](https://v2.tauri.app/plugin/updater/), consultado 2026-10-07.
- [Electron: actualizaciones](https://www.electronjs.org/docs/latest/tutorial/updates), [autoUpdater](https://www.electronjs.org/docs/latest/api/auto-updater), consultados 2026-10-07.
