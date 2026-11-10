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

## Docker coverage

Linux builds, tests, packaging, coverage, random-traffic tests, and image
publication use `.github/scripts/tracebit-docker.sh`. The wrapper supports
`run`, `build`, and `buildx` (meaning `docker buildx build`). Python only prepares
canary files and NUL-delimited arguments; bash executes a fixed Docker command
with quoted array arguments, without eval or shell interpretation of arguments.

For `docker run`, the helper reconstructs the canary AWS profile and the dedicated
`tracebit_ci_canary` SSH key/config and mounts them read-only under `/root`.
It forwards prefixed AWS and optional npm/PyPI canary environment variables by
name. It never copies unrelated runner credentials or passes SECURITY_API_TOKEN.
Real S3 upload credentials use a separate temporary location inside the upload
container in JSON/Bloom, so their AWS CLI configuration does not write over the
read-only canary files.

For image builds, the helper creates a temporary copy of the selected Dockerfile
and adds BuildKit secret mounts to every RUN instruction. Files and environment
variables exist only during those instructions. The Dockerfile frontend is 1.10
for secret environment mounts. No credential values are written into Dockerfile
ARG/ENV/COPY instructions or command-line arguments. Both ordinary builds and
buildx publication use the same preparation. The original Dockerfile and build
context remain unchanged, including Dockerfile-specific ignore rules.

Only the existing local, root-user CI Dockerfiles with an explicit -f/--file are
supported. Backtick escape directives and RUN heredocs require explicit review.
Temporary files live outside the build context and are removed on exit, failure,
or cancellation. Without canaries, including fork PRs without secrets, Docker
runs unchanged. Cache hits and prebuilt images skip the corresponding RUN steps;
we do not force them to execute. Secrets do not persist automatically, but build
commands must not copy them into outputs or print them.

Tracebit v1.4.0 exports AWS canaries under `__AWS__`, not `AWS_`. The real AWS
credential environment remains separate. Optional npm/PyPI variables are passed
only when supplied by Tracebit; their config files are not mounted.

## Validation and remaining scope

The Tracebit container validation workflow runs offline regression tests and
real Docker/BuildKit tests using inert fixtures. It checks read-only mounts,
argument preservation, missing credentials, exit status, cleanup, and scans the
fixture image, history, and exported max-mode build cache for fixture secrets.
Build RUN instructions and test containers have networking disabled. Public
builder/base images may be downloaded. These tests do not exercise live canaries
against AWS or SSH. Run locally with:

```sh
python3 .github/scripts/test_tracebit_container.py
python3 .github/scripts/test_tracebit_buildkit.py
```

Security must confirm trusted-run coverage in the Tracebit console. Alerts need
internet access; the console associates the shared credential with the workflow
run and does not distinguish host from container. FreeBSD VMs and remote
benchmark machines remain outside this coverage.

### Preparation failures and repository scope

If staging canaries or preparing a Dockerfile fails, the wrapper emits a warning,
cleans partial staging, and runs the original Docker command without additional
canary mounts or environment flags. Docker's exit status is preserved. The warning
means container coverage is absent for that invocation; investigate it rather than
treating a green build as proof of coverage. Exception contents are not logged.

The API-token input is empty outside this repository, in addition to the step
condition. The pinned action's pre phase checks for an empty token before requesting
credentials. This also protects mirrors that define SECURITY_API_TOKEN.
