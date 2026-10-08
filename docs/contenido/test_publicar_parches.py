from __future__ import annotations

import argparse
import hashlib
import json
import os
import tarfile
import tempfile
import unittest
from contextlib import contextmanager, redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

from docs.contenido.publicar_parches import (
    Artifact,
    PublishError,
    assets_to_upload,
    build_deterministic_tar,
    build_next_manifest,
    detect_changes,
    prepare_artifacts,
    publish,
    select_assets_to_delete,
    select_source_folder,
    validate_mpq,
    verify_public_assets,
)


class FakeHttpClient:
    def __init__(self, asset_payloads: dict[str, bytes] | None = None) -> None:
        self.releases: dict[str, dict[str, object]] = {
            "patch": {"id": 1, "assets": []},
            "RuneEngraver": {"id": 2, "assets": []},
        }
        self.asset_payloads = asset_payloads or {}
        self.uploaded: list[str] = []
        self.downloads: list[str] = []

    def get_release(self, tag: str) -> dict[str, object]:
        return self.releases[tag]

    def upload_asset(self, release_id: int, name: str, path: Path, size: int):
        payload = path.read_bytes()
        self.asset_payloads[name] = payload
        self.uploaded.append(name)
        release = self.releases["patch" if release_id == 1 else "RuneEngraver"]
        release["assets"].append({"id": len(self.uploaded), "name": name, "size": size})
        return {}

    def download_sha256(self, url: str) -> str:
        name = url.rsplit("/", 1)[-1]
        self.downloads.append(name)
        return hashlib.sha256(self.asset_payloads[name]).hexdigest()


def make_artifacts(root: Path, folder_name: str = "20261008-test") -> list[Artifact]:
    folder = root / folder_name
    (folder / "esES").mkdir(parents=True)
    (folder / "RuneEngraver").mkdir()
    (folder / "patch-z.mpq").write_bytes(b"MPQ\x1azone-patch")
    (folder / "esES/patch-esES-z.mpq").write_bytes(b"MPQ\x1alocale-patch")
    (folder / "RuneEngraver/Core.lua").write_text("addon", encoding="utf-8")
    output = root / "RuneEngraver.tar"
    build_deterministic_tar(folder / "RuneEngraver", output)
    items = (
        ("patch", "Data/patch-Z.MPQ", "patch", f"patch-Z-{folder_name}.MPQ", folder / "patch-z.mpq", False),
        (
            "locale",
            "Data/esES/patch-esES-Z.MPQ",
            "patch",
            f"patch-esES-Z-{folder_name}.MPQ",
            folder / "esES/patch-esES-z.mpq",
            False,
        ),
        (
            "addon",
            "Interface/AddOns/RuneEngraver.tar",
            "RuneEngraver",
            f"RuneEngraver-{folder_name}.tar",
            output,
            True,
        ),
    )
    artifacts = []
    for key, path, tag, name, file_path, archive in items:
        payload = file_path.read_bytes()
        artifacts.append(
            Artifact(key, path, tag, name, file_path, hashlib.sha256(payload).hexdigest(), len(payload), archive)
        )
    return artifacts


def make_manifest(artifacts: list[Artifact], addon_path: str | None = None) -> dict[str, object]:
    files: list[dict[str, object]] = []
    for artifact in artifacts:
        files.append(
            {
                "path": addon_path if artifact.archive and addon_path else artifact.entry_path,
                "role": "required",
                "kind": "archive" if artifact.archive else "clientPatch",
                "sha256": artifact.digest,
                "sizeBytes": artifact.size,
                "source": {"url": artifact.public_url, "compressedSizeBytes": artifact.size, "compression": "none"},
            }
        )
    return {
        "schemaVersion": 1,
        "realm": "icetracks",
        "channel": "production",
        "clientBuild": 12340,
        "minLauncherVersion": "0.1.0",
        "manifestVersion": 7,
        "publishedAt": "2026-10-01T00:00:00Z",
        "files": files,
        "signature": {"keyId": "test-key", "algorithm": "ed25519", "value": "deadbeef"},
    }


