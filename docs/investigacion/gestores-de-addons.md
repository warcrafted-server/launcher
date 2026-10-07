# Gestores de addons de WoW

Consulta de fuentes: 2026-10-07. Los catálogos y permisos de distribución de terceros cambian con frecuencia; no se asume que una API permita incorporar sus addons a WarCrafted.

## Comparativa observada

| Producto/fuente | Descubrimiento e instalación | Actualización e integridad observada | Licencia, actividad y límites relevantes |
|---|---|---|---|
| CurseForge App | Selecciona una instancia de WoW detectada o añadida manualmente; filtra por juego/sabor/categoría y busca addon por nombre; instala en la instancia activa. Admite gestión por flavor, lista de instalados y sincronización. | Puede buscar y aplicar actualización individual o «Update All», automática por frecuencia/configuración. Advierte si archivos locales cambiaron o si la versión corresponde a otro flavor; en estado «Modified» evita sobrescribir automáticamente. | Aplicación propietaria de Overwolf; el catálogo son proyectos subidos/gestionados por autores y algunos addons no están en él. Su API oficial requiere solicitar acceso de terceros; la documentación también incluye el campo `allowModDistribution`. No asumir acceso anónimo o permiso universal. |
| WowUp | App comunitaria con proveedores de catálogos; encuentra instalaciones, permite búsqueda y descubrimiento cruzado entre fuentes. Su guía dice que GitHub requiere una release etiquetada con ZIP que la app pueda localizar. | Presenta versiones disponibles y actualiza addons; el cliente avisa de incompatibilidades o paquetes no reconocidos. La API/fuente puede variar por proveedor. | Repositorio del cliente GPL-3.0, por tanto no se integra código si WarCrafted quiere conservar libertad de licencia sin cumplir copyleft. La página del repositorio mostraba actividad acumulada (3.921 commits), pero no pude verificar la fecha del último commit en esta consulta; consultado 2026-10-07. No reutilizar branding. |
| Wago.io / aplicación Wago | Plataforma y proveedor de addons, perfiles/UI packs; WowUp incluye el dominio Wago entre fuentes compatibles según su changelog. La aplicación Wago puede abrir e instalar UI Packs. | La disponibilidad de versión/actualización se basa en metadatos del proveedor y releases del autor. Verificar permiso/contrato y mecanismo hash por API antes de integrarlo. | Servicio y términos propios; la licencia del programa no da derecho a redistribuir todos los addons alojados. No se confirma desde la documentación consultada que todos los proyectos incluyan hashes firmados o compatibilidad 3.3.5. |
| GitHub Releases (como fuente de un addon) | WowUp documenta instalación desde URL/release: autor crea tag y publica ZIP empaquetado. Puede ser una fuente sencilla para addons propios o de autores que la elijan. | Un asset por release puede acompañarse de hash y manifest del producto. GitHub no ofrece por sí solo semántica de addon ni compatibilidad de interfaz. | Licencia, permiso de redistribución y formato los fija cada autor. Que un repositorio sea público no equivale a licencia ni permiso para republicar. |

Las afirmaciones de actividad se limitan al estado visible durante la consulta. CurseForge y Wago son servicios comerciales/hosted; no hay una única licencia de código que abarque los addons de terceros.

## Qué sí es aplicable a WarCrafted

- Detectar carpetas AddOns por instancia del cliente y leer los metadatos de `.toc` para mostrar nombre, interfaz objetivo y versión declarada. Un nombre de carpeta no es un identificador estable: incluir ID de proyecto/fuente y hashes de archivos.
- Separar fuente/catálogo, resolver versiones compatibles con el cliente 3.3.5 y aplicar a una ruta de cliente escogida. Una ficha debe declarar fuente, autor, versión, build/interface objetivo, archivos instalados, fecha, hash y URL.
- Conservar una copia/estado previo antes de reemplazar addon. Si hay archivos locales modificados, informar y ofrecer backup/decisión, siguiendo la idea protectora que documenta CurseForge para estados modificados.
- No inferir que un addon es opcional por su nombre o por estar en la carpeta Interface/AddOns. Los addons requeridos de WarCrafted tienen lista de control propia y no se desactivan desde la UI de opcionales.
- Mantener historial de instalación, reparar diferencias por hash y quitar solo archivos cuya propiedad conoce el manifest del launcher. Evitar borrar carpetas completas de usuario o addons instalados por otro gestor.
- En catálogo de opcionales, ofrecer búsqueda/lista curada primero; el acceso de descubrimiento amplio agrega dependencia de APIs comerciales, claves, términos de redistribución y mantenimiento de proveedores.

