#!/usr/bin/env python3
"""Publica los parches de cliente preparados por el repositorio del servidor."""

from __future__ import annotations

import argparse
import copy
import datetime as dt
import fcntl
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from contextlib import contextmanager
from pathlib import Path
from typing import BinaryIO, Iterator


SCRIPT_DIRECTORY = Path(__file__).resolve().parent
if str(SCRIPT_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIRECTORY))

from generar_manifest import (  # noqa: E402
    ManifestError,
    canonical_json,
    hash_file,
    load_previous_manifest,
    sign_and_verify,
)


REPOSITORY = "warcrafted-server/launcher"
API_ROOT = f"https://api.github.com/repos/{REPOSITORY}"
UPLOAD_ROOT = f"https://uploads.github.com/repos/{REPOSITORY}"
PUBLIC_ROOT = f"https://github.com/{REPOSITORY}/releases/download"
FOLDER_PATTERN = re.compile(r"^[0-9]{8}-[A-Za-z0-9._-]+$")
DEFAULT_SOURCE_ROOT = Path.home() / "Repos/acore-sod/datos/parche-cliente"
DEFAULT_SIGNING_KEY = Path.home() / ".warcrafted/manifest-signing-key.pem"
DEFAULT_TOKEN_FILE = Path.home() / ".warcrafted/github-token"
MANIFEST_RELATIVE_PATH = Path("docs/contenido/manifest.json")
MPQ_HEADER = b"MPQ\x1a"


class PublishError(Exception):
    """Error operativo con un mensaje seguro para mostrar al operador."""


class NoRedirectHandler(urllib.request.HTTPRedirectHandler):
    """Evita reenviar la cabecera de autorización fuera del endpoint de la API."""

    def redirect_request(self, request, file_pointer, code, message, headers, new_url):
        return None


class GitHubClient:
    """Cliente HTTP pequeño, aislado para poder sustituirlo en pruebas."""

    def __init__(
        self,
        token: str,
        api_opener: object | None = None,
        download_opener: object | None = None,
    ) -> None:
        self._token = token
        self._api_opener = api_opener or urllib.request.build_opener(NoRedirectHandler())
        self._download_opener = download_opener or urllib.request.build_opener()

    def _open(
        self, request: urllib.request.Request, timeout: int = 60, public_download: bool = False
    ):
        try:
            opener = self._download_opener if public_download else self._api_opener
            return opener.open(request, timeout=timeout)  # type: ignore[attr-defined]
        except urllib.error.HTTPError as error:
            error.close()
            raise PublishError(f"GitHub respondió con estado HTTP {error.code}") from None
        except (urllib.error.URLError, TimeoutError, OSError):
            raise PublishError("no se pudo completar la solicitud a GitHub") from None

    def _api_json(self, method: str, url: str) -> dict[str, object]:
        request = urllib.request.Request(
            url,
            headers={
                "Authorization": f"Bearer {self._token}",
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
            },
            method=method,
        )
        with self._open(request) as response:
            try:
                result = json.loads(response.read().decode("utf-8"))
            except (UnicodeDecodeError, json.JSONDecodeError):
                raise PublishError("GitHub devolvió una respuesta JSON no válida") from None
        if not isinstance(result, dict):
            raise PublishError("GitHub devolvió una respuesta inesperada")
        return result

    def get_release(self, tag: str) -> dict[str, object]:
        url = f"{API_ROOT}/releases/tags/{urllib.parse.quote(tag, safe='-._')}"
        return self._api_json("GET", url)

    def upload_asset(
        self, release_id: int, name: str, path: Path, size: int
    ) -> dict[str, object]:
        url = (
            f"{UPLOAD_ROOT}/releases/{release_id}/assets?"
            f"name={urllib.parse.quote(name, safe='-._')}"
        )

        def chunks() -> Iterator[bytes]:
            with path.open("rb") as stream:
                while chunk := stream.read(1024 * 1024):
                    yield chunk

        request = urllib.request.Request(
            url,
            data=chunks(),
            headers={
                "Authorization": f"Bearer {self._token}",
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
                "Content-Type": "application/octet-stream",
                "Content-Length": str(size),
            },
            method="POST",
        )
        with self._open(request, timeout=1800) as response:
            try:
                result = json.loads(response.read().decode("utf-8"))
            except (UnicodeDecodeError, json.JSONDecodeError):
                raise PublishError("GitHub devolvió una respuesta de subida no válida") from None
        if not isinstance(result, dict):
            raise PublishError("GitHub devolvió una respuesta inesperada al subir el asset")
        return result

    def delete_asset(self, asset_id: int) -> None:
        url = f"{API_ROOT}/releases/assets/{asset_id}"
        request = urllib.request.Request(
            url,
            headers={
                "Authorization": f"Bearer {self._token}",
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
            },
            method="DELETE",
        )
        with self._open(request):
            pass

    def download_sha256(self, url: str) -> str:
        request = urllib.request.Request(url, headers={"Accept": "application/octet-stream"})
        digest = hashlib.sha256()
        with self._open(request, timeout=1800, public_download=True) as response:
            while chunk := response.read(1024 * 1024):
                digest.update(chunk)
        return digest.hexdigest()


