#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_DIR="${REPO_ROOT}/sdk"
PYTHON_BIN="${PYTHON_BIN:-python3.11}"
REPOSITORY="${PYPI_REPOSITORY:-pypi}"

if ! command -v "${PYTHON_BIN}" >/dev/null 2>&1; then
  echo "python runtime not found: ${PYTHON_BIN}" >&2
  exit 1
fi

if [[ ! -f "${SDK_DIR}/pyproject.toml" ]]; then
  echo "sdk/pyproject.toml not found" >&2
  exit 1
fi

case "${REPOSITORY}" in
  pypi) REPO_URL="https://upload.pypi.org/legacy/" ;;
  testpypi) REPO_URL="https://test.pypi.org/legacy/" ;;
  *) REPO_URL="${REPOSITORY}" ;;
esac

echo "installing build tools..."
"${PYTHON_BIN}" -m pip install --upgrade build twine >/dev/null

echo "cleaning old artifacts..."
rm -rf "${SDK_DIR}/dist" "${SDK_DIR}/build" "${SDK_DIR}/"*.egg-info

echo "building sdk..."
"${PYTHON_BIN}" -m build "${SDK_DIR}"

echo "checking package metadata..."
"${PYTHON_BIN}" -m twine check "${SDK_DIR}"/dist/*

if [[ "${PYPI_SKIP_UPLOAD:-0}" == "1" ]]; then
  echo "build complete; upload skipped (PYPI_SKIP_UPLOAD=1)"
  exit 0
fi

if [[ -z "${PYPI_TOKEN:-}" ]]; then
  echo "PYPI_TOKEN is required for upload" >&2
  exit 1
fi

echo "uploading to ${REPOSITORY}..."
"${PYTHON_BIN}" -m twine upload \
  --non-interactive \
  --username "__token__" \
  --password "${PYPI_TOKEN}" \
  --repository-url "${REPO_URL}" \
  "${SDK_DIR}"/dist/*

echo "release complete"