class DeterministicTarTests(unittest.TestCase):
    def test_tar_is_stable_excludes_git_and_normalizes_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            addon = root / "RuneEngraver"
            (addon / "Nested").mkdir(parents=True)
            (addon / "Nested/File.lua").write_text("contenido", encoding="utf-8")
            (addon / ".git/objects").mkdir(parents=True)
            (addon / ".git/objects/private").write_text("no incluir", encoding="utf-8")
            first = root / "first.tar"
            second = root / "second.tar"

            build_deterministic_tar(addon, first)
            os.utime(addon / "Nested/File.lua", (100, 100))
            os.utime(addon / "Nested", (200, 200))
            build_deterministic_tar(addon, second)

            self.assertEqual(first.read_bytes(), second.read_bytes())
            with tarfile.open(first) as archive:
                members = archive.getmembers()
                self.assertTrue(members[0].isdir())
                self.assertEqual(members[0].name, "RuneEngraver")
                self.assertEqual(first.read_bytes()[:100].split(b"\0", 1)[0], b"RuneEngraver/")
                self.assertFalse(any(".git" in member.name for member in members))
                self.assertEqual([member.name for member in members], sorted(member.name for member in members))
                self.assertTrue(all(member.mtime == 0 and member.uid == 0 and member.gid == 0 for member in members))

    def test_tar_rejects_symlinks(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            addon = root / "RuneEngraver"
            addon.mkdir()
            target = root / "target"
            target.write_text("contenido", encoding="utf-8")
            (addon / "link").symlink_to(target)
            with self.assertRaises(PublishError):
                build_deterministic_tar(addon, root / "out.tar")


class SelectionAndValidationTests(unittest.TestCase):
    def test_selects_highest_ready_folder_and_does_not_skip_published(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            unfinished = root / "20261009-unfinished"
            unfinished.mkdir()
            older = root / "20261007-ready"
            older.mkdir()
            (older / "LISTO").touch()
            newest = root / "20261008-ready"
            newest.mkdir()
            (newest / "LISTO").touch()
            (newest / "PUBLICADO").write_text(json.dumps({"resultado": "publicado"}))

            self.assertEqual(select_source_folder(root), newest)
            self.assertTrue((select_source_folder(root) / "PUBLICADO").exists())
            self.assertEqual(select_source_folder(root, "20261007-ready"), older)

    def test_published_folder_exits_without_git_or_manifest_access(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            folder = source / "20261008-ready"
            folder.mkdir(parents=True)
            (folder / "LISTO").touch()
            (folder / "PUBLICADO").write_text('{"resultado":"publicado"}', encoding="utf-8")

            @contextmanager
            def no_lock(_path):
                yield

            args = argparse.Namespace(
                source_root=source,
                folder=None,
                keep=5,
                dry_run=True,
                no_push=False,
                signing_key=root / "missing-key.pem",
                token_file=root / "missing-token",
            )
            output = StringIO()
            with patch("docs.contenido.publicar_parches.exclusive_lock", no_lock):
                with redirect_stdout(output):
                    result = publish(args, repository_root=root)
            self.assertEqual(result, 0)
            self.assertIn("nada que publicar", output.getvalue())

    def test_mpq_requires_regular_file_and_magic_header(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "patch.mpq"
            path.write_bytes(b"MPQ\x1adata")
            validate_mpq(path)
            path.write_bytes(b"nope")
            with self.assertRaisesRegex(PublishError, "cabecera MPQ"):
                validate_mpq(path)


class ChangeDetectionTests(unittest.TestCase):
    def test_no_changes_and_only_mpq_change(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifacts = make_artifacts(root)
            manifest = make_manifest(artifacts)
            self.assertEqual(detect_changes(manifest, artifacts), set())

            changed_artifacts = list(artifacts)
            changed_artifacts[0] = Artifact(
                "patch",
                artifacts[0].entry_path,
                artifacts[0].release_tag,
                artifacts[0].asset_name,
                artifacts[0].local_path,
                "f" * 64,
                artifacts[0].size,
            )
            self.assertEqual(detect_changes(manifest, changed_artifacts), {"patch"})

    def test_addon_hash_change_and_path_correction_are_detected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifacts = make_artifacts(root)
            manifest = make_manifest(artifacts)
            changed_addon = list(artifacts)
            original = artifacts[2]
            changed_addon[2] = Artifact(
                original.key,
                original.entry_path,
                original.release_tag,
                original.asset_name,
                original.local_path,
                "a" * 64,
                original.size,
                True,
            )
            self.assertEqual(detect_changes(manifest, changed_addon), {"addon"})
            wrong_path = make_manifest(artifacts, "Data/Interface/AddOns/RuneEngraver.tar")
            self.assertEqual(detect_changes(wrong_path, artifacts), {"addon"})

    def test_prepare_artifacts_validates_source_structure(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            folder = root / "20261008-ready"
            (folder / "esES").mkdir(parents=True)
            (folder / "RuneEngraver").mkdir()
            (folder / "patch-z.mpq").write_bytes(b"MPQ\x1aone")
            (folder / "esES/patch-esES-z.mpq").write_bytes(b"MPQ\x1atwo")
            (folder / "RuneEngraver/Addon.lua").write_text("addon", encoding="utf-8")
            artifacts = prepare_artifacts(folder, folder.name, root)
            self.assertEqual([item.key for item in artifacts], ["patch", "locale", "addon"])


class ManifestAndRetentionTests(unittest.TestCase):
    def test_new_manifest_updates_changed_entries_without_signature(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifacts = make_artifacts(Path(temporary), "20261008-release")
            previous = make_manifest(artifacts, "Data/Interface/AddOns/RuneEngraver.tar")
            updated = build_next_manifest(
                previous, artifacts, {"patch", "addon"}, "2026-10-08T12:00:00Z"
            )
            self.assertEqual(updated["manifestVersion"], 8)
            self.assertEqual(updated["publishedAt"], "2026-10-08T12:00:00Z")
            self.assertNotIn("signature", updated)
            self.assertEqual(updated["files"][0]["path"], "Data/patch-Z.MPQ")
            self.assertEqual(
                updated["files"][0]["source"]["url"],
                "https://github.com/warcrafted-server/launcher/releases/download/"
                "patch/patch-Z-20261008-release.MPQ",
            )
            self.assertEqual(updated["files"][1]["path"], "Data/esES/patch-esES-Z.MPQ")
            addon = updated["files"][2]
            self.assertEqual(addon["path"], "Interface/AddOns/RuneEngraver.tar")
            self.assertEqual(
                addon["source"]["url"],
                "https://github.com/warcrafted-server/launcher/releases/download/"
                "RuneEngraver/RuneEngraver-20261008-release.tar",
            )
            self.assertEqual(addon["source"]["compressedSizeBytes"], addon["sizeBytes"])
            self.assertEqual(addon["source"]["compression"], "none")

    def test_retention_keeps_recent_per_family_and_references(self) -> None:
        assets = [
            {"id": 1, "name": "patch-Z-old.MPQ", "created_at": "2026-01-01T00:00:00Z"},
            {"id": 2, "name": "patch-Z-mid.MPQ", "created_at": "2026-01-02T00:00:00Z"},
            {"id": 3, "name": "patch-Z-new.MPQ", "created_at": "2026-01-03T00:00:00Z"},
            {"id": 4, "name": "patch-esES-Z-old.MPQ", "created_at": "2026-01-01T00:00:00Z"},
            {"id": 5, "name": "patch-esES-Z-mid.MPQ", "created_at": "2026-01-02T00:00:00Z"},
            {"id": 6, "name": "patch-esES-Z-new.MPQ", "created_at": "2026-01-03T00:00:00Z"},
            {"id": 7, "name": "RuneEngraver-old.tar", "created_at": "2026-01-01T00:00:00Z"},
            {"id": 8, "name": "RuneEngraver-mid.tar", "created_at": "2026-01-02T00:00:00Z"},
            {"id": 9, "name": "RuneEngraver-new.tar", "created_at": "2026-01-03T00:00:00Z"},
            {"id": 10, "name": "RuneEngraver-newest.tar", "created_at": "2026-01-04T00:00:00Z"},
            {"id": 11, "name": "patch-z.mpq", "created_at": "2020-01-01T00:00:00Z"},
            {"id": 12, "name": "RuneEngraver.tar", "created_at": "2020-01-01T00:00:00Z"},
            {"id": 13, "name": "other.zip", "created_at": "2020-01-01T00:00:00Z"},
        ]
        deleted = select_assets_to_delete(
            assets,
            2,
            ("patch-Z-", "patch-esES-Z-", "RuneEngraver-"),
            {"patch-Z-old.MPQ", "RuneEngraver-old.tar"},
        )
        self.assertEqual(
            [asset["name"] for asset in deleted],
            ["patch-esES-Z-old.MPQ", "RuneEngraver-mid.tar"],
        )


class FakeHttpWorkflowTests(unittest.TestCase):
    def test_changed_assets_upload_and_download_verification_use_fake_http(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifacts = make_artifacts(Path(temporary))
            changed = {"patch", "addon"}
            fake = FakeHttpClient()
            to_verify = assets_to_upload(artifacts, changed, fake)
            sleeps: list[int] = []
            verify_public_assets(to_verify, fake, sleeps.append)
            self.assertEqual(set(fake.uploaded), {artifacts[0].asset_name, artifacts[2].asset_name})
            self.assertEqual(set(fake.downloads), set(fake.uploaded))
            self.assertEqual(sleeps, [])

    def test_existing_same_size_asset_is_reused_and_verified(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifacts = make_artifacts(Path(temporary))
            artifact = artifacts[0]
            payload = artifact.local_path.read_bytes()
            fake = FakeHttpClient({artifact.asset_name: payload})
            fake.releases["patch"]["assets"].append(
                {"id": 99, "name": artifact.asset_name, "size": artifact.size}
            )
            to_verify = assets_to_upload(artifacts, {"patch"}, fake)
            verify_public_assets(to_verify, fake, lambda _seconds: None)
            self.assertEqual(fake.uploaded, [])
            self.assertEqual(fake.downloads, [artifact.asset_name])

    def test_existing_asset_with_different_size_aborts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifacts = make_artifacts(Path(temporary))
            artifact = artifacts[0]
            fake = FakeHttpClient()
            fake.releases["patch"]["assets"].append(
                {"id": 99, "name": artifact.asset_name, "size": artifact.size + 1}
            )
            with self.assertRaisesRegex(PublishError, "tamaño distinto"):
                assets_to_upload(artifacts, {"patch"}, fake)

    def test_failed_digest_retries_without_exposing_server_details(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifact = make_artifacts(Path(temporary))[0]

            class WrongDigestClient:
                def __init__(self):
                    self.calls = 0

                def download_sha256(self, _url):
                    self.calls += 1
                    return "0" * 64

            fake = WrongDigestClient()
            sleeps: list[int] = []
            with self.assertRaisesRegex(PublishError, "SHA-256"):
                verify_public_assets([artifact], fake, sleeps.append)
            self.assertEqual(fake.calls, 5)
            self.assertEqual(sleeps, [2, 3, 4, 5])


if __name__ == "__main__":
    unittest.main()