class GitClient:
    """Operaciones Git del flujo; en pruebas se sustituye por un doble."""

    def __init__(self, repository_root: Path) -> None:
        self.repository_root = repository_root

    def check_main_clean_and_pull(self) -> None:
        branch = self._run("branch", "--show-current").strip()
        if branch != "main":
            raise PublishError("el repositorio del launcher debe estar en la rama main")
        dirty = self._run("status", "--porcelain", "--untracked-files=no")
        if dirty.strip():
            raise PublishError("el repositorio del launcher tiene cambios versionados sin confirmar")
        self._run("pull", "--ff-only")
        # El push final sube todo lo que haya en main: si hubiera commits locales ajenos se
        # publicarían sin aprobación, y si un push anterior falló el manifest quedaría sin subir.
        ahead = self._run("rev-list", "--count", "origin/main..HEAD").strip()
        if ahead != "0":
            raise PublishError(
                f"main tiene {ahead} commit(s) sin subir a origin; publícalos o resuélvelo antes"
            )

    def commit_manifest(self, manifest_path: Path, message: str, push: bool) -> str:
        relative_path = manifest_path.relative_to(self.repository_root)
        self._run("add", "--", relative_path.as_posix())
        self._run("commit", "-m", message)
        commit = self._run("rev-parse", "HEAD").strip()
        if push:
            self._run("push", "origin", "main")
        return commit

    def _run(self, *arguments: str) -> str:
        try:
            result = subprocess.run(
                ["git", *arguments],
                cwd=self.repository_root,
                check=True,
                capture_output=True,
                text=True,
            )
        except FileNotFoundError:
            raise PublishError("no se encontró git en PATH") from None
        except subprocess.CalledProcessError as error:
            detail = (error.stderr or "").strip().splitlines()
            suffix = f": {detail[-1]}" if detail else ""
            raise PublishError(f"falló git {arguments[0]}{suffix}") from None
        return result.stdout


class Artifact:
    def __init__(
        self,
        key: str,
        entry_path: str,
        release_tag: str,
        asset_name: str,
        local_path: Path,
        digest: str,
        size: int,
        archive: bool = False,
    ) -> None:
        self.key = key
        self.entry_path = entry_path
        self.release_tag = release_tag
        self.asset_name = asset_name
        self.local_path = local_path
        self.digest = digest
        self.size = size
        self.archive = archive

    @property
    def public_url(self) -> str:
        return (
            f"{PUBLIC_ROOT}/{urllib.parse.quote(self.release_tag, safe='-._')}/"
            f"{urllib.parse.quote(self.asset_name, safe='-._')}"
        )


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Publica los parches de cliente preparados y actualiza el manifest firmado."
    )
    parser.add_argument("--source-root", type=Path, default=DEFAULT_SOURCE_ROOT)
    parser.add_argument("--folder", metavar="NOMBRE", help="carpeta concreta que contiene LISTO")
    parser.add_argument("--keep", type=int, default=5, help="assets versionados que se conservan")
    parser.add_argument("--dry-run", action="store_true", help="analiza los cambios sin publicarlos")
    parser.add_argument("--no-push", action="store_true", help="omite git push origin main")
    parser.add_argument("--signing-key", type=Path, default=DEFAULT_SIGNING_KEY)
    parser.add_argument("--token-file", type=Path, default=DEFAULT_TOKEN_FILE)
    args = parser.parse_args()
    if args.keep < 1:
        parser.error("--keep debe ser al menos 1")
    return args


@contextmanager
def exclusive_lock(lock_path: Path) -> Iterator[None]:
    try:
        lock_path.parent.mkdir(parents=True, exist_ok=True)
        lock_file = lock_path.open("a+b")
    except OSError:
        raise PublishError("no se pudo adquirir el bloqueo exclusivo de publicación") from None
    with lock_file:
        try:
            fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX)
        except OSError:
            raise PublishError("no se pudo adquirir el bloqueo exclusivo de publicación") from None
        yield


