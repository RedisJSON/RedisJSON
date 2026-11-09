#!/usr/bin/env python3
"""Stage canary-only files and Docker arguments; never execute a command."""
import ipaddress
import os
from pathlib import Path
import re
import shlex
import sys

AWS_NAMES = ("ACCESS_KEY_ID", "SECRET_ACCESS_KEY", "SESSION_TOKEN", "REGION", "DEFAULT_REGION", "PROFILE")
ENV_NAMES = tuple("__AWS__" + name for name in AWS_NAMES) + ("__NPM__TOKEN", "__PYPI__TOKEN")
KEY_NAME = "tracebit_ci_canary"


def prepare(root, mode, arguments):
    if mode not in ("run", "build", "buildx"):
        raise ValueError("Expected run, build, or buildx")
    options = []
    mounts = []

    def mount(name, target, content):
        source = root / name
        source.write_text(content)
        source.chmod(0o600)
        if mode == "run":
            options.extend(["--mount", f"type=bind,src={source},dst={target},readonly"])
        else:
            secret_id = "tracebit-" + name
            options.extend(["--secret", f"id={secret_id},src={source}"])
            mounts.append(f"--mount=type=secret,id={secret_id},target={target}")

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


    # Names only: never put credential values in Docker's argv or Dockerfile.
    for name in ENV_NAMES:
        if os.environ.get(name):
            if mode == "run":
                options.extend(["--env", name])
            else:
                secret_id = "tracebit-env-" + name.strip("_").lower().replace("_", "-")
                options.extend(["--secret", f"id={secret_id},env={name}"])
                mounts.append(f"--mount=type=secret,id={secret_id},env={name}")

    arguments = list(arguments)
    if mounts:
        # Our workflows use one explicit local Dockerfile and a local context.
        # Reject other forms rather than accidentally building an uninstrumented file.
        file_options = [i for i, arg in enumerate(arguments) if arg in ("-f", "--file")]
        if len(file_options) != 1 or file_options[0] + 1 >= len(arguments):
            raise ValueError("Build coverage requires one explicit -f/--file Dockerfile")
        index = file_options[0] + 1
        source = Path(arguments[index])
        dockerfile = source.read_text()
        if re.search(r"(?im)^\s*#\s*escape\s*=|^\s*RUN\b[^\n]*<<", dockerfile):
            raise ValueError("Backtick escapes and heredoc RUN instructions need explicit coverage review")
        # Secret environment mounts require Dockerfile frontend 1.10 or newer.
        dockerfile = re.sub(r"(?im)^\s*#\s*syntax\s*=.*\n", "", dockerfile)
        dockerfile = "# syntax=docker/dockerfile:1.10\n" + dockerfile
        prefix = "RUN " + " ".join(mounts) + " "
        dockerfile, count = re.subn(r"(?im)^\s*RUN[ \t]+", lambda _: prefix, dockerfile)
        if not count:
            raise ValueError("Dockerfile has no RUN instructions to cover")
        generated = root / "Dockerfile"
        generated.write_text(dockerfile)
        # Preserve per-Dockerfile context exclusions, if present.
        ignore = Path(str(source) + ".dockerignore")
        if ignore.is_file():
            (root / "Dockerfile.dockerignore").write_bytes(ignore.read_bytes())
        arguments[index] = str(generated)
        print(f"Tracebit: covering {count} Dockerfile RUN instructions with BuildKit secrets", flush=True)
    elif options:
        print("Tracebit: forwarding canaries into build container", flush=True)
    else:
        print("Tracebit: no canaries available; running Docker unchanged", flush=True)
    # NUL-delimited argv is consumed as an array by bash; no eval or shell parsing.
    (root / "arguments").write_bytes(b"".join(arg.encode() + b"\0" for arg in options + arguments))


if __name__ == "__main__":
    prepare(Path(sys.argv[1]), sys.argv[2], sys.argv[3:])
