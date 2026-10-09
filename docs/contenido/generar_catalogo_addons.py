#!/usr/bin/env python3
"""Genera el catálogo firmado de addons opcionales (decisión 0005).

El catálogo es un documento JSON firmado con Ed25519, igual que el manifest del cliente: el
launcher solo instala addons que aparezcan en un catálogo firmado por la clave de confianza.

Entrada esperada en `--source-root`: una carpeta por versión de addon con el nombre
`<id>-<version>` (por ejemplo `questhelper-1.2.0`), y dentro las carpetas del addon tal como
deben quedar en `Interface/AddOns`. Los metadatos (nombre visible, descripción, autor, licencia y
web) se leen de un JSON aparte para no inventarlos: cada addon sin ficha completa se rechaza.

Uso:
    python3 generar_catalogo_addons.py \
        --source-root /ruta/addons \
        --metadata /ruta/addons-metadata.json \
        --output docs/contenido/addons.json \
        --assets-output /ruta/salida-release \
        --release-base https://github.com/warcrafted-server/launcher/releases/download/addons-20261009

Por defecto firma con la misma clave que el manifest, `~/.warcrafted/manifest-signing-key.pem`;
`--signing-key` permite indicar otra y `--unsigned-output` deja además el documento canónico sin
firmar para revisarlo antes de publicarlo.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tarfile
import tempfile
from datetime import datetime, timezone
from pathlib import Path

SCRIPT_DIRECTORY = Path(__file__).resolve().parent
if str(SCRIPT_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIRECTORY))

from generar_manifest import ManifestError, canonical_json, hash_file, sign_and_verify

CATALOG_SCHEMA_VERSION = 1
VERSION_DIRECTORY_PATTERN = re.compile(r"^(?P<id>[a-z0-9-]+)-(?P<version>[0-9][0-9A-Za-z.+-]*)$")
SEMVER_PATTERN = re.compile(
    r"^(?P<core>0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-(?P<pre>[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z.-]+)?$"
)
INVALID_FOLDER_NAMES = {".", ".."}
BLIZZARD_FOLDER_PREFIX = "blizzard_"
REQUIRED_METADATA_FIELDS = ("name", "description", "author", "license", "homepage")


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Genera el catálogo firmado de addons opcionales")
    parser.add_argument("--source-root", required=True, type=Path)
    parser.add_argument("--metadata", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument(
        "--release-base",
        required=True,
        help="URL base de la release donde se suben los TAR de los addons",
    )
    parser.add_argument(
        "--assets-output",
        required=True,
        type=Path,
        help="carpeta donde dejar los TAR exactos que hay que subir a la release de contenido",
    )
    parser.add_argument(
        "--signing-key",
        type=Path,
        default=Path("~/.warcrafted/manifest-signing-key.pem").expanduser(),
        help=(
            "clave privada Ed25519 en PEM; por defecto la misma que firma el manifest,"
            " ~/.warcrafted/manifest-signing-key.pem"
        ),
    )
    parser.add_argument("--key-id", default="warcrafted-manifest-2026")
    parser.add_argument(
        "--catalog-version",
        type=int,
        help="catalogVersion explícita; por defecto se incrementa la del catálogo anterior",
    )
    parser.add_argument(
        "--published-at",
        help="marca RFC 3339 en UTC (por defecto la hora actual); útil para generar de forma reproducible",
    )
    parser.add_argument(
        "--unsigned-output",
        type=Path,
        help="documento canónico sin firmar (para revisión o para firmar en otro paso)",
    )
    return parser.parse_args()


def load_metadata(path: Path) -> dict[str, dict[str, object]]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ManifestError(f"no se pudo leer la ficha de addons {path}: {error}") from error
    if not isinstance(document, dict):
        raise ManifestError("la ficha de addons debe ser un objeto JSON con un addon por clave")
    for addon_id, fields in document.items():
        if not isinstance(fields, dict):
            raise ManifestError(f"la ficha del addon {addon_id!r} debe ser un objeto JSON")
    return document


def validate_semver(version: str) -> None:
    match = SEMVER_PATTERN.match(version)
    if match is None:
        raise ManifestError(f"versión semántica inválida: {version!r}")
    prerelease = match.group("pre")
    if prerelease:
        for identifier in prerelease.split("."):
            if identifier.isdigit() and len(identifier) > 1 and identifier.startswith("0"):
                raise ManifestError(f"versión semántica inválida: {version!r}")


def validate_folder_name(folder: str, addon_id: str) -> None:
    if (
        not folder
        or folder in INVALID_FOLDER_NAMES
        or "/" in folder
        or "\\" in folder
        or ":" in folder
        or Path(folder).is_absolute()
        or folder.casefold().startswith(BLIZZARD_FOLDER_PREFIX)
    ):
        raise ManifestError(f"carpeta de addon no válida en {addon_id}: {folder!r}")



def addon_folders(version_directory: Path, addon_id: str) -> list[str]:
    folders: list[str] = []
    for entry in sorted(version_directory.iterdir(), key=lambda item: item.name.casefold()):
        if entry.is_symlink():
            raise ManifestError(f"no se admiten enlaces simbólicos: {entry}")
        if not entry.is_dir():
            raise ManifestError(f"solo se admiten carpetas dentro de {version_directory}")
        validate_folder_name(entry.name, addon_id)
        folders.append(entry.name)
    if not folders:
        raise ManifestError(f"el addon {addon_id} no contiene ninguna carpeta")
    folded = [folder.casefold() for folder in folders]
    if len(set(folded)) != len(folded):
        raise ManifestError(f"carpetas repetidas en {addon_id}")
    return folders


def build_deterministic_tar(addon_root: Path, output_path: Path, folders: list[str]) -> None:
    """Empaqueta las carpetas declaradas sin metadatos variables (misma entrada, mismo hash)."""
    with tarfile.open(output_path, "w", format=tarfile.PAX_FORMAT) as archive:
        for folder in folders:
            for path in sorted(
                (addon_root / folder).rglob("*"),
                key=lambda item: item.relative_to(addon_root).as_posix().casefold(),
            ):
                if path.is_symlink():
                    raise ManifestError(f"no se admiten enlaces simbólicos: {path}")
                relative = path.relative_to(addon_root)
                info = archive.gettarinfo(str(path), arcname=relative.as_posix())
                info.uid = 0
                info.gid = 0
                info.uname = ""
                info.gname = ""
                info.mtime = 0
                info.mode = 0o644 if path.is_file() else 0o755
                if path.is_file():
                    with path.open("rb") as stream:
                        archive.addfile(info, stream)
                else:
                    archive.addfile(info)


def tar_folders(archive_path: Path) -> list[str]:
    try:
        with tarfile.open(archive_path, "r") as archive:
            names = archive.getnames()
    except (OSError, tarfile.TarError) as error:
        raise ManifestError(f"no se pudo leer el TAR generado {archive_path}: {error}") from error
    if not names:
        raise ManifestError("el TAR generado está vacío")
    return sorted({name.split("/", 1)[0] for name in names})


def build_addon_entry(
    version_directory: Path,
    addon_id: str,
    version: str,
    metadata: dict[str, object],
    release_base: str,
    temporary_root: Path,
) -> dict[str, object]:
    for field in REQUIRED_METADATA_FIELDS:
        value = metadata.get(field)
        if not isinstance(value, str) or not value.strip():
            raise ManifestError(f"falta el campo {field!r} en la ficha del addon {addon_id}")
    homepage = str(metadata["homepage"])
    if not homepage.startswith("https://"):
        raise ManifestError(f"homepage del addon {addon_id} debe ser HTTPS")

    folders = addon_folders(version_directory, addon_id)
    asset_name = f"{addon_id}-{version}.tar"
    archive_path = temporary_root / asset_name
    build_deterministic_tar(version_directory, archive_path, folders)

    packaged_folders = tar_folders(archive_path)
    if packaged_folders != sorted(folders, key=str.casefold):
        raise ManifestError(
            f"el TAR de {addon_id} contiene {packaged_folders} y el catálogo declara {folders}"
        )

    digest, size_bytes = hash_file(archive_path)
    return {
        "id": addon_id,
        "name": str(metadata["name"]),
        "description": str(metadata["description"]),
        "author": str(metadata["author"]),
        "version": version,
        "license": str(metadata["license"]),
        "homepage": homepage,
        "sha256": digest,
        "sizeBytes": size_bytes,
        "source": {"url": f"{release_base.rstrip('/')}/{asset_name}"},
        "folders": folders,
    }



def previous_catalog_version(output_path: Path) -> int:
    try:
        document = json.loads(output_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ManifestError(
            f"no se pudo leer el catálogo anterior {output_path}: {error}"
        ) from error
    version = document.get("catalogVersion") if isinstance(document, dict) else None
    if isinstance(version, bool) or not isinstance(version, int) or version < 0:
        raise ManifestError("catalogVersion del catálogo anterior debe ser un entero no negativo")
    return version


def collect_addons(
    source_root: Path,
    metadata_by_id: dict[str, dict[str, object]],
    release_base: str,
    temporary_root: Path,
) -> list[dict[str, object]]:
    if not source_root.is_dir():
        raise ManifestError(f"no existe la carpeta de addons {source_root}")
    seen_ids: set[str] = set()
    seen_folders: set[str] = set()
    addons: list[dict[str, object]] = []
    for entry in sorted(source_root.iterdir(), key=lambda item: item.name.casefold()):
        if entry.name.startswith("."):
            continue
        if entry.is_symlink() or not entry.is_dir():
            raise ManifestError(f"solo se admiten carpetas de versión dentro de {source_root}")
        match = VERSION_DIRECTORY_PATTERN.match(entry.name)
        if match is None:
            raise ManifestError(
                f"la carpeta {entry.name!r} debe llamarse <id>-<version>,"
                " por ejemplo questhelper-1.2.0"
            )
        addon_id = match.group("id")
        version = match.group("version")
        validate_semver(version)
        if addon_id in seen_ids:
            raise ManifestError(f"id de addon repetido en el catálogo: {addon_id}")
        seen_ids.add(addon_id)
        metadata = metadata_by_id.get(addon_id)
        if metadata is None:
            raise ManifestError(f"no hay ficha para el addon {addon_id} en --metadata")
        addon = build_addon_entry(entry, addon_id, version, metadata, release_base, temporary_root)
        for folder in addon["folders"]:
            if folder.casefold() in seen_folders:
                raise ManifestError(f"carpeta de addon repetida en el catálogo: {folder}")
            seen_folders.add(folder.casefold())
        addons.append(addon)
    if not addons:
        raise ManifestError("no se encontró ningún addon que publicar")
    return addons


def utc_timestamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def generate(args: argparse.Namespace) -> dict[str, object]:
    metadata_by_id = load_metadata(args.metadata)
    if args.catalog_version is not None and args.catalog_version < 0:
        raise ManifestError("--catalog-version debe ser un entero no negativo")
    catalog_version = args.catalog_version
    if catalog_version is None:
        catalog_version = previous_catalog_version(args.output) + 1 if args.output.exists() else 1
    published_at = args.published_at or utc_timestamp()

    with tempfile.TemporaryDirectory(prefix="warcrafted-addons-") as temporary_directory:
        addons = collect_addons(
            args.source_root, metadata_by_id, args.release_base, Path(temporary_directory)
        )

    document: dict[str, object] = {
        "schemaVersion": CATALOG_SCHEMA_VERSION,
        "catalogVersion": catalog_version,
        "publishedAt": published_at,
        "addons": addons,
    }
    canonical_document = canonical_json(document)
    if args.unsigned_output is not None:
        args.unsigned_output.parent.mkdir(parents=True, exist_ok=True)
        args.unsigned_output.write_bytes(canonical_document + b"\n")
    if args.signing_key is None:
        print("Catálogo generado sin firmar (no se indicó --signing-key).")
        return document

    signed = sign_and_verify(canonical_document, args.signing_key, args.output, args.key_id)
    print(f"Catálogo firmado: {args.output} (catalogVersion {catalog_version})")
    return signed


def main() -> int:
    args = parse_arguments()
    try:
        generate(args)
    except ManifestError as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1
    except (OSError, ValueError) as error:
        print(f"Error: no se pudo generar el catálogo de addons ({error})", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
