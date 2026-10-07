# 0003 — Fragmentación de archivos grandes en el manifest

**Fecha:** 2026-10-07. **Estado:** decidida.

## Contexto

El cliente completo de WotLK 3.3.5 ronda los **20 GB** (cifra aproximada dada por el usuario, no
medida con precisión — pendiente de confirmar con el archivo real). Esto supera el límite de 2 GiB
por archivo de GitHub Releases (infraestructura de distribución ya fijada en la decisión 0001), y
descartamos como origen alternativo TeraBox, Hugging Face Hub Datasets y DigiStorage tras
investigar cada uno (sin API estable, sin garantía contractual, o atado a una cuenta personal —
ver conversación e informes de cada uno; no se generó documento aparte por ser descartes, no
decisiones).

Necesitamos que el formato de manifest (decisión 0002) pueda describir un archivo final grande
como un conjunto de fragmentos descargables por separado, sin perder ninguna de sus garantías:
hash verificado por pieza, hash verificado del resultado ensamblado, y aplicación atómica.

## Decisión

Un archivo del manifest puede declararse **ensamblado a partir de fragmentos**. Se añade un campo
opcional `assembly` a una entrada de `files[]`:

```json
{
  "path": "Data/client-base.pkg",
  "role": "required",
  "kind": "clientBase",
  "sizeBytes": 21474836480,
  "sha256": "<hash del archivo final ya ensamblado>",
  "assembly": {
    "partSizeBytes": 2000000000,
    "parts": [
      { "sha256": "<hash del fragmento 0>", "sizeBytes": 2000000000, "source": { "url": "…parte-000", "compressedSizeBytes": 2000000000, "compression": "none" } },
      { "sha256": "<hash del fragmento 1>", "sizeBytes": 2000000000, "source": { "url": "…parte-001", "compressedSizeBytes": 2000000000, "compression": "none" } }
    ]
  }
}
```

- Si `assembly` está ausente, el archivo se descarga de una pieza con su propio `source` (como ya
  describía 0002) — ningún archivo existente cambia de comportamiento.
- Si `assembly` está presente, la entrada **no lleva `source` propio**: el origen vive en cada
  `parts[].source`, uno por fragmento, cada uno con su `sha256`/`sizeBytes` individuales.
- `partSizeBytes` es informativo (tamaño nominal de fragmento, para estimar progreso de descarga);
  el tamaño real de cada parte es el que indica su propio `sizeBytes`.

### Reglas de validación e integridad (añaden a las de 0002, no las sustituyen)

1. Cada fragmento se descarga y verifica su `sha256` individualmente, igual que cualquier archivo
   normal del manifest — el motor de integridad no necesita distinguir "fragmento" de "archivo" en
   esa fase.
2. Solo cuando **todos** los fragmentos de una entrada están descargados y verificados, el motor de
   staging los concatena en orden (`parts[0] + parts[1] + … `) en un archivo temporal.
3. Se verifica el `sha256` del archivo ensamblado contra el de la entrada (`files[].sha256`) antes
   de moverlo a su destino final. Si no coincide, se descarta el ensamblado y se repite desde el
   fragmento que falle (no hace falta volver a descargar los fragmentos ya verificados
   individualmente, solo rehacer la concatenación si el fallo está ahí, o redescargar el fragmento
   concreto si su verificación individual ya había fallado).
4. La sustitución del destino final sigue siendo atómica (renombrado en el mismo volumen), como ya
   describe la investigación de `docs/investigacion/autoactualizacion-y-manifest.md`.
5. Los fragmentos en staging se pueden borrar tras un ensamblado exitoso y verificado; no forman
   parte del estado final instalado.

### Qué no cambia

- El límite de 2 GiB de GitHub Releases sigue aplicando por fragmento, no por archivo lógico: cada
  `parts[].source.url` apunta a un asset normal de una Release, dentro de ese límite.
- La separación de `source` del hash de integridad (principio ya fijado en 0002) se mantiene igual
  a nivel de fragmento: migrar de host de origen no afecta a ningún hash, ni al del archivo final
  ni a los de sus partes.
- No se reintroduce la idea de deltas binarios (fuera de alcance, ver 0002): esto es solo partición
  de un archivo grande en trozos para sortear un límite de hosting, no una optimización de ancho de
  banda entre versiones.

## Pendiente de validar en la práctica

- Tamaño real del cliente (20 GB es una cifra aproximada del usuario, no medida).
- Tamaño de fragmento óptimo: 2 GiB es el límite duro de GitHub, pero un tamaño menor (p. ej. 1 GiB)
  deja más margen para reintentos parciales más baratos; se decidirá al implementar, no es una
  decisión de arquitectura.
