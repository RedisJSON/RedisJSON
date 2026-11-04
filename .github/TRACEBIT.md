# Tracebit CI coverage

GitHub Actions jobs in this public repository use Tracebit Community Edition
v1.4.0, pinned to `b149c6c24dbd25df59fe3c81419a96757a06a2e8`.
The action uses the repository's `SECURITY_API_TOKEN` Actions secret, customer
`sha5oz2y`, AWS profile `administrator`, region `us-east-1`, and async mode.

The step runs immediately after checkout when checkout is the first step, or
first otherwise so dependency installation is covered. Reusable workflow calls
forward the token; explicit secret mappings retain their existing restrictions.
The repository condition limits deployment to this public repository when the
workflows are copied to forks or private mirrors. Fork pull requests with no
secret retain Tracebit's warning-and-skip behavior.

## Coverage boundary

The action injects canaries into the runner and GitHub-managed job containers.
Linux build/test/package, coverage, and random-traffic `docker run` steps use
`.github/scripts/tracebit-docker-run.py` to forward canaries, following Tracebit's
container guidance. These images run as root; the mounts target `/root`.

The helper reconstructs only the canary AWS profile and mounts the dedicated
`tracebit_ci_canary` SSH key and its canary-only config read-only. Temporary files
live outside the workspace and are removed when the container command exits,
including failure and cancellation. Runner AWS profiles and unrelated SSH keys
are never copied. Prefixed AWS and optional npm/PyPI canary variables are passed
by name, keeping their values out of Docker's command line. npm/PyPI variables
are forwarded only when the action supplies them; their config files are not
mounted. The Tracebit API token is never passed to Docker.

When no canaries are available (including fork PRs without secrets), the helper
runs the original container command without extra mounts or variables. It
preserves Docker's exit status. Canary use requires internet access to alert;
Tracebit associates alerts with the workflow run, not a separate container.

Dockerfile `RUN` steps during `docker build`, FreeBSD VMs, and remote benchmark
machines remain outside this coverage. Docker build integration still needs
Tracebit/Security confirmation. Do not put canaries in image layers or artifacts.

## Rollout validation

- Install the Secure Pipelines GitHub App for this repository with read access
  to Actions and metadata, and provision `SECURITY_API_TOKEN` through GitHub
  Actions secrets.
- Inspect a trusted CI run to confirm the Tracebit step deploys canaries;
  verify jobs reached through reusable workflows as well as direct jobs.
- Confirm fork PR jobs still run without access to the token and report the
  expected skip rather than failing.
- Have the Security project owner verify repository/workflow coverage in the
  Tracebit console. Do not exercise canary credentials against AWS or SSH as a
  routine test; their use triggers an incident alert.
- Confirm container forwarding in a trusted CI run and have Security verify
  coverage. Resolve Docker image-build/other isolated environments before
  declaring full build coverage.
