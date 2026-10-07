# Encargo original del launcher

Texto literal recibido del usuario el 2026-10-07, fuente de la que derivan las instrucciones de
`launcher/AGENTS.md` y todas las decisiones posteriores. No editar: si el alcance cambia, se
documenta como una nueva decisión en `docs/decisiones/`, no modificando este archivo.

---

Quiero que desarrolles dentro del repositorio existente:

https://github.com/warcrafted-server/WotLK-SoD

un launcher oficial para el ecosistema **WarCrafted**.

Antes de empezar a implementar, quiero que estudies cuidadosamente el repositorio existente, su
documentación, estructura, convenciones y cualquier instrucción para agentes que exista en el
proyecto.

Tu papel es el de **orquestador, arquitecto y responsable técnico del proyecto**. Dispones de un
ejecutor/agente que realizará el trabajo de implementación. Tú debes investigar, analizar,
planificar, delegar, revisar los resultados y corregir o hacer replantear aquello que no cumpla
los objetivos. No delegues ciegamente: valida siempre el resultado.

No quiero imponerte una arquitectura concreta ni una tecnología concreta. Considera las
alternativas razonables y elige la que consideres técnicamente más adecuada para conseguir un
producto profesional, mantenible, seguro y preparado para evolucionar.

### Objetivo

Crear un launcher de escritorio para el proyecto WarCrafted, inicialmente orientado a nuestro
reino WotLK 3.3.5, pero diseñado desde el principio para poder soportar varios reinos en el
futuro.

El cliente objetivo inicialmente es World of Warcraft 3.3.5 build 12340.

Nuestro servidor está basado en AzerothCore y utiliza modificaciones propias del cliente,
incluyendo patches MPQ y determinados addons obligatorios.

El launcher debe conseguir que un jugador pueda instalar el launcher, preparar el cliente y
mantenerlo siempre compatible con la versión que requiere nuestro servidor, sin tener que
realizar manualmente las actualizaciones de patches o addons obligatorios.

### Funciones principales

El launcher deberá contemplar, como mínimo:

* Detección y configuración del cliente WoW.
* Comprobación de que el cliente corresponde a la versión/build requerida.
* Comprobación de todos los archivos obligatorios del cliente.
* Sistema fiable de detección de archivos inexistentes, modificados o corruptos.
* Actualización automática de patches obligatorios.
* Actualización automática de addons obligatorios.
* Bloqueo de la posibilidad de jugar mientras el cliente no esté en un estado compatible.
* Descarga solamente de los archivos que sean necesarios.
* Verificación de integridad de los archivos descargados.
* Posibilidad de reparar archivos dañados.
* Opción de borrar caché
* Comprobación automática del realmlist y actualización a logon.warcrafted.com si es necesario
  que no coincida
* Actualización del propio launcher.
* Progreso de descargas claro para el usuario.
* Manejo razonable de errores de red y descargas interrumpidas.
* Preparación inicial de un cliente que todavía no tenga los archivos necesarios.
* Descarga del cliente si se detecta que no existe ninguno en el equipo. Más tarde veo donde alojo
  el cliente para descargar
* Configuración de la ruta del cliente.
* Lanzamiento del juego una vez que el cliente esté validado.
* Preparación arquitectónica para múltiples reinos aunque inicialmente solamente exista uno.

### Noticias

El launcher debe disponer de una sección visual de noticias del servidor.

Las noticias deben poder incluir como mínimo:

* Imagen.
* Título.
* Texto/resumen.
* Fecha.
* Enlace para ampliar información.
* Puede haber más de una noticia, ya vemos como lo hacemos porque igual ponemos como un índice
  también

El contenido de las noticias debe poder actualizarse remotamente sin tener que recompilar el
launcher.

La solución debe permitir que posteriormente podamos disponer de varias noticias, destacar una
noticia principal y navegar entre ellas.

### Enlaces

Debe existir una sección claramente accesible con enlaces a los servicios oficiales actuales de
WarCrafted:

https://wotlk.warcrafted.com/

https://wotlk.warcrafted.com/forum/

https://db.warcrafted.com/

La arquitectura debe permitir añadir más servicios posteriormente.

### Addons opcionales

Debe existir una sección independiente para addons opcionales.

Hay que distinguir claramente entre:

* archivos necesarios para poder jugar;
* addons opcionales que el jugador puede instalar voluntariamente.

Los addons obligatorios deben estar bajo el control del sistema de actualización y no deben poder
considerarse opcionales.

Los addons opcionales deben poder descubrirse, descargarse, instalarse y, si procede, actualizarse
desde el launcher.

### Cliente completo

Quiero que estudies la posibilidad de que la instalación del launcher permita preparar también el
cliente completo desde cero.

### Diseño y experiencia de usuario

No quiero un launcher genérico, barato, improvisado o con apariencia de herramienta técnica.

Quiero un producto que visualmente transmita que pertenece a un juego comercial de calidad.

Debe tener:

* identidad visual propia de WarCrafted;
* interfaz moderna;
* buena jerarquía visual;
* navegación clara;
* animaciones y transiciones cuando aporten valor;
* estados de descarga y actualización bien diseñados;
* excelente tratamiento de errores;
* soporte para diferentes resoluciones y escalado de Windows;
* aspecto cuidado tanto en estado normal como durante actualizaciones;
* una pantalla principal atractiva;
* clara separación entre jugar, noticias, addons, configuración y otros contenidos.

Debe inspirarse en la calidad y experiencia de los launchers profesionales de videojuegos, pero
**no debe copiar el diseño, branding, artwork ni identidad visual de Blizzard**.

La identidad visual debe poder evolucionar independientemente de la lógica del launcher.

