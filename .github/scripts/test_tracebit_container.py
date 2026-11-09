"""Offline regression tests. All credentials are inert fixtures."""
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("helper", Path(__file__).with_name("tracebit-docker.py"))
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


class ContainerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.home = self.root / "home"
        self.home.mkdir()
        self.stage = self.root / "stage"
        self.stage.mkdir()
        self.previous_cwd = Path.cwd()
        os.chdir(self.root)
        self.addCleanup(os.chdir, self.previous_cwd)
        self.env = {"HOME": str(self.home), "RUNNER_TEMP": str(self.root), "PATH": os.environ["PATH"]}

    def prepare(self, mode="run", args=None):
        with patch.dict(os.environ, self.env, clear=True):
            helper.prepare(self.stage, mode, args or ["--rm", "image", "echo", "hello world"])
        return (self.stage / "arguments").read_bytes().decode().split("\0")[:-1]

    def canaries(self):
        self.env.update({"__AWS__ACCESS_KEY_ID": "fake-id", "__AWS__SECRET_ACCESS_KEY": "fake-secret", "__AWS__SESSION_TOKEN": "fake-session", "__NPM__TOKEN": "fake-npm", "SECURITY_API_TOKEN": "never-forward-this", "AWS_ACCESS_KEY_ID": "real-id", "AWS_SECRET_ACCESS_KEY": "real-secret"})
        ssh = self.home / ".ssh"
        ssh.mkdir()
        key = ssh / helper.KEY_NAME
        key.write_text("fake-private-key")
        key.with_suffix(".pub").write_text("fake-public-key")
        (ssh / "config").write_text(f"Host github.com\n IdentityFile /real/key\nHost 192.0.2.1\n IdentityFile {key}\n ProxyCommand real-secret\n")

    def test_missing_credentials_preserves_command(self):
        for mode in ("run", "build", "buildx"):
            self.assertEqual(self.prepare(mode), ["--rm", "image", "echo", "hello world"])

    def test_only_canaries_are_mounted_read_only(self):
        self.canaries()
        args = self.prepare()
        self.assertIn("__NPM__TOKEN", args)
        self.assertNotIn("AWS_SESSION_TOKEN", args)
        self.assertNotIn("AWS_PROFILE", args)
        mounts = [args[i + 1] for i, value in enumerate(args) if value == "--mount"]
        self.assertEqual(len(mounts), 5)
        for mount in mounts:
            self.assertTrue(mount.endswith(",readonly"))
            fields = dict(item.split("=", 1) for item in mount.split(",") if "=" in item)
            path = Path(fields["src"])
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            self.assertNotIn("real-", path.read_text())
            self.assertNotIn("github.com", path.read_text())
        for secret in ("fake-id", "fake-secret", "fake-session", "fake-npm", "never-forward-this"):
            self.assertNotIn(secret, " ".join(args))
        self.assertIn(f"IdentityFile /root/.ssh/{helper.KEY_NAME}", (self.stage / "ssh-config").read_text())

    def test_build_covers_every_run_without_embedding_values(self):
        self.canaries()
        original = self.root / "Dockerfile.example"
        original.write_text('FROM alpine\nRUN echo one \\\n && echo two\nRUN ["echo", "three"]\n')
        ignore = Path(str(original) + ".dockerignore")
        ignore.write_text("private/\n")
        for mode in ("build", "buildx"):
            args = self.prepare(mode, ["-f", str(original), "--build-arg", "VERSION=1", "."])
            generated = Path(args[args.index("-f") + 1]).read_text()
            self.assertEqual(generated.count("RUN --mount=type=secret"), 2)
            self.assertIn("env=__AWS__ACCESS_KEY_ID", generated)
            self.assertIn('RUN ["echo", "three"]', original.read_text())
            self.assertIn('--build-arg', args)
            self.assertEqual((self.stage / "Dockerfile.dockerignore").read_text(), "private/\n")
            for value in ("fake-id", "fake-secret", "fake-session", "fake-private-key", "never-forward-this"):
                self.assertNotIn(value, generated)
                self.assertNotIn(value, " ".join(args))

    def test_incomplete_aws_and_symlink_key_are_not_mounted(self):
        self.env["__AWS__ACCESS_KEY_ID"] = "fake-id"
        ssh = self.home / ".ssh"
        ssh.mkdir()
        target = self.home / "real-key"
        target.write_text("unrelated")
        (ssh / helper.KEY_NAME).symlink_to(target)
        self.assertNotIn("--mount", self.prepare())

    def test_build_rejects_outside_context_and_symlink_dockerfiles(self):
        self.canaries()
        outside = self.home / "Dockerfile"
        outside.write_text("FROM alpine\nRUN echo unsafe\n")
        with self.assertRaises(ValueError):
            self.prepare("build", ["-f", str(outside), "."])
        alias = self.root / "Dockerfile.link"
        alias.symlink_to(outside)
        with self.assertRaises(ValueError):
            self.prepare("build", ["-f", str(alias), "."])

    def test_unsupported_build_fails_before_docker(self):
        self.canaries()
        with self.assertRaises(ValueError):
            self.prepare("build", ["."])
        source = self.root / "Dockerfile"
        source.write_text("FROM alpine\nRUN <<EOF\necho hello\nEOF\n")
        with self.assertRaises(ValueError):
            self.prepare("build", ["-f", str(source), "."])

    def test_shell_preserves_arguments_exit_status_and_cleans_up(self):
        # A fake Docker executable inspects argv without executing supplied text.
        bindir = self.root / "bin"
        bindir.mkdir()
        docker = bindir / "docker"
        docker.write_text('#!/bin/bash\n[[ "$1" == run && "$2" == --rm && "$3" == image && "$4" == "\\$(touch sentinel); echo not-executed" ]] || exit 99\nexit 23\n')
        docker.chmod(0o700)
        env = dict(self.env, PATH=str(bindir) + os.pathsep + self.env["PATH"])
        wrapper = Path(__file__).with_name("tracebit-docker.sh").resolve()
        result = subprocess.run(["bash", str(wrapper), "run", "--rm", "image", "$(touch sentinel); echo not-executed"], env=env, cwd=self.root, capture_output=True)
        self.assertEqual(result.returncode, 23, result.stderr)
        self.assertFalse((self.root / "sentinel").exists())
        self.assertFalse(list(self.root.glob("tracebit-container.*")))


if __name__ == "__main__":
    unittest.main()
