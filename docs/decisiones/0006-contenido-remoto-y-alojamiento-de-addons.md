# 0006 — Contenido remoto (noticias, portada, características) y alojamiento de addons

**Fecha:** 2026-10-09. **Estado:** propuesta, pendiente de las decisiones de la sección final.

## Contexto

Hoy las noticias, las características y los enlaces de la portada están embebidos en la aplicación
(`src/content/content.sample.json`): cambiarlos exige compilar y publicar una versión. El catálogo
de addons opcionales (decisión 0005) existe y está firmado, pero está vacío y no hay criterio de
dónde alojar los `.tar` ni de cómo curarlo. Queremos que el equipo de WarCrafted pueda actualizar
portada, noticias y addons sin tocar el launcher, y que lo exclusivo del servidor se vea.

Datos que condicionan el diseño (consultados el 2026-10-09):
- Un asset de GitHub Releases admite hasta 2 GiB y GitHub no es un CDN (ya recogido en el AGENTS.md).
  Los addons y las imágenes de noticias son pequeños (KB a pocos MB): GitHub Releases basta.
- El launcher solo descarga de los hosts de `ALLOWED_SOURCE_HOSTS` (`raw.githubusercontent.com` y
  `github.com`); cambiarlos exige publicar una versión del launcher.
- El manifest de contenido se firma con una clave que puede distribuir ejecutables (parches MPQ y
  addons obligatorios). Las noticias no ejecutan nada.

## Decisión

### 1. Clave de firma separada para el contenido editorial

Noticias, portada, características y enlaces se publican en **un documento firmado propio**,
`contenido.json`, con una **segunda clave Ed25519** (`warcrafted-content-2026`), distinta de la del
manifest. Si esa clave se filtrara, un atacante solo podría falsear noticias (el launcher abre
únicamente enlaces `https`), no distribuir archivos al cliente. La clave del manifest sigue
exclusivamente para el manifest y el catálogo de addons.

### 2. Formato editorial: Markdown con cabecera, compilado a JSON firmado

Se escribe en Markdown (un archivo por noticia) en `contenido/` del repositorio:

```
contenido/noticias/2026-10-12-festival-cosecha.md   cabecera + texto
contenido/portada.yml                               slides fijas (evergreen), orden y prioridad
contenido/caracteristicas.yml                       características del servidor
contenido/enlaces.yml                               enlaces oficiales
contenido/imagenes/                                 imágenes 16:9
```

Cabecera de una noticia: `id`, `title`, `summary`, `date`, `category` (`parche`, `evento`,
`mantenimiento`, `anuncio`), `image`, `link`, `featured`, `featuredUntil`, `expiresAt`. El cuerpo
admite solo lo que el launcher ya renderiza (encabezados, párrafos, listas, negrita): nada de HTML.

Un script (`scripts/publicar-contenido.sh`, mismo patrón que `publicar-parches.sh`) valida, compila
`contenido.json`, lo firma con la clave de contenido y lo sube como asset de una release fija
`contenido-remoto` (pre-release). La URL del launcher no cambia nunca. El autor edita Markdown y
ejecuta **un comando**. (Una Action que lo haga al hacer push es posible más adelante; exigiría
guardar la clave de contenido como secret.)

Seguridad: firma obligatoria; `contentVersion` creciente (anti-rollback); imágenes solo `https` de
hosts permitidos con `sha256` en el documento firmado (una imagen alterada se descarta); enlaces
solo `https`; textos siempre como texto, nunca HTML. Última versión válida en caché local: sin red o
con firma inválida el launcher muestra la caché y, si no hay, el contenido embebido de respaldo.

### 3. Frescura: portada que se mantiene sola

- El launcher consulta al arrancar y cada 6 h con `ETag`; no bloquea el arranque.
- Cada slide temporal lleva `featuredUntil`: **caduca sola** y desaparece del carrusel sin editar
  nada. Las slides **evergreen** (permanentes) rellenan hasta tener al menos 3.