### Investigación previa

Antes de tomar decisiones importantes, estudia proyectos existentes relacionados con:

* launchers de AzerothCore;
* launchers de WoW 3.3.5;
* sistemas de autoactualización;
* sistemas de manifest;
* distribución incremental de archivos;
* gestores de addons;
* launchers profesionales de videojuegos;
* tecnologías de UI de escritorio apropiadas para este producto.

Puedes estudiar proyectos públicos para aprender de sus decisiones, arquitectura, problemas y
soluciones.

Puedes estudiar incluso proyectos que tengan una licencia que no podamos utilizar.

Delega la búsqueda y estudio de proyectos a tu ejecutor, la finalidad es ahorrar tokens de claude

**Pero no copies código, assets, diseños propietarios ni partes derivadas de proyectos cuya
licencia no permita su reutilización.**

Nuestro objetivo es crear una implementación propia y WarCrafted debe poder utilizar su propia
licencia.

Presta especial atención a las licencias de:

* framework;
* librerías;
* componentes UI;
* fuentes;
* iconos;
* assets;
* código reutilizado;
* dependencias transitivas cuando sea relevante.

Si una dependencia obliga a unas condiciones incompatibles con nuestra intención de tener libertad
para elegir la licencia del proyecto, considérala cuidadosamente antes de incorporarla.

### GitHub y distribución

Actualmente GitHub forma parte de nuestra infraestructura de distribución.

El launcher debe poder comprobar si existe una nueva versión del launcher y si existen nuevas
versiones de los archivos del cliente que debe distribuir WarCrafted.

### Integridad y seguridad

El sistema de actualización debe tratar los archivos descargados como contenido no confiable
hasta haberlos validado.

Quiero una solución sólida para:

* integridad de archivos;
* hashes criptográficos;
* descargas incompletas;
* archivos corruptos;
* sustitución segura;
* actualizaciones atómicas;
* recuperación ante errores;
* evitar dejar el cliente en un estado parcialmente actualizado;
* validación de URLs;
* protección frente a path traversal;
* evitar ejecución arbitraria de contenido descargado;
* actualización segura del propio launcher.

No sacrifiques seguridad por simplicidad.

### Evolución futura

Aunque inicialmente solamente tendremos un reino, quiero que la arquitectura permita
posteriormente:

* varios reinos;
* diferentes configuraciones por reino;
* diferentes versiones de cliente;
* diferentes conjuntos de archivos obligatorios;
* diferentes addons obligatorios;
* diferentes noticias;
* diferentes servidores;
* posibles futuras versiones de WoW.

No quiero implementar ahora funcionalidades innecesarias solamente por anticiparnos al futuro,
pero sí quiero evitar decisiones que hagan que añadirlas posteriormente obligue a rehacer todo el
proyecto.

### Calidad del producto

Este no debe ser un prototipo visual.

Quiero un producto real, mantenible y preparado para producción.

Debe contar con:

* arquitectura clara;
* separación adecuada de responsabilidades;
* tests donde aporten valor;
* manejo de errores;
* logging apropiado;
* documentación;
* proceso reproducible de build;
* generación del instalador;
* configuración de desarrollo;
* configuración de producción;
* estrategia clara de releases;
* versionado;
* CI/CD cuando resulte apropiado.

No quiero sobreingeniería gratuita.

Si consideras que alguna funcionalidad que he solicitado debe resolverse de otra manera,
explícamelo y propón una alternativa mejor.

### Muy importante

No quiero que simplemente ejecutes mi lista de requisitos literalmente.

Quiero que actúes como responsable técnico del producto.

Si durante tu investigación encuentras:

* una arquitectura mejor;
* una funcionalidad importante que falte;
* un problema de seguridad;
* una limitación de GitHub;
* una tecnología más apropiada;
* una mala decisión de diseño;
* una forma de conseguir una UX significativamente mejor;

debes indicármelo y proponer la alternativa.

Antes de implementar decisiones arquitectónicas relevantes, razona sobre ellas y justifica la
elección.

No copies soluciones de otros proyectos: aprende de ellas.

### Método de trabajo

Tú eres el orquestador.

Dispones de un ejecutor para realizar el trabajo de implementación.

Planifica el trabajo, divide las tareas de forma razonable, delega cuando corresponda y revisa
críticamente cada resultado.

El ejecutor no debe convertirse en quien decide la arquitectura por ti.

Después de cada bloque importante de trabajo, verifica que lo implementado realmente cumple el
objetivo y que no introduce deuda técnica o desviaciones arquitectónicas.

No des por bueno un resultado simplemente porque compile.

Comprueba funcionalidad, integración, seguridad, UX y mantenibilidad según corresponda.

### Primera fase

Antes de comenzar una implementación importante:

1. Analiza el repositorio actual.
2. Analiza sus instrucciones y documentación existentes.
3. Investiga los proyectos externos relevantes.
4. Evalúa las alternativas tecnológicas y arquitectónicas.
5. Identifica riesgos y decisiones importantes.
6. Propón una arquitectura inicial.
7. Identifica las decisiones que consideres suficientemente claras para implementar y aquellas
   sobre las que necesites mi opinión.
8. Después, comienza el desarrollo de forma ordenada.

No quiero que inventes información sobre el repositorio ni sobre tecnologías que no hayas
comprobado.

Cuando una afirmación sea incierta, compruébala.

Cuando tengas que tomar una decisión que pueda afectar significativamente al producto, quiero
conocer el razonamiento y las alternativas consideradas.

El objetivo final es construir el **WarCrafted Launcher**, un producto propio, profesional y
preparado para convertirse en el punto de entrada de los jugadores al ecosistema WarCrafted.