def validate_folder_name(name: str) -> None:
    if not FOLDER_PATTERN.fullmatch(name):
        raise PublishError("el nombre de carpeta no cumple el formato AAAAMMDD-sufijo")


def select_source_folder(source_root: Path, requested_name: str | None = None) -> Path:
    if not source_root.is_dir():
        raise PublishError(f"no existe la carpeta de origen: {source_root}")
    if requested_name is not None:
        validate_folder_name(requested_name)
        candidate = source_root / requested_name
        if candidate.is_symlink() or not candidate.is_dir() or not (candidate / "LISTO").is_file():
            raise PublishError(f"la carpeta indicada no existe o no contiene LISTO: {requested_name}")
        return candidate

    candidates = sorted(
        (
            child
            for child in source_root.iterdir()
            if not child.is_symlink() and child.is_dir() and (child / "LISTO").is_file()
        ),
        key=lambda child: child.name,
    )
    if not candidates:
        raise PublishError("no hay carpetas completas con el marcador LISTO")
    return candidates[-1]


def validate_mpq(path: Path) -> None:
    try:
        file_stat = path.lstat()
    except OSError:
        raise PublishError(f"falta el archivo MPQ requerido: {path.name}") from None
    if stat.S_ISLNK(file_stat.st_mode) or not stat.S_ISREG(file_stat.st_mode):
        raise PublishError(f"el archivo MPQ debe ser un archivo regular: {path.name}")
    try:
        with path.open("rb") as stream:
            if stream.read(len(MPQ_HEADER)) != MPQ_HEADER:
                raise PublishError(f"cabecera MPQ no válida: {path.name}")
    except OSError:
        raise PublishError(f"no se pudo leer el archivo MPQ: {path.name}") from None


def _walk_addon(addon_root: Path) -> list[tuple[Path, bool]]:
    try:
        root_stat = addon_root.lstat()
    except OSError:
        raise PublishError("falta la carpeta obligatoria RuneEngraver") from None
    if stat.S_ISLNK(root_stat.st_mode) or not stat.S_ISDIR(root_stat.st_mode):
        raise PublishError("RuneEngraver debe ser una carpeta real")

    entries: list[tuple[Path, bool]] = []

    def visit(directory: Path) -> None:
        try:
            children = sorted(directory.iterdir(), key=lambda child: child.name)
        except OSError:
            raise PublishError("no se pudo recorrer la carpeta RuneEngraver") from None
        for child in children:
            try:
                child_stat = child.lstat()
            except OSError:
                raise PublishError("no se pudo inspeccionar RuneEngraver") from None
            if stat.S_ISLNK(child_stat.st_mode):
                raise PublishError("RuneEngraver no puede contener enlaces simbólicos")
            if stat.S_ISDIR(child_stat.st_mode):
                if child.name == ".git":
                    continue
                entries.append((child, True))
                visit(child)
            elif stat.S_ISREG(child_stat.st_mode):
                entries.append((child, False))
            else:
                raise PublishError("RuneEngraver no puede contener archivos especiales")

    visit(addon_root)
    return entries


