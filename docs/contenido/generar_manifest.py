#!/usr/bin/env python3
"""Genera y firma el manifest de contenido WarCrafted."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import shlex
import stat
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import BinaryIO
from urllib.parse import quote, urlparse


# Exclusions are kept here so the content policy can be edited without changing traversal logic.
EXCLUDED_DIRECTORIES = {"enus", "documentation"}
EXCLUDED_FILENAMES = {
    "eula.html",
    "termination.html",
    "tos.html",
    "connection-help.html",
    "patch-a.mpq",
}
EXCLUDED_FILENAME_PREFIXES = ("credits",)
EXCLUDED_EXTENSIONS = {".url"}

# These two mod patches are published independently from the base client release.
SOD_PATCH_FILENAMES = {"patch-z.mpq", "patch-eses-z.mpq"}
PART_PATTERN = re.compile(r"^(?P<base>.+)\.part-(?P<index>[0-9]{3,})$", re.IGNORECASE)
CHUNK_SIZE = 1024 * 1024


class ManifestError(Exception):
    """Error de datos que se puede presentar directamente al operador."""


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Regenera y firma el manifest de contenido del cliente."
    )
    parser.add_argument("client_root", type=Path, help="carpeta raíz del cliente")
    parser.add_argument(
        "--previous-manifest", required=True, type=Path, help="manifest anterior"
    )
    parser.add_argument("--output", required=True, type=Path, help="ruta del manifest firmado")
    parser.add_argument(
        "--signing-key",
        type=Path,
        default=os.environ.get("WARCRAFTED_SIGNING_KEY"),
        help="clave privada Ed25519 (o variable WARCRAFTED_SIGNING_KEY)",
    )
    parser.add_argument(
        "--key-id", help="identificador de clave; por defecto conserva el del manifest anterior"
    )
    parser.add_argument(
        "--release-base",
        action="append",
        default=[],
        metavar="GRUPO=URL",
        help=(
            "URL HTTPS base de una release (repetible): clientBase, sodPatch, archive; "
            "clientPatch es opcional y por defecto usa clientBase"
        ),
    )
    args = parser.parse_args()
    if not args.signing_key:
        parser.error(
            "falta la clave privada: indica --signing-key o define WARCRAFTED_SIGNING_KEY"
        )
    return args


def parse_release_bases(values: list[str]) -> dict[str, str]:
    allowed_groups = {"clientBase", "clientPatch", "sodPatch", "archive"}
    release_bases: dict[str, str] = {}
    for value in values:
        group, separator, url = value.partition("=")
        if not separator or not group or not url:
            raise ManifestError(
                f"formato incorrecto para --release-base: {value!r}; usa GRUPO=URL"
            )
        if group not in allowed_groups:
            raise ManifestError(f"grupo de release desconocido: {group!r}")
        parsed = urlparse(url)
        if parsed.scheme != "https" or not parsed.netloc or parsed.query or parsed.fragment:
            raise ManifestError(f"la URL base de {group} debe ser HTTPS y no llevar query ni fragmento")
        if group in release_bases:
            raise ManifestError(f"se indicó más de una URL base para {group}")
        release_bases[group] = url.rstrip("/")
    return release_bases


def is_excluded(relative_path: Path) -> bool:
    if any(part.casefold() in EXCLUDED_DIRECTORIES for part in relative_path.parts[:-1]):
        return True
    name = relative_path.name.casefold()
    if name in EXCLUDED_FILENAMES or relative_path.suffix.casefold() in EXCLUDED_EXTENSIONS:
        return True
    return any(name.startswith(prefix) for prefix in EXCLUDED_FILENAME_PREFIXES) and name.endswith(
        ".html"
    )


def classify_kind(relative_path: Path) -> str:
    """Clasifica el archivo de forma centralizada para facilitar cambios futuros."""
    name = relative_path.name.casefold()
    suffix = relative_path.suffix.casefold()
    if name == "realmlist.wtf":
        return "config"
    if suffix in {".exe", ".dll"}:
        return "clientBase"
    if suffix == ".tar":
        return "archive"
    if name.startswith("patch"):
        return "clientPatch"
    return "clientBase"


def source_group(relative_path: Path, kind: str, release_bases: dict[str, str]) -> str:
    if kind == "archive":
        return "archive"
    if relative_path.name.casefold() in SOD_PATCH_FILENAMES:
        return "sodPatch"
    if kind == "clientPatch" and "clientPatch" in release_bases:
        return "clientPatch"
    return "clientBase"


def asset_url(release_bases: dict[str, str], group: str, asset_name: str) -> str:
    base = release_bases.get(group)
    if base is None:
        raise ManifestError(
            f"falta --release-base {group}=URL para el archivo {asset_name!r}"
        )
    return f"{base}/{quote(asset_name, safe='-._~')}"


def hash_stream(stream: BinaryIO) -> tuple[str, int]:
    digest = hashlib.sha256()
    size = 0
    while chunk := stream.read(CHUNK_SIZE):
        digest.update(chunk)
        size += len(chunk)
    return digest.hexdigest(), size


def hash_file(path: Path) -> tuple[str, int]:
    with path.open("rb") as stream:
        return hash_stream(stream)


def list_client_files(root: Path) -> tuple[dict[str, Path], dict[str, list[tuple[int, Path]]]]:
    files: dict[str, Path] = {}
    parts: dict[str, list[tuple[int, Path]]] = {}
    for current_text, directory_names, filenames in os.walk(root, topdown=True, followlinks=False):
        current = Path(current_text)
        kept_directories: list[str] = []
        for directory_name in directory_names:
            directory = current / directory_name
            relative = directory.relative_to(root)
            if any(part.casefold() in EXCLUDED_DIRECTORIES for part in relative.parts):
                continue
            if directory.is_symlink():
                raise ManifestError(f"no se admiten enlaces simbólicos dentro del cliente: {relative}")
            kept_directories.append(directory_name)
        directory_names[:] = kept_directories

        for filename in filenames:
            path = current / filename
            relative = path.relative_to(root)
            match = PART_PATTERN.match(filename)
            if match:
                logical_path = relative.with_name(match.group("base"))
                if is_excluded(logical_path):
                    continue
                index = int(match.group("index"))
                key = logical_path.as_posix()
                if path.is_symlink() or not stat.S_ISREG(path.stat().st_mode):
                    raise ManifestError(f"la pieza no es un archivo normal: {relative}")
                parts.setdefault(key, []).append((index, path))
                continue
            if is_excluded(relative):
                continue
            if path.is_symlink() or not stat.S_ISREG(path.stat().st_mode):
                raise ManifestError(f"no se admiten enlaces ni archivos especiales: {relative}")
            files[relative.as_posix()] = path
    return files, parts


def ordered_parts(logical_path: str, part_files: list[tuple[int, Path]]) -> list[tuple[int, Path]]:
    ordered = sorted(part_files, key=lambda item: item[0])
    indices = [index for index, _ in ordered]
    if len(indices) != len(set(indices)):
        raise ManifestError(f"hay sufijos de pieza duplicados para {logical_path}")
    if indices != list(range(len(indices))):
        raise ManifestError(f"las piezas de {logical_path} deben empezar en 000 y ser consecutivas")
    return ordered


def build_file_entry(
    relative_name: str,
    file_path: Path | None,
    part_files: list[tuple[int, Path]] | None,
    release_bases: dict[str, str],
) -> dict[str, object]:
    relative_path = Path(relative_name)
    kind = classify_kind(relative_path)
    group = source_group(relative_path, kind, release_bases)
    entry: dict[str, object] = {
        "path": relative_path.as_posix(),
        "role": "required",
        "kind": kind,
    }

    if part_files:
        ordered = ordered_parts(relative_name, part_files)
        final_digest = hashlib.sha256()
        part_entries: list[dict[str, object]] = []
        total_size = 0
        first_part_size = 0
        for position, (index, part_path) in enumerate(ordered):
            part_digest = hashlib.sha256()
            part_size = 0
            with part_path.open("rb") as stream:
                while chunk := stream.read(CHUNK_SIZE):
                    part_digest.update(chunk)
                    final_digest.update(chunk)
                    part_size += len(chunk)
            if position == 0:
                first_part_size = part_size
            total_size += part_size
            part_name = part_path.name
            part_entries.append(
                {
                    "sha256": part_digest.hexdigest(),
                    "sizeBytes": part_size,
                    "source": {
                        "url": asset_url(release_bases, group, part_name),
                        "compressedSizeBytes": part_size,
                        "compression": "none",
                    },
                }
            )

        assembled_digest = final_digest.hexdigest()
        if file_path is not None:
            file_digest, file_size = hash_file(file_path)
            if file_size != total_size or file_digest != assembled_digest:
                raise ManifestError(
                    f"el archivo original {relative_name} no coincide con sus piezas"
                )
        entry["sizeBytes"] = total_size
        entry["sha256"] = assembled_digest
        entry["assembly"] = {"partSizeBytes": first_part_size, "parts": part_entries}
        return entry

    if file_path is None:
        raise ManifestError(f"no se encontró el archivo ni sus piezas: {relative_name}")
    digest, size = hash_file(file_path)
    entry["sizeBytes"] = size
    entry["sha256"] = digest
    entry["source"] = {
        "url": asset_url(release_bases, group, file_path.name),
        "compressedSizeBytes": size,
        "compression": "none",
    }
    return entry


def load_previous_manifest(path: Path) -> dict[str, object]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ManifestError(f"no se pudo leer el manifest anterior {path}: {error}") from error
    if not isinstance(document, dict):
        raise ManifestError("el manifest anterior debe contener un objeto JSON")
    version = document.get("manifestVersion")
    if isinstance(version, bool) or not isinstance(version, int) or version < 0:
        raise ManifestError("manifestVersion del documento anterior debe ser un entero no negativo")
    for field in ("schemaVersion", "realm", "channel", "clientBuild", "minLauncherVersion"):
        if field not in document:
            raise ManifestError(f"falta el campo {field!r} en el manifest anterior")
    return document


def canonical_json(document: dict[str, object]) -> bytes:
    return json.dumps(
        document,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")


def run_openssl(arguments: list[str]) -> None:
    # Se asume que openssl está disponible en el PATH para firmar y verificar Ed25519.
    try:
        subprocess.run(arguments, check=True)
    except FileNotFoundError as error:
        raise ManifestError("no se encontró openssl en PATH; es necesario para firmar") from error
    except subprocess.CalledProcessError as error:
        command = " ".join(shlex.quote(part) for part in arguments)
        raise ManifestError(f"falló OpenSSL al ejecutar {command}") from error


def sign_and_verify(
    canonical_document: bytes, signing_key: Path, output_path: Path, key_id: str
) -> dict[str, object]:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="warcrafted-manifest-") as temporary_directory:
        temporary_root = Path(temporary_directory)
        canonical_path = temporary_root / "manifest.canonical.json"
        signature_path = temporary_root / "manifest.signature"
        public_key_path = temporary_root / "manifest-public.pem"
        canonical_path.write_bytes(canonical_document)

        run_openssl(
            [
                "openssl",
                "pkeyutl",
                "-sign",
                "-inkey",
                str(signing_key),
                "-rawin",
                "-in",
                str(canonical_path),
                "-out",
                str(signature_path),
            ]
        )
        run_openssl(
            [
                "openssl",
                "pkey",
                "-in",
                str(signing_key),
                "-pubout",
                "-out",
                str(public_key_path),
            ]
        )
        signature_hex = signature_path.read_bytes().hex()

        signed_document = json.loads(canonical_document.decode("utf-8"))
        signed_document["signature"] = {
            "keyId": key_id,
            "algorithm": "ed25519",
            "value": signature_hex,
        }
        output_bytes = (
            json.dumps(signed_document, ensure_ascii=False, indent=2) + "\n"
        ).encode("utf-8")
        file_descriptor, temporary_output = tempfile.mkstemp(
            prefix=f".{output_path.name}.", suffix=".tmp", dir=output_path.parent
        )
        try:
            with os.fdopen(file_descriptor, "wb") as stream:
                stream.write(output_bytes)
                stream.flush()
                os.fsync(stream.fileno())

            try:
                reloaded_document = json.loads(
                    Path(temporary_output).read_text(encoding="utf-8")
                )
            except json.JSONDecodeError as error:
                raise ManifestError(f"no se pudo releer el manifest firmado: {error}") from error
            reloaded_signature = reloaded_document.pop("signature", None)
            if (
                not isinstance(reloaded_signature, dict)
                or reloaded_signature.get("algorithm") != "ed25519"
                or not isinstance(reloaded_signature.get("value"), str)
            ):
                raise ManifestError("el manifest firmado no contiene una firma Ed25519 válida")
            try:
                reloaded_signature_bytes = bytes.fromhex(reloaded_signature["value"])
            except ValueError as error:
                raise ManifestError("la firma del manifest no es hexadecimal válida") from error
            signature_path.write_bytes(reloaded_signature_bytes)
            canonical_path.write_bytes(canonical_json(reloaded_document))
            run_openssl(
                [
                    "openssl",
                    "pkeyutl",
                    "-verify",
                    "-pubin",
                    "-inkey",
                    str(public_key_path),
                    "-rawin",
                    "-in",
                    str(canonical_path),
                    "-sigfile",
                    str(signature_path),
                ]
            )
            os.replace(temporary_output, output_path)
        except Exception:
            try:
                os.unlink(temporary_output)
            except FileNotFoundError:
                pass
            raise
    return signed_document


def generate(args: argparse.Namespace) -> dict[str, object]:
    try:
        root = args.client_root.resolve(strict=True)
    except OSError as error:
        raise ManifestError(f"no se pudo abrir la raíz del cliente: {error}") from error
    if not root.is_dir():
        raise ManifestError("la ruta de cliente debe ser una carpeta")
    if not args.signing_key.is_file():
        raise ManifestError(f"no existe la clave privada indicada: {args.signing_key}")

    release_bases = parse_release_bases(args.release_base)
    previous = load_previous_manifest(args.previous_manifest)
    signature = previous.get("signature")
    previous_key_id = signature.get("keyId") if isinstance(signature, dict) else None
    key_id = args.key_id or previous_key_id
    if not isinstance(key_id, str) or not key_id:
        raise ManifestError("indica --key-id o proporciona un keyId en el manifest anterior")

    files, parts = list_client_files(root)
    all_names = sorted(set(files) | set(parts))
    entries = [
        build_file_entry(name, files.get(name), parts.get(name), release_bases)
        for name in all_names
    ]
    document: dict[str, object] = {
        "schemaVersion": previous["schemaVersion"],
        "realm": previous["realm"],
        "channel": previous["channel"],
        "clientBuild": previous["clientBuild"],
        "manifestVersion": previous["manifestVersion"] + 1,
        "publishedAt": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace(
            "+00:00", "Z"
        ),
        "minLauncherVersion": previous["minLauncherVersion"],
        "files": entries,
    }
    return sign_and_verify(canonical_json(document), args.signing_key, args.output, key_id)


def main() -> int:
    args = parse_arguments()
    try:
        document = generate(args)
    except (ManifestError, OSError) as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1
    print(f"Archivos incluidos: {len(document['files'])}")
    print(f"manifestVersion: {document['manifestVersion']}")
    print("Firma Ed25519 verificada correctamente con OpenSSL.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
