"""Offline regression tests; all credentials below are inert fixtures."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("helper", Path(__file__).with_name("tracebit-docker-run.py"))
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


class ContainerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.environment = {"HOME": str(self.root), "RUNNER_TEMP": str(self.root)}

    def run_helper(self, inspect):
        with patch.dict(os.environ, self.environment, clear=True), patch.object(helper.sys, "argv", ["helper", "--rm", "image", "exit", "23"]), patch.object(helper.subprocess, "call", side_effect=inspect), patch.object(helper.signal, "signal"):
            result = helper.main()
        self.assertFalse(list(self.root.glob("tracebit-container-*")))
        return result

    def test_missing_credentials_preserves_command_and_exit_status(self):
        def inspect(command):
            self.assertEqual(command, ["docker", "run", "--rm", "image", "exit", "23"])
            return 23
        self.assertEqual(self.run_helper(inspect), 23)

    def test_only_canaries_are_mounted_read_only(self):
        self.environment.update({"__AWS__ACCESS_KEY_ID": "fake-id", "__AWS__SECRET_ACCESS_KEY": "fake-secret", "__AWS__SESSION_TOKEN": "fake-session", "__NPM__TOKEN": "fake-npm", "SECURITY_API_TOKEN": "never-forward-this"})
        ssh = self.root / ".ssh"
        ssh.mkdir()
        key = ssh / helper.KEY_NAME
        key.write_text("fake-private-key")
        key.with_suffix(".pub").write_text("fake-public-key")
        (ssh / "config").write_text(f"Host github.com\n IdentityFile /real/key\nHost 192.0.2.1\n IdentityFile {key}\n ProxyCommand real-secret\n")
        aws = self.root / ".aws"
        aws.mkdir()
        (aws / "credentials").write_text("real-credential-must-not-be-copied")

        def inspect(command):
            rendered = " ".join(command)
            for value in ("fake-id", "fake-secret", "fake-session", "fake-npm", "SECURITY_API_TOKEN"):
                self.assertNotIn(value, rendered)
            self.assertIn("__NPM__TOKEN", command)
            mounts = [command[i + 1] for i, value in enumerate(command) if value == "--mount"]
            self.assertEqual(len(mounts), 5)
            content = {}
            for mount in mounts:
                self.assertTrue(mount.endswith(",readonly"))
                fields = dict(item.split("=", 1) for item in mount.split(",") if "=" in item)
                path = Path(fields["src"])
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)
                content[fields["dst"]] = path.read_text()
            self.assertIn("fake-secret", content["/root/.aws/credentials"])
            self.assertIn(f"IdentityFile /root/.ssh/{helper.KEY_NAME}", content["/root/.ssh/config"])
            for value in content.values():
                self.assertNotIn("real-", value)
                self.assertNotIn("github.com", value)
            return 0
        self.assertEqual(self.run_helper(inspect), 0)

    def test_incomplete_aws_and_symlink_key_are_not_mounted(self):
        self.environment["__AWS__ACCESS_KEY_ID"] = "fake-id"
        ssh = self.root / ".ssh"
        ssh.mkdir()
        other = self.root / "real-key"
        other.write_text("unrelated")
        (ssh / helper.KEY_NAME).symlink_to(other)
        def inspect(command):
            self.assertNotIn("--mount", command)
            return 0
        self.run_helper(inspect)

    def test_cleanup_after_docker_error(self):
        with self.assertRaises(OSError):
            self.run_helper(lambda command: (_ for _ in ()).throw(OSError("docker unavailable")))
        self.assertFalse(list(self.root.glob("tracebit-container-*")))


if __name__ == "__main__":
    unittest.main()
