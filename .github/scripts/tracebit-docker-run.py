#!/usr/bin/env python3
"""Run a root build container with isolated, read-only Tracebit canaries."""
import ipaddress
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import tempfile


AWS_NAMES = ("ACCESS_KEY_ID", "SECRET_ACCESS_KEY", "SESSION_TOKEN", "REGION", "DEFAULT_REGION", "PROFILE")
ENV_NAMES = tuple("__AWS__" + name for name in AWS_NAMES) + ("__NPM__TOKEN", "__PYPI__TOKEN")
KEY_NAME = "tracebit_ci_canary"


def main():
    # A fresh directory per invocation stays outside the workspace/build context.
    # TemporaryDirectory removes it on success, failure, and runner cancellation.
    def cancel(signum, _frame):
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGTERM, cancel)
    with tempfile.TemporaryDirectory(prefix="tracebit-container-", dir=os.environ.get("RUNNER_TEMP")) as directory:
        root = Path(directory)
        options = []

        def mount(name, target, content):
            source = root / name
            source.write_text(content)
            source.chmod(0o600)
            options.extend(["--mount", f"type=bind,src={source},dst={target},readonly"])

        # Reconstruct only the canary profile, never copy the runner's AWS files.
        aws = {name: os.environ.get("__AWS__" + name, "") for name in AWS_NAMES}
        if all(aws[name] for name in ("ACCESS_KEY_ID", "SECRET_ACCESS_KEY", "SESSION_TOKEN")):
            # These inputs are fixed on our pinned Tracebit steps.
            mount("aws-config", "/root/.aws/config", "[profile administrator]\nregion = us-east-1\n")
            mount("aws-credentials", "/root/.aws/credentials",
                  "[administrator]\n" + "".join(f"{field} = {aws[name]}\n" for field, name in (
                      ("aws_access_key_id", "ACCESS_KEY_ID"),
                      ("aws_secret_access_key", "SECRET_ACCESS_KEY"),
                      ("aws_session_token", "SESSION_TOKEN"))))

        # The action owns this dedicated key name. Never copy other SSH keys or
        # the runner's config; reconstruct only its canary IP/IdentityFile entry.
        ssh = Path.home() / ".ssh"
        key = ssh / KEY_NAME
        if key.is_file() and not key.is_symlink():
            mount("ssh-key", f"/root/.ssh/{KEY_NAME}", key.read_text())
            public = ssh / (KEY_NAME + ".pub")
            if public.is_file() and not public.is_symlink():
                mount("ssh-public", f"/root/.ssh/{KEY_NAME}.pub", public.read_text())
            config = ssh / "config"
            hosts = []
            host = None
            if config.is_file():
                for line in config.read_text().splitlines():
                    fields = shlex.split(line, comments=True)
                    if not fields:
                        continue
                    directive = fields[0].lower()
                    if directive in ("host", "match"):
                        host = None
                        if directive == "host" and len(fields) == 2:
                            try:
                                host = str(ipaddress.ip_address(fields[1]))
                            except ValueError:
                                pass
                    elif directive == "identityfile" and fields[1:] == [str(key)] and host:
                        hosts.append(host)
            if hosts:
                mount("ssh-config", "/root/.ssh/config", "".join(
                    f"Host {host}\n    IdentityFile /root/.ssh/{KEY_NAME}\n    IdentitiesOnly yes\n"
                    "    PasswordAuthentication no\n    StrictHostKeyChecking no\n" for host in hosts))

        # Forward names, not credential values in Docker's command line. The
        # optional HTTP tokens exist only when configured in the Tracebit portal.
        for name in ENV_NAMES:
            if os.environ.get(name):
                options.extend(["--env", name])
        if options:
            print("Tracebit: forwarding canaries into build container", flush=True)
        else:
            print("Tracebit: no canaries available; running container unchanged", flush=True)
        return subprocess.call(["docker", "run", *options, *sys.argv[1:]])


if __name__ == "__main__":
    sys.exit(main())
