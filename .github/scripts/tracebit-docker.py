#!/usr/bin/env python3
"""Stage canary-only files and Docker arguments; never execute a command."""
import ipaddress
import os
from pathlib import Path
import re
import shlex
import shutil
import sys
import tempfile

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
        requested = Path(arguments[index]).resolve()
        # Select a discovered local file rather than using a caller-controlled
        # pathname for I/O. Dockerfiles outside this context are unsupported.
        source = next((candidate for candidate in Path.cwd().glob("Dockerfile*")
                       if re.fullmatch(r"Dockerfile(?:\.[a-zA-Z0-9_-]+)?", candidate.name)
                       and not candidate.is_symlink() and candidate.is_file()
                       and candidate.resolve() == requested), None)
        if source is None:
            raise ValueError("Dockerfile must be a regular file in the current build context")
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
        if ignore.is_symlink():
            raise ValueError("Dockerfile-specific ignore files must not be symlinks")
        if ignore.is_file():
            shutil.copyfile(ignore, root / "Dockerfile.dockerignore")
        arguments[index] = str(generated)
        print(f"Tracebit: covering {count} Dockerfile RUN instructions with BuildKit secrets", flush=True, file=sys.stderr)
    elif options:
        print("Tracebit: forwarding canaries into build container", flush=True, file=sys.stderr)
    else:
        print("Tracebit: no canaries available; running Docker unchanged", flush=True, file=sys.stderr)
    # NUL-delimited argv is consumed as an array by bash; no eval or shell parsing.
    (root / "arguments").write_bytes(b"".join(arg.encode() + b"\0" for arg in options + arguments))


def main(arguments):
    directory = None
    try:
        directory = Path(tempfile.mkdtemp(prefix="tracebit-container.", dir=os.environ.get("RUNNER_TEMP")))
        prepare(directory, arguments[0], arguments[1:])
    except BaseException as error:
        # Do not log exception text: it can contain SSH config or credentials.
        if directory is not None:
            shutil.rmtree(directory, ignore_errors=True)
        if not isinstance(error, Exception):
            raise
        print(f"Tracebit: preparation failed ({type(error).__name__})", file=sys.stderr)
        return 1
    print(directory)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