- Máximo 5 slides, ordenadas por `priority` y luego por fecha. El script avisa si lleva más de
  30 días sin una slide nueva.

### 4. Carrusel de portada

Orden de prioridad de lo que se muestra (adaptable): 1) evento o novedad vigente; 2) lo exclusivo
del servidor (runas de clase y fases); 3) los bots (jugar sin esperar grupo); 4) cómo empezar y
la comunidad (Discord, web); 5) próximamente (siguiente fase). Cada slide: imagen 16:9 (1280×720,
WebP o JPG < 300 KB), título corto, una línea y un botón (abre una noticia o un enlace).

### 5. Características y lo exclusivo

`caracteristicas.yml` alimenta el bloque de características: icono, título, texto breve,
`exclusive: true` para mostrar la etiqueta «Solo en WarCrafted», e imagen o enlace opcional. Se
reservan para lo que otros servidores de WotLK no tienen. El contenido concreto lo aporta quien
lleva el servidor (ver preguntas); como candidatas, según el README del servidor del 2026-10-09:
sistema de runas de clase estilo Season of Discovery (grabado por ranuras), progreso por fases
(fase 1 hasta nivel 25), bots (Playerbots) y el parche de cliente propio. Hay que confirmar cuáles
están realmente jugables antes de prometerlas.

### 6. Alojamiento de los addons opcionales

- **Dónde:** assets de una release fija `addons` (pre-release) del repositorio del launcher, con
  nombres inmutables `<id>-<version>.tar` (una versión nueva es un archivo nuevo, nunca se
  sobrescribe). La integridad no depende del host: el `sha256` está en el catálogo firmado.
- **Cuándo migrar:** si el ancho de banda de GitHub se queda corto, `source.url` es por addon; basta
  cambiarlo y, si el host no está permitido, publicar una versión del launcher con el host nuevo.
  Se propone añadir ya `portal.warcrafted.com` a la lista de hosts permitidos en la próxima versión
  para tener salida sin forzar una actualización.
- **Qué entra (política):** licencia que permita redistribuir (MIT, BSD, zlib, GPL u otra con
  permiso expreso por escrito del autor), registrada con URL y fecha de consulta; funciona en 3.3.5a
  (`## Interface: 30300`); código revisado a mano (sin código ofuscado ni red inesperada); carpeta
  con su propio nombre sin colisión con los obligatorios. No se «rascan» CurseForge ni WoWInterface.
  Los addons propios (`~/Repos/addons`) salen de sus releases.

### 7. Organización del catálogo (esquema 2, compatible hacia atrás)

Campos nuevos opcionales por addon: `category` (`interfaz`, `combate`, `profesiones`, `misiones`,
`mapas`, `utilidades`, `bots`), `tags`, `recommended` (selección de WarCrafted, se muestra
primero), `screenshot`, `requires` (ids del propio catálogo, para librerías) y `conflicts`. La UI
muestra primero «Recomendados», luego por categoría, con buscador. El catálogo se mantiene
**pequeño y curado**: pocos addons buenos antes que muchos.

## Consecuencias

- Una segunda clave que custodiar (copia de seguridad fuera del Debian, igual que las otras).
- Cambiar portada o noticias deja de requerir versión del launcher; cambiar la lista de hosts o el
  formato sí.
- La primera versión necesita contenido real (texto, capturas) que solo puede aportar quien lleva el
  servidor; sin él, el sistema funciona con el contenido de respaldo.

## Decisiones pendientes del usuario

1. ¿De acuerdo con la clave de contenido separada y el flujo «Markdown + un comando»?
2. ¿Las noticias también irán a la web (`portal.warcrafted.com`)? Si sí, conviene que la web consuma
   el mismo `contenido.json` firmado en lugar de duplicar textos.
3. Lista real de características exclusivas y capturas o artes para el carrusel.
4. ¿Añadimos `portal.warcrafted.com` a los hosts permitidos?
5. Primeros addons opcionales a ofrecer.