def build_deterministic_tar(addon_root: Path, output_path: Path) -> None:
    entries = _walk_addon(addon_root)
    tar_entries: list[tuple[str, Path | None, bool]] = [("RuneEngraver/", None, True)]
    for path, is_directory in entries:
        relative = path.relative_to(addon_root).as_posix()
        name = f"RuneEngraver/{relative}"
        if is_directory:
            name += "/"
        tar_entries.append((name, None if is_directory else path, is_directory))
    tar_entries[1:] = sorted(tar_entries[1:], key=lambda entry: entry[0])

    try:
        with tarfile.open(output_path, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            for name, source_path, is_directory in tar_entries:
                info = tarfile.TarInfo(name)
                info.mtime = 0
                info.uid = 0
                info.gid = 0
                info.uname = ""
                info.gname = ""
                info.mode = 0o755 if is_directory else 0o644
                if is_directory:
                    info.type = tarfile.DIRTYPE
                    info.size = 0
                    archive.addfile(info)
                else:
                    assert source_path is not None
                    info.type = tarfile.REGTYPE
                    info.size = source_path.stat().st_size
                    with source_path.open("rb") as stream:
                        archive.addfile(info, stream)
    except (OSError, tarfile.TarError, ValueError):
        raise PublishError("no se pudo crear el tar determinista de RuneEngraver") from None


def prepare_artifacts(folder: Path, folder_name: str, temporary_root: Path) -> list[Artifact]:
    mpq_path = folder / "patch-z.mpq"
    locale_path = folder / "esES/patch-esES-z.mpq"
    addon_path = folder / "RuneEngraver"
    validate_mpq(mpq_path)
    validate_mpq(locale_path)
    _walk_addon(addon_path)

    addon_tar = temporary_root / "RuneEngraver.tar"
    build_deterministic_tar(addon_path, addon_tar)
    specifications = (
        ("patch", "Data/patch-Z.MPQ", "patch", f"patch-Z-{folder_name}.MPQ", mpq_path, False),
        (
            "locale",
            "Data/esES/patch-esES-Z.MPQ",
            "patch",
            f"patch-esES-Z-{folder_name}.MPQ",
            locale_path,
            False,
        ),
        (
            "addon",
            "Interface/AddOns/RuneEngraver.tar",
            "RuneEngraver",
            f"RuneEngraver-{folder_name}.tar",
            addon_tar,
            True,
        ),
    )
    artifacts = []
    for key, entry_path, tag, name, path, is_archive in specifications:
        digest, size = hash_file(path)
        artifacts.append(Artifact(key, entry_path, tag, name, path, digest, size, is_archive))
    return artifacts


def _manifest_entry(manifest: dict[str, object], artifact: Artifact) -> dict[str, object]:
    files = manifest.get("files")
    if not isinstance(files, list):
        raise ManifestError("el manifest anterior no contiene una lista files válida")
    if artifact.archive:
        matches = [
            entry
            for entry in files
            if isinstance(entry, dict)
            and entry.get("kind") == "archive"
            and Path(str(entry.get("path", ""))).name == "RuneEngraver.tar"
        ]
    else:
        matches = [
            entry
            for entry in files
            if isinstance(entry, dict) and entry.get("path") == artifact.entry_path
        ]
    if len(matches) != 1:
        raise ManifestError(f"el manifest debe contener exactamente una entrada para {artifact.key}")
    return matches[0]


def detect_changes(manifest: dict[str, object], artifacts: list[Artifact]) -> set[str]:
    changed: set[str] = set()
    for artifact in artifacts:
        entry = _manifest_entry(manifest, artifact)
        if (
            entry.get("sha256") != artifact.digest
            or entry.get("sizeBytes") != artifact.size
            or (artifact.archive and entry.get("path") != artifact.entry_path)
        ):
            changed.add(artifact.key)
    return changed


def build_next_manifest(
    previous: dict[str, object],
    artifacts: list[Artifact],
    changed: set[str],
    published_at: str | None = None,
) -> dict[str, object]:
    if not changed:
        raise ManifestError("no se puede crear una nueva versión del manifest sin cambios")
    signature = previous.get("signature")
    key_id = signature.get("keyId") if isinstance(signature, dict) else None
    if not isinstance(key_id, str) or not key_id:
        raise ManifestError("el manifest anterior no contiene un signature.keyId válido")
    result = copy.deepcopy(previous)
    result.pop("signature", None)
    version = previous.get("manifestVersion")
    if isinstance(version, bool) or not isinstance(version, int):
        raise ManifestError("manifestVersion del manifest anterior no es válido")
    result["manifestVersion"] = version + 1
    result["publishedAt"] = published_at or utc_timestamp()
    for artifact in artifacts:
        if artifact.key not in changed:
            continue
        entry = _manifest_entry(result, artifact)
        entry["path"] = artifact.entry_path
        entry["sha256"] = artifact.digest
        entry["sizeBytes"] = artifact.size
        source = entry.get("source")
        if not isinstance(source, dict):
            source = {}
            entry["source"] = source
        source["url"] = artifact.public_url
        source["compressedSizeBytes"] = artifact.size
        source["compression"] = "none"
    return result


def utc_timestamp() -> str:
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ")


def assets_to_upload(
    artifacts: list[Artifact], changed: set[str], github: GitHubClient
) -> list[Artifact]:
    releases: dict[str, dict[str, object]] = {}
    to_verify: list[Artifact] = []
    for artifact in artifacts:
        if artifact.key not in changed:
            continue
        release = releases.setdefault(artifact.release_tag, {})
        if not release:
            release.update(github.get_release(artifact.release_tag))
        release_id = release.get("id")
        assets = release.get("assets")
        if not isinstance(release_id, int) or not isinstance(assets, list):
            raise PublishError(f"la release {artifact.release_tag} tiene una respuesta incompleta")
        matches = [asset for asset in assets if isinstance(asset, dict) and asset.get("name") == artifact.asset_name]
        if len(matches) > 1:
            raise PublishError(f"hay assets duplicados llamados {artifact.asset_name}")
        if matches:
            existing_size = matches[0].get("size")
            if existing_size != artifact.size:
                raise PublishError(
                    f"el asset {artifact.asset_name} ya existe con un tamaño distinto"
                )
            print(f"Asset existente del mismo tamaño: {artifact.asset_name}")
        else:
            github.upload_asset(release_id, artifact.asset_name, artifact.local_path, artifact.size)
            print(f"Subido: {artifact.asset_name}")
        to_verify.append(artifact)
    return to_verify


def verify_public_assets(
    artifacts: list[Artifact], github: GitHubClient, sleeper=time.sleep
) -> None:
    for artifact in artifacts:
        last_failure = ""
        for attempt in range(5):
            try:
                remote_digest = github.download_sha256(artifact.public_url)
                if remote_digest == artifact.digest:
                    print(f"Verificado: {artifact.asset_name}")
                    break
                last_failure = "el SHA-256 descargado no coincide"
            except Exception:
                last_failure = "falló la descarga de verificación"
            if attempt < 4:
                sleeper(2 + attempt)
        else:
            raise PublishError(f"no se pudo verificar {artifact.asset_name}: {last_failure}")


def referenced_asset_names(manifest: dict[str, object], release_tag: str) -> set[str]:
    names: set[str] = set()

    def visit(value: object) -> None:
        if isinstance(value, dict):
            for key, nested in value.items():
                if key == "url" and isinstance(nested, str):
                    parsed = urllib.parse.urlparse(nested)
                    parts = [urllib.parse.unquote(part) for part in parsed.path.split("/")]
                    if len(parts) >= 2 and parts[-2] == release_tag:
                        names.add(parts[-1])
                visit(nested)
        elif isinstance(value, list):
            for nested in value:
                visit(nested)

    visit(manifest)
    return names


def select_assets_to_delete(
    assets: list[dict[str, object]], keep: int, prefixes: tuple[str, ...], referenced: set[str]
) -> list[dict[str, object]]:
    deletions: list[dict[str, object]] = []
    for prefix in prefixes:
        family = [
            asset
            for asset in assets
            if isinstance(asset.get("name"), str) and asset["name"].startswith(prefix)
        ]
        family.sort(
            key=lambda asset: (str(asset.get("created_at", "")), str(asset.get("name", ""))),
            reverse=True,
        )
        kept = {str(asset.get("name", "")) for asset in family[:keep]}
        kept.update(referenced)
        deletions.extend(
            asset for asset in family if str(asset.get("name", "")) not in kept
        )
    return deletions


def run_retention(github: GitHubClient, manifest: dict[str, object], keep: int) -> None:
    families = (
        ("patch", ("patch-Z-", "patch-esES-Z-")),
        ("RuneEngraver", ("RuneEngraver-",)),
    )
    for tag, prefixes in families:
        try:
            release = github.get_release(tag)
            assets = release.get("assets")
            if not isinstance(assets, list):
                raise PublishError("la release no contiene assets válidos")
            referenced = referenced_asset_names(manifest, tag)
            for asset in select_assets_to_delete(assets, keep, prefixes, referenced):
                asset_id = asset.get("id")
                name = asset.get("name", "asset desconocido")
                if not isinstance(asset_id, int):
                    print(f"Aviso: no se pudo borrar {name}: falta su id")
                    continue
                try:
                    github.delete_asset(asset_id)
                    print(f"Retirado por retención: {name}")
                except Exception:
                    print(f"Aviso: no se pudo borrar {name}")
        except Exception:
            print(f"Aviso: no se pudo aplicar la retención de la release {tag}")


def write_json_atomic(path: Path, document: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary_path: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            "w", encoding="utf-8", dir=path.parent, prefix=f".{path.name}.", delete=False
        ) as stream:
            temporary_path = Path(stream.name)
            json.dump(document, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary_path, path)
    except OSError:
        if temporary_path is not None:
            try:
                temporary_path.unlink(missing_ok=True)
            except OSError:
                pass
        raise PublishError(f"no se pudo escribir el marcador {path.name}") from None


def read_token(token_file: Path) -> str:
    try:
        token = token_file.read_text(encoding="utf-8").strip()
    except OSError:
        raise PublishError(f"no se pudo leer el token de GitHub: {token_file}") from None
    if not token:
        raise PublishError("el archivo del token de GitHub está vacío")
    return token


def publish(
    args: argparse.Namespace,
    repository_root: Path | None = None,
    github_factory=GitHubClient,
    git_factory=GitClient,
    signer=sign_and_verify,
    sleeper=time.sleep,
) -> int:
    repository_root = repository_root or Path(__file__).resolve().parents[2]
    manifest_path = repository_root / MANIFEST_RELATIVE_PATH
    source_root = args.source_root.expanduser()
    print("Paso 1: adquiriendo bloqueo exclusivo")
    with exclusive_lock(Path.home() / ".warcrafted/publicar-parches.lock"):
        if not args.dry_run:
            print("Paso 2: comprobando y actualizando el repositorio del launcher")
            git_factory(repository_root).check_main_clean_and_pull()
        else:
            print("Paso 2: dry-run, sin ejecutar comandos Git")

        folder = select_source_folder(source_root, args.folder)
        folder_name = folder.name
        validate_folder_name(folder_name)
        if (folder / "PUBLICADO").exists():
            print("nada que publicar")
            return 0
        print(f"Paso 3: carpeta seleccionada {folder_name}")
        print("Paso 4: validando MPQ y RuneEngraver")

        with tempfile.TemporaryDirectory(prefix="warcrafted-parches-") as temporary_directory:
            artifacts = prepare_artifacts(folder, folder_name, Path(temporary_directory))
            previous = load_previous_manifest(manifest_path)
            changed = detect_changes(previous, artifacts)
            print("Paso 5: tar determinista preparado")
            print("Paso 6: comparación con el manifest completada")

            if not changed:
                if args.dry_run:
                    print("Dry-run: no hay cambios; no se escribiría PUBLICADO")
                    print(
                        f"Dry-run: manifestVersion seguiría siendo {previous['manifestVersion']}"
                    )
                else:
                    write_json_atomic(
                        folder / "PUBLICADO",
                        {"resultado": "sin cambios", "fecha": utc_timestamp()},
                    )
                    print("nada que publicar")
                return 0

            next_version = int(previous["manifestVersion"]) + 1
            changed_artifacts = [artifact for artifact in artifacts if artifact.key in changed]
            if args.dry_run:
                print("Dry-run: subiría " + ", ".join(a.asset_name for a in changed_artifacts))
                print(f"Dry-run: manifestVersion resultante {next_version}")
                return 0

            token = read_token(args.token_file.expanduser())
            signing_key = args.signing_key.expanduser()
            if not signing_key.is_file():
                raise PublishError(f"no existe la clave privada indicada: {signing_key}")
            github = github_factory(token)
            print("Paso 7: publicando solo los assets modificados")
            to_verify = assets_to_upload(artifacts, changed, github)
            print("Paso 8: descargando y verificando los assets")
            verify_public_assets(to_verify, github, sleeper)

            print("Paso 9: generando y firmando el manifest")
            unsigned = build_next_manifest(previous, artifacts, changed)
            signature = previous.get("signature")
            key_id = signature.get("keyId") if isinstance(signature, dict) else None
            assert isinstance(key_id, str)
            signer(canonical_json(unsigned), signing_key, manifest_path, key_id)

            commit_message = (
                f"Publica parches {folder_name} (manifestVersion {next_version})"
            )
            print("Paso 10: confirmando el manifest en Git")
            git = git_factory(repository_root)
            commit = git.commit_manifest(manifest_path, commit_message, not args.no_push)
            print(f"Commit: {commit}")

            print("Paso 11: aplicando retención de assets")
            signed_manifest = load_previous_manifest(manifest_path)
            run_retention(github, signed_manifest, args.keep)

            print("Paso 12: marcando la carpeta como publicada")
            write_json_atomic(
                folder / "PUBLICADO",
                {
                    "resultado": "publicado",
                    "manifestVersion": next_version,
                    "commit": commit,
                    "fecha": utc_timestamp(),
                    "assets": [artifact.asset_name for artifact in changed_artifacts],
                },
            )
            return 0


def main() -> int:
    args = parse_arguments()
    try:
        return publish(args)
    except (PublishError, ManifestError) as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1
    except (OSError, ValueError, subprocess.SubprocessError):
        print("Error: falló la publicación de parches", file=sys.stderr)
        return 1
    except Exception:
        print("Error: falló la publicación de parches", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
