from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT_DIRECTORY = Path(__file__).resolve().parent
if str(SCRIPT_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIRECTORY))

# ManifestError se importa del propio módulo para garantizar que es la misma clase que lanza el
# generador (evita el módulo duplicado `generar_manifest` frente a `docs.contenido.generar_manifest`).
from docs.contenido.generar_catalogo_addons import (
    ManifestError,
    build_addon_entry,
    build_deterministic_tar,
    collect_addons,
    generate,
    tar_folders,
    validate_folder_name,
    validate_semver,
)

BASE_METADATA = {
    "name": "QuestHelper",
    "description": "Guía de misiones dentro del juego.",
    "author": "Equipo de addons de WarCrafted",
    "license": "GPL-3.0-or-later",
    "homepage": "https://github.com/warcrafted-server/launcher",
}
RELEASE_BASE = "https://github.com/warcrafted-server/launcher/releases/download/addons-1"


def make_addon_tree(root: Path, addon_name: str = "QuestHelper") -> Path:
    """Crea una carpeta de addon con archivos y subcarpetas, como las que se empaquetan."""
    folder = root / addon_name
    (folder / "libs").mkdir(parents=True)
    (folder / f"{addon_name}.toc").write_text("## Interface: 30300\n", encoding="utf-8")
    (folder / "core.lua").write_text("-- addon\n", encoding="utf-8")
    (folder / "libs" / "lib.lua").write_text("return {}\n", encoding="utf-8")
    return folder


def make_source_root(root: Path, addons: dict[str, dict[str, object]]) -> Path:
    """Crea `root/<id>-<version>/` con las carpetas indicadas en `folder` (por defecto QuestHelper)."""
    for addon_id, fields in addons.items():
        version_directory = root / f"{addon_id}-{fields['version']}"
        make_addon_tree(version_directory, str(fields.get("folder", "QuestHelper")))
    return root


def metadata_for(addons: dict[str, dict[str, object]]) -> dict[str, dict[str, object]]:
    return {
        addon_id: {**BASE_METADATA, **{k: v for k, v in fields.items() if k != "folder"}}
        for addon_id, fields in addons.items()
    }


def make_args(
    source_root: Path, metadata: dict[str, dict[str, object]], output: Path, **overrides: object
) -> argparse.Namespace:
    values: dict[str, object] = {
        "source_root": source_root,
        "metadata": output.parent / "metadata.json",
        "output": output,
        "release_base": RELEASE_BASE,
        "signing_key": None,
        "key_id": "warcrafted-manifest-2026",
        "catalog_version": None,
        "published_at": "2026-10-09T12:00:00Z",
        "unsigned_output": None,
    }
    values.update(overrides)
    Path(str(values["metadata"])).write_text(json.dumps(metadata), encoding="utf-8")
    return argparse.Namespace(**values)


