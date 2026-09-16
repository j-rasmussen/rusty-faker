# Releasing

Publishing is gated in three independent places, so nothing reaches PyPI by accident:

1. the `publish` job in `.github/workflows/release.yml` is skipped unless the repository
   variable `PYPI_PUBLISH_ENABLED` is exactly `true`,
2. it also requires the ref to be a `v*` tag, so `workflow_dispatch` can never publish,
3. the `pypi` GitHub environment only accepts `v*` tags and requires a review.

## One-time setup

Nothing below has been done yet — the first three steps need a PyPI account and are
yours to do.

1. **Register a pending Trusted Publisher on PyPI.** At
   <https://pypi.org/manage/account/publishing/>, add a publisher for a project that
   does not exist yet:

   | field | value |
   | --- | --- |
   | PyPI project name | `rusty-faker` |
   | Owner | `j-rasmussen` |
   | Repository name | `rusty-faker` |
   | Workflow name | `release.yml` |
   | Environment name | `pypi` |

   No API token is created or stored anywhere; the publish job authenticates with a
   short-lived OIDC token, which is why it declares `id-token: write`.

2. *(Recommended)* **Do the same on TestPyPI** at
   <https://test.pypi.org/manage/account/publishing/> and dry-run the upload once by
   temporarily pointing `pypa/gh-action-pypi-publish` at
   `repository-url: https://test.pypi.org/legacy/`. A name can only be uploaded once,
   so a rehearsal is cheap insurance.

3. **Arm the workflow:**

   ```bash
   gh variable set PYPI_PUBLISH_ENABLED --body true
   ```

   The `pypi` environment already exists, is restricted to `v*` tags, and lists
   `j-rasmussen` as a required reviewer, so the job will still pause for approval.

## Cutting a release

1. Bump `[workspace.package] version` in `Cargo.toml`. That is the only copy — the
   wheel metadata and `rusty_faker.__version__` both derive from it.
2. Move the `Unreleased` section of `CHANGELOG.md` down to the new version.
3. Re-run the benchmarks if anything performance-relevant changed, and update
   `benchmarks.md` (it records the versions it measured).
4. Commit, then tag and push:

   ```bash
   git tag -a v0.1.0 -m "v0.1.0"
   git push origin main v0.1.0
   ```

5. The tag starts `release.yml`: five abi3 wheels plus an sdist, then verification jobs
   that install each wheel and run the full compat suite, and build the sdist from
   source. Only after all of those pass does `publish` wait for your approval.
6. Approve the deployment. Check <https://pypi.org/p/rusty-faker>, then install from a
   clean environment to confirm:

   ```bash
   uv venv /tmp/rf-check && uv pip install --python /tmp/rf-check/bin/python rusty-faker
   /tmp/rf-check/bin/python -c "import rusty_faker; print(rusty_faker.__version__)"
   ```

## If a release goes wrong

A version number can never be reused on PyPI, even after deletion. If a release is
broken, yank it from the project's Releases page on PyPI (Manage project -> Releases ->
Yank): a yanked version stays installable for anyone who pins it exactly, but resolvers
stop choosing it. Then fix the problem and publish the next patch version. Do not delete
the release unless it leaked something sensitive, since deletion breaks pinned installs
without giving them a path forward.