## Precauciones específicas

1. **Autenticidad:** la descarga debe estar ligada a una versión y hashes desde un manifest autenticado por WarCrafted o por la fuente autorizada. TLS y checksum sin firma ayudan con corrupción, no prueban quién decidió la versión.
2. **Compatibilidad:** no equiparar Retail, Classic actual, Classic Era o Cataclysm con WotLK 3.3.5a. El campo Interface de `.toc` es pista y no una certificación: el addon puede usar API no disponible o ser una bifurcación privada.
3. **Ejecución:** Lua de addon se ejecuta dentro del cliente. Opcional no significa inocuo. Mostrar procedencia, licencia individual, mantenedor y versión antes de instalar; no ejecutar scripts descargados fuera del juego.
4. **Archivo ZIP:** normalizar cada ruta, rechazar absolute paths, `..`, symlinks, entradas duplicadas, exceso de tamaño/expansión y destinos fuera de AddOns. Extraer a staging, verificar y luego colocar.
5. **Derechos:** respetar licencias declaradas por proyecto. No copiar los archivos desde CurseForge/Wago o cachearlos en distribución WarCrafted sin permiso y acceso de API en regla.
6. **Separación:** la lista obligatoria sirve al validador del cliente y gobierna el bloqueo de juego; opcionales se administran en un subsistema con consentimiento expreso y sin autoridad para volver válido un cliente con obligatorios faltantes.

## Incertidumbres

- CurseForge para Studios presenta una API para servicios de terceros y un flujo para solicitar clave, pero el informe no verificó aprobación para WarCrafted, alcance exacto de esa clave ni términos de almacenamiento/distribución aplicables a addons de WoW. Consultar legalmente antes de integrar.
- No se verificó que CurseForge, WowUp o Wago detecten y validen addons contra el cliente privado exacto 3.3.5a build 12340. Sus etiquetas de flavor no bastan para concluir compatibilidad.
- No se evaluaron todos los proveedores, API, límites, costes ni políticas antifraude. La actividad de los proyectos, requisitos de API y disponibilidad de paquetes puede cambiar.
- La fuente pública consultada no certifica hashes firmados en todas las descargas de estos gestores. No presentar su indicador «corrupto» como una garantía criptográfica equivalente a un manifest firmado.

## Fuentes

- [CurseForge: FAQ de addons de WoW, instalación, sync y avisos de archivos](https://support.curseforge.com/support/solutions/articles/9000198422-addons-faq), consultado 2026-10-07.
- [CurseForge: empezar, descubrir y configurar actualizaciones](https://support.curseforge.com/support/solutions/articles/9000193488-getting-started), consultado 2026-10-07.
- [CurseForge for Studios: API REST y acceso de terceros](https://docs.curseforge.com/rest-api/), consultado 2026-10-07.
- [WowUp: repositorio, proveedores y licencia](https://github.com/WowUp/WowUp), consultado 2026-10-07; [guía de instalación desde GitHub](https://wowup.io/guide/get-addons/overview), consultado 2026-10-07.
- [WowUp: guía general de descubrimiento](https://wowup.io/guide/get-addons/overview), consultado 2026-10-07; [sitio oficial de WowUp](https://wowup.io/), consultado 2026-10-07.
- [Wago: sitio de addons](https://addons.wago.io/), consultado 2026-10-07.
