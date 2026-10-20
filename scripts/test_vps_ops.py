import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import stat
import tarfile
import tempfile
import unittest
from unittest import mock

MODULE_PATH = Path(__file__).with_name("vps_ops.py")
spec = importlib.util.spec_from_file_location("vps_ops", MODULE_PATH)
vps_ops = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vps_ops)


class VpsOpsTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def source_tree(self):
        source = self.root / "source"
        (source / "skills").mkdir(parents=True)
        (source / "cache").mkdir()
        (source / "logs").mkdir()
        (source / "config.yaml").write_text("model: test\n", encoding="utf-8")
        (source / ".env").write_text("TOKEN=not-a-real-secret\n", encoding="utf-8")
        (source / "skills" / "hello.md").write_text("hello", encoding="utf-8")
        (source / "cache" / "large.bin").write_bytes(b"x" * 128)
        (source / "logs" / "secret.log").write_text("do not copy", encoding="utf-8")
        return source

    def test_backup_is_selective_contains_manifest_private_and_preserves_mode(self):
        source = self.source_tree()
        executable = source / "scripts" / "run.sh"
        executable.parent.mkdir()
        executable.write_text("#!/bin/sh\n", encoding="utf-8")
        executable.chmod(0o755)
        archive = self.root / "backup.tgz"
        result = vps_ops.create_backup(source, archive)
        self.assertEqual(result["files"], 4)
        manifest = vps_ops.validate_archive(archive)
        if os.name != "nt":
            self.assertEqual(manifest["files"]["scripts/run.sh"]["mode"], 0o755)
        with tarfile.open(archive, "r:gz") as tf:
            names = tf.getnames()
            self.assertIn("manifest.json", names)
            self.assertFalse(any("cache" in n or "logs" in n for n in names))
        if os.name != "nt":
            self.assertEqual(stat.S_IMODE(archive.stat().st_mode), 0o600)

    def test_backup_rejects_symlink_in_any_selected_path_ancestor(self):
        source = self.source_tree()
        outside = self.root / "outside"
        outside.mkdir()
        (outside / "stolen").write_text("secret", encoding="utf-8")
        link = source / "plugins"
        try:
            link.symlink_to(outside, target_is_directory=True)
        except OSError:
            self.skipTest("symlinks unavailable")
        with self.assertRaises(ValueError):
            vps_ops.create_backup(source, self.root / "x.tgz", include=("plugins/stolen",))

    def test_backup_rejects_output_inside_source_and_existing_without_force(self):
        source = self.source_tree()
        with self.assertRaises(ValueError):
            vps_ops.create_backup(source, source / "backups" / "x.tgz")
        archive = self.root / "backup.tgz"
        archive.write_bytes(b"keep")
        with self.assertRaises(FileExistsError):
            vps_ops.create_backup(source, archive)
        self.assertEqual(archive.read_bytes(), b"keep")
        vps_ops.create_backup(source, archive, force=True)
        self.assertEqual(vps_ops.validate_archive(archive)["format"], 1)

    def test_validate_rejects_traversal_absolute_and_links(self):
        for name, kind, link in [("payload/../../escape", tarfile.REGTYPE, ""), ("/absolute", tarfile.REGTYPE, ""), ("payload/link", tarfile.SYMTYPE, "../../escape")]:
            archive = self.root / (hashlib.sha256(name.encode()).hexdigest() + ".tgz")
            with tarfile.open(archive, "w:gz") as tf:
                info = tarfile.TarInfo(name); info.type = kind; info.linkname = link; info.size = 0
                tf.addfile(info, io.BytesIO())
            with self.assertRaises(vps_ops.UnsafeArchive):
                vps_ops.validate_archive(archive)

    def _raw_archive(self, entries):
        archive = self.root / f"raw-{len(list(self.root.glob('raw-*')))}.tgz"
        manifest = {"format": 1, "files": {}}
        with tarfile.open(archive, "w:gz") as tf:
            for name, data in entries:
                info = tarfile.TarInfo("payload/" + name); info.size = len(data); info.mode = 0o600
                tf.addfile(info, io.BytesIO(data))
                manifest["files"][name] = {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data), "mode": 0o600}
            raw = json.dumps(manifest).encode(); info = tarfile.TarInfo("manifest.json"); info.size = len(raw)
            tf.addfile(info, io.BytesIO(raw))
        return archive

    def test_archive_limits_count_single_file_and_total_size(self):
        cases = [
            ([('a', b'x'), ('b', b'y')], {"max_files": 1}),
            ([('a', b'xx')], {"max_file_size": 1}),
            ([('a', b'xx'), ('b', b'yy')], {"max_total_size": 3}),
        ]
        for entries, limits in cases:
            with self.subTest(limits=limits), self.assertRaises(vps_ops.InvalidArchive):
                vps_ops.validate_archive(self._raw_archive(entries), **limits)

    def test_restore_backup_is_exact_and_rollback_removes_introduced_files(self):
        source = self.source_tree(); archive = self.root / "source.tgz"
        vps_ops.create_backup(source, archive)
        destination = self.root / "destination"; destination.mkdir()
        (destination / "config.yaml").write_text("destination: old\n", encoding="utf-8")
        result = vps_ops.restore_backup(archive, destination, backup_dir=self.root / "backups")
        self.assertTrue((destination / "skills" / "hello.md").exists())
        vps_ops.restore_backup(result["destination_backup"], destination, backup_dir=self.root / "backups", force_backup=True)
        self.assertEqual((destination / "config.yaml").read_text(), "destination: old\n")
        self.assertFalse((destination / "skills" / "hello.md").exists())
        self.assertFalse((destination / ".env").exists())

    def test_restore_transaction_restores_replaced_and_removes_new_on_failure(self):
        archive = self._raw_archive([("config.yaml", b"new"), ("skills/new", b"new")])
        destination = self.root / "destination"; destination.mkdir()
        old = destination / "config.yaml"; old.write_bytes(b"old")
        real_replace = vps_ops.os.replace
        calls = 0
        def fail_second(src, dst):
            nonlocal calls
            if str(src).find("hermes-restore-") >= 0:
                calls += 1
                if calls == 2: raise OSError("injected")
            return real_replace(src, dst)
        with mock.patch.object(vps_ops.os, "replace", side_effect=fail_second):
            with self.assertRaises(OSError):
                vps_ops.restore_backup(archive, destination, backup_dir=self.root / "backups")
        self.assertEqual(old.read_bytes(), b"old")
        self.assertFalse((destination / "skills" / "new").exists())

    def test_restore_detects_checksum_tampering_before_mutation(self):
        destination = self.root / "destination"; destination.mkdir()
        marker = destination / "config.yaml"; marker.write_text("keep", encoding="utf-8")
        archive = self._raw_archive([("config.yaml", b"evil")])
        with tarfile.open(archive, "r:gz") as tf:
            pass
        # Corrupt the payload while retaining the manifest.
        bad = self.root / "bad.tgz"
        with tarfile.open(archive, "r:gz") as src, tarfile.open(bad, "w:gz") as dst:
            for member in src.getmembers():
                data = src.extractfile(member).read() if member.isfile() else None
                if member.name == "payload/config.yaml": data = b"xxxx"
                dst.addfile(member, io.BytesIO(data) if data is not None else None)
        with self.assertRaises(vps_ops.InvalidArchive): vps_ops.restore_backup(bad, destination)
        self.assertEqual(marker.read_text(), "keep")

    def test_dry_run_does_not_create_archive_or_change_destination(self):
        source = self.source_tree(); archive = self.root / "backup.tgz"
        result = vps_ops.create_backup(source, archive, dry_run=True)
        self.assertFalse(archive.exists()); self.assertEqual(result["files"], 3)

    def test_env_report_redacts_values(self):
        env = self.root / ".env"; env.write_text("ADMIN_TOKEN=super-secret\nEMPTY=\n", encoding="utf-8")
        report = vps_ops.inspect_env(env, required=("ADMIN_TOKEN", "MISSING"))
        self.assertNotIn("super-secret", json.dumps(report)); self.assertEqual(report["missing"], ["MISSING"])


if __name__ == "__main__": unittest.main()
