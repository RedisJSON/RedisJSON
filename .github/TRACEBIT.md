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

The action injects canaries into the environment where GitHub runs the job's
steps, including GitHub-managed job containers. It does not automatically inject
canaries into nested `docker run` / `docker build` environments, FreeBSD VMs, or
remote benchmark machines. Extending that coverage is pending Security's
recommendation from Tracebit; runner coverage alone is not full build coverage.
Do not copy the Tracebit API token into images or build artifacts.

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
- Resolve nested-container coverage with Security before declaring the rollout
  complete.
