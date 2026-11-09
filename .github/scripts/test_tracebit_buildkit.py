#!/usr/bin/env python3
"""Exercise Docker/BuildKit with inert canaries; scan image and exported cache.

Requires Docker with buildx. No AWS/SSH requests are made. Build RUN instructions
and test containers have networking disabled. Only public base/builder images
may be downloaded. The isolated builder, images, and temporary files are removed.
"""
import gzip
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import uuid


def run(*args, **kwargs):
    return subprocess.run(list(args), check=True, **kwargs)


def main():
    suffix = uuid.uuid4().hex[:12]
    builder = "tracebit-test-" + suffix
    tag = "tracebit-test:" + suffix
    wrapper = Path(__file__).with_name("tracebit-docker.sh").resolve()
    marker = "inert-tracebit-" + suffix
    values = [marker + ending for ending in ("-id", "-secret", "-session", "-ssh", "-npm", "-pypi")]
    needles = [value.encode() for value in values]

    def scan(data, where):
        assert not any(value in data for value in needles), f"Canary persisted in {where}"
        if data.startswith(b"\x1f\x8b"):
            scan(gzip.decompress(data), where + " (gzip)")

    with tempfile.TemporaryDirectory(prefix="tracebit-buildkit-test-") as directory:
        root = Path(directory)
        context = root / "context"
        context.mkdir()
        staging = root / "staging"
        staging.mkdir()
        home = root / "home"
        ssh = home / ".ssh"
        ssh.mkdir(parents=True)
        (ssh / "tracebit_ci_canary").write_text(values[3])
        (ssh / "tracebit_ci_canary.pub").write_text("inert-public-key")
        (ssh / "config").write_text(f"Host 192.0.2.1\n IdentityFile {ssh}/tracebit_ci_canary\n")
        source = context / "Dockerfile"
        source.write_text('''FROM alpine:3.22
RUN test -s /root/.aws/credentials && test -s /root/.ssh/tracebit_ci_canary \\
 && test -n "$__AWS__ACCESS_KEY_ID" && test -n "$__NPM__TOKEN" && test -n "$__PYPI__TOKEN" \\
 && test -z "${AWS_ACCESS_KEY_ID:-}" && test -z "${SECURITY_API_TOKEN:-}"
RUN test -s /root/.aws/credentials && test -n "$__AWS__SESSION_TOKEN" \\
 && ! (echo changed >> /root/.aws/credentials) \\
 && ! (echo changed >> /root/.ssh/tracebit_ci_canary)
''')
        # Retain access to the user's Docker context after isolating HOME.
        docker_config = os.environ.get("DOCKER_CONFIG", str(Path.home() / ".docker"))
        env = dict(os.environ, HOME=str(home), RUNNER_TEMP=str(staging), DOCKER_CONFIG=docker_config,
                   __AWS__ACCESS_KEY_ID=values[0], __AWS__SECRET_ACCESS_KEY=values[1],
                   __AWS__SESSION_TOKEN=values[2], __NPM__TOKEN=values[4], __PYPI__TOKEN=values[5],
                   SECURITY_API_TOKEN="inert-api-token-not-for-docker")
        try:
            run("docker", "buildx", "create", "--name", builder, "--driver", "docker-container")
            cache = root / "cache"
            run("bash", str(wrapper), "buildx", "--builder", builder, "--network=none", "--load",
                "--cache-to", f"type=local,dest={cache},mode=max,compression=gzip,force-compression=true",
                "-f", str(source), "-t", tag, str(context), env=env)
            assert not list(staging.iterdir()), "Staging files survived build"
            # Secrets must not be available in the finished container.
            run("docker", "run", "--rm", "--network=none", tag, "sh", "-ec",
                'test ! -s /root/.aws/credentials; test ! -s /root/.ssh/tracebit_ci_canary; '
                'test -z "${__AWS__ACCESS_KEY_ID:-}"; test -z "${__NPM__TOKEN:-}"')
            # Exercise the run wrapper with the same fixtures and verify isolation.
            run("bash", str(wrapper), "run", "--rm", "--network=none", tag, "sh", "-ec",
                'test -s /root/.aws/credentials && test -n "$__AWS__ACCESS_KEY_ID" && '
                '! (echo changed >> /root/.aws/credentials)', env=env)
            saved = root / "image.tar"
            run("docker", "save", "-o", str(saved), tag)
            with tarfile.open(saved) as archive:
                for member in archive:
                    if member.isfile():
                        scan(archive.extractfile(member).read(), member.name)
            for path in cache.rglob("*"):
                if path.is_file():
                    scan(path.read_bytes(), str(path.relative_to(cache)))
            scan(subprocess.check_output(["docker", "history", "--no-trunc", tag]), "history")
            # Fork/local builds without canaries retain ordinary Docker behavior.
            source.write_text("FROM alpine:3.22\nRUN test ! -s /root/.aws/credentials\n")
            clean_env = {key: value for key, value in env.items() if not key.startswith(("__AWS__", "__NPM__", "__PYPI__"))}
            clean_home = root / "clean-home"
            clean_home.mkdir()
            clean_env["HOME"] = str(clean_home)
            run("bash", str(wrapper), "build", "--network=none", "-f", str(source), "-t", tag, str(context), env=clean_env)
            result = subprocess.run(["bash", str(wrapper), "run", "--rm", "--network=none", tag, "sh", "-c", "exit 23"], env=env)
            assert result.returncode == 23, "Docker exit status was changed"
            assert not list(staging.iterdir()), "Staging files survived failed container"
            print("PASS: BuildKit and Docker canaries available only during execution; image, history, and max-mode cache contain no fixture secrets; cleanup and skip behavior pass")
        finally:
            subprocess.run(["docker", "image", "rm", "-f", tag], check=False)
            subprocess.run(["docker", "buildx", "rm", "-f", builder], check=False)


if __name__ == "__main__":
    main()
