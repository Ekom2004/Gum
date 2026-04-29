# PyPI Release

Gum's Python import remains `import gum`.
The PyPI distribution name is `usegum`.

## GitHub Actions release (recommended)

Two workflows are available:

- `.github/workflows/ci.yml`:
  - runs SDK tests on `ubuntu-latest`, `macos-latest`, and `windows-latest`
  - Python versions `3.10`, `3.11`, `3.12`
- `.github/workflows/release-pypi.yml`:
  - automatic publish on tag push `usegum-v*`
  - manual release via `workflow_dispatch` (`pypi` or `testpypi`)
  - optional `dry_run` mode (build/check only)

### One-time PyPI setup

Configure trusted publishing on PyPI for this GitHub repo/environment:

1. In PyPI project `usegum`, add a trusted publisher for this repository.
2. Use environment `pypi` for production releases.
3. Optional: configure `testpypi` environment for TestPyPI.

### Release by tag

1. Bump `sdk/pyproject.toml` version.
2. Commit and push.
3. Create and push a release tag:

```bash
git tag usegum-v0.2.0
git push origin usegum-v0.2.0
```

That tag triggers `.github/workflows/release-pypi.yml` and publishes to PyPI.

### Manual release from Actions

Use Actions -> `Release PyPI` -> `Run workflow`:

1. `target=pypi` or `target=testpypi`
2. `dry_run=true` to build/check only

## Dry run (build + metadata check, no upload)

```bash
PYPI_SKIP_UPLOAD=1 ./scripts/pypi_release.sh
```

## Publish to TestPyPI

```bash
PYPI_REPOSITORY=testpypi ./scripts/pypi_release.sh
```

## Publish to PyPI

```bash
PYPI_REPOSITORY=pypi ./scripts/pypi_release.sh
```

## Notes

- Override python runtime if needed: `PYTHON_BIN=python3.12`.
- Script always rebuilds artifacts from a clean `sdk/dist`.
- If upload fails because version already exists, bump `sdk/pyproject.toml` version and rerun.
- For automated releases, use `usegum-v*` tags to keep the trigger explicit.