class BuildingTests(unittest.TestCase):
    def test_builds_entry_with_hash_size_and_folders(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = make_source_root(root, {"questhelper": {"version": "1.2.0"}})
            archives = root / "archives"
            archives.mkdir()

            entry = build_addon_entry(
                source_root / "questhelper-1.2.0",
                "questhelper",
                "1.2.0",
                BASE_METADATA,
                RELEASE_BASE,
                archives,
            )
            archive = archives / "questhelper-1.2.0.tar"

            self.assertEqual(entry["id"], "questhelper")
            self.assertEqual(entry["version"], "1.2.0")
            self.assertEqual(entry["folders"], ["QuestHelper"])
            self.assertEqual(entry["sizeBytes"], archive.stat().st_size)
            self.assertRegex(entry["sha256"], r"^[0-9a-f]{64}$")
            self.assertEqual(entry["source"]["url"], f"{RELEASE_BASE}/questhelper-1.2.0.tar")
            self.assertEqual(entry["license"], "GPL-3.0-or-later")
            self.assertEqual(entry["name"], "QuestHelper")

    def test_tar_contains_exactly_the_declared_folders(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            addon_root = make_addon_tree(root)
            archive = root / "questhelper.tar"

            build_deterministic_tar(addon_root.parent, archive, ["QuestHelper"])

            self.assertEqual(tar_folders(archive), ["QuestHelper"])

    def test_tar_is_deterministic(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            addon_root = make_addon_tree(root)
            first = root / "first.tar"
            second = root / "second.tar"

            build_deterministic_tar(addon_root.parent, first, ["QuestHelper"])
            build_deterministic_tar(addon_root.parent, second, ["QuestHelper"])

            self.assertEqual(first.read_bytes(), second.read_bytes())



class ValidationTests(unittest.TestCase):
    def test_rejects_folder_names_that_escape_the_client(self) -> None:
        bad_folders = ("..", ".", "sub/dir", "sub\\dir", "/absolute", "C:drive", "Blizzard_Auction")
        for folder in bad_folders:
            with self.subTest(folder=folder):
                with self.assertRaises(ManifestError):
                    validate_folder_name(folder, "questhelper")
        validate_folder_name("QuestHelper", "questhelper")

    def test_rejects_invalid_semver(self) -> None:
        for version in ("1.2", "v1.2.0", "1.2.0.4", "01.2.0", "1.2.0-01"):
            with self.subTest(version=version):
                with self.assertRaises(ManifestError):
                    validate_semver(version)
        validate_semver("1.2.0")
        validate_semver("1.2.0-beta.2")

    def test_rejects_directory_without_version(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_addon_tree(root / "questhelper")
            with self.assertRaisesRegex(ManifestError, "debe llamarse"):
                collect_addons(root, {"questhelper": BASE_METADATA}, RELEASE_BASE, root)

    def test_rejects_file_inside_version_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = make_source_root(root, {"questhelper": {"version": "1.0.0"}})
            (source_root / "questhelper-1.0.0" / "suelto.txt").write_text("x", encoding="utf-8")
            with self.assertRaisesRegex(ManifestError, "solo se admiten carpetas"):
                collect_addons(source_root, {"questhelper": BASE_METADATA}, RELEASE_BASE, root)

    def test_rejects_addon_without_complete_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = make_source_root(root, {"questhelper": {"version": "1.0.0"}})
            incomplete = {"questhelper": {"name": "Sin ficha", "author": "Alguien"}}
            with self.assertRaisesRegex(ManifestError, "description"):
                collect_addons(source_root, incomplete, RELEASE_BASE, root)

    def test_rejects_non_https_homepage(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archives = root / "archives"
            archives.mkdir()
            source_root = make_source_root(root, {"questhelper": {"version": "1.0.0"}})
            metadata = {**BASE_METADATA, "homepage": "http://example.com"}
            with self.assertRaisesRegex(ManifestError, "HTTPS"):
                build_addon_entry(
                    source_root / "questhelper-1.0.0",
                    "questhelper",
                    "1.0.0",
                    metadata,
                    RELEASE_BASE,
                    archives,
                )

    def test_rejects_duplicate_folders_across_addons(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            addons = {
                "questhelper": {"version": "1.0.0", "folder": "Compartido"},
                "otro-addon": {"version": "2.0.0", "folder": "CompArtido"},
            }
            source_root = make_source_root(root, addons)
            with self.assertRaisesRegex(ManifestError, "carpeta de addon repetida"):
                collect_addons(source_root, metadata_for(addons), RELEASE_BASE, root)

    def test_rejects_addon_without_folders(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = root / "questhelper-1.0.0"
            source_root.mkdir(parents=True)
            with self.assertRaisesRegex(ManifestError, "no contiene ninguna carpeta"):
                collect_addons(
                    source_root.parent, {"questhelper": BASE_METADATA}, RELEASE_BASE, root
                )


class GenerateTests(unittest.TestCase):
    def make_case(self, root: Path) -> tuple[Path, Path, argparse.Namespace]:
        addons = {"questhelper": {"version": "1.2.0"}}
        source_root = make_source_root(root / "addons", addons)
        output = root / "addons.json"
        return source_root, output, make_args(source_root, metadata_for(addons), output)

    def test_generates_canonical_unsigned_catalog(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, output, args = self.make_case(root)
            unsigned = root / "unsigned.json"
            args.unsigned_output = unsigned

            document = generate(args)

            self.assertFalse(output.exists())
            self.assertEqual(document["schemaVersion"], 1)
            self.assertEqual(document["catalogVersion"], 1)
            self.assertEqual(document["publishedAt"], "2026-10-09T12:00:00Z")
            self.assertEqual(len(document["addons"]), 1)
            self.assertEqual(document["addons"][0]["folders"], ["QuestHelper"])
            self.assertEqual(json.loads(unsigned.read_text(encoding="utf-8")), document)

    def test_increments_catalog_version_from_previous_catalog(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, output, args = self.make_case(root)
            output.write_text('{"catalogVersion": 4}', encoding="utf-8")

            self.assertEqual(generate(args)["catalogVersion"], 5)

    def test_explicit_catalog_version_wins(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, output, args = self.make_case(root)
            output.write_text('{"catalogVersion": 4}', encoding="utf-8")
            args.catalog_version = 9

            self.assertEqual(generate(args)["catalogVersion"], 9)

    def test_rejects_broken_previous_catalog(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, output, args = self.make_case(root)
            output.write_text("{no es json", encoding="utf-8")

            with self.assertRaisesRegex(ManifestError, "catálogo anterior"):
                generate(args)

    @unittest.skipUnless(shutil.which("openssl"), "openssl no está disponible")
    def test_generates_signed_catalog_with_ed25519_key(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, output, args = self.make_case(root)
            key = root / "signing-key.pem"
            subprocess.run(
                ["openssl", "genpkey", "-algorithm", "ed25519", "-out", str(key)],
                check=True,
                capture_output=True,
            )
            args.signing_key = key

            signed = generate(args)

            written = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(written, signed)
            self.assertEqual(written["signature"]["keyId"], "warcrafted-manifest-2026")
            self.assertEqual(written["signature"]["algorithm"], "ed25519")
            self.assertRegex(written["signature"]["value"], r"^[0-9a-f]{128}$")
            self.assertEqual(written["catalogVersion"], 1)


if __name__ == "__main__":
    unittest.main()
