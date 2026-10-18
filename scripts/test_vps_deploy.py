import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class DeployScriptTests(unittest.TestCase):
    @property
    def repo(self): return Path(__file__).resolve().parents[1]

    @property
    def script(self): return self.repo / "vps-control" / "deploy" / "deploy.sh"

    def test_dry_run_plans_hardened_atomic_release_without_mutating_target(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); target = root / "app"; env = root / "control.env"
            env.write_text("ADMIN_TOKEN=placeholder-only-for-test-value\n", encoding="utf-8")
            result = subprocess.run(["bash", self.script.as_posix(), "deploy", "--source", self.repo.as_posix(), "--target", target.as_posix(), "--env-file", env.as_posix(), "--dry-run", "--skip-nginx"], capture_output=True, text=True, env={**os.environ, "PYTHON": "python"})
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("docker compose", result.stdout); self.assertIn("healthz", result.stdout)
            self.assertIn("control.env", result.stdout); self.assertFalse(target.exists())

    def test_rollback_release_must_be_confined_to_target_releases(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); target = root / "app"; outside = root / "outside"
            (outside / "vps-control").mkdir(parents=True); (outside / "vps-control" / "compose.yml").write_text("services: {}")
            result = subprocess.run(["bash", self.script.as_posix(), "rollback", "--target", target.as_posix(), "--release", outside.as_posix(), "--dry-run", "--skip-nginx"], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0); self.assertIn("outside target releases", result.stderr)

    def test_script_contains_lock_automatic_rollback_and_versioned_config(self):
        text = self.script.read_text(encoding="utf-8")
        self.assertIn("flock", text)
        self.assertIn("automatic rollback", text)
        self.assertIn('"$release/config/control.env"', text)
        self.assertIn('"$release/config/nginx-control.conf"', text)
        self.assertIn("install -m 0600", text)
        self.assertIn("mktemp -d", text)

    def test_invalid_action_is_rejected(self):
        result = subprocess.run(["bash", self.script.as_posix(), "destroy"], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__": unittest.main()
