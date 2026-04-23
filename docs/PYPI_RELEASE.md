# PyPI Release

Gum's Python import remains `import gum`.
The PyPI distribution name is `usegum`.

## First-time setup

1. Create a PyPI token scoped to `usegum`.
2. Export token in your shell:

```bash
export PYPI_TOKEN="pypi-***"
```

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
