#!/usr/bin/env bash
set -euo pipefail

DRIVER_VERSION="${DRIVER_VERSION:-1.60.0}"
DEST="${HOME}/.cache/ms-playwright-go/${DRIVER_VERSION}"

command -v node >/dev/null || { echo "node is required but not on PATH" >&2; exit 1; }
command -v npm  >/dev/null || { echo "npm is required but not on PATH" >&2; exit 1; }

if [ -x "${DEST}/node" ] && [ -f "${DEST}/package/cli.js" ]; then
  echo "driver ${DRIVER_VERSION} already present at ${DEST}"
else
  echo "assembling playwright driver ${DRIVER_VERSION}..."
  workdir="$(mktemp -d)"
  trap 'rm -rf "${workdir}"' EXIT

  ( cd "${workdir}" && npm install --silent --no-audit --no-fund \
      "playwright-core@${DRIVER_VERSION}" )

  rm -rf "${DEST}"
  mkdir -p "${DEST}"
  cp -r "${workdir}/node_modules/playwright-core" "${DEST}/package"
  cp "$(command -v node)" "${DEST}/node"
  chmod +x "${DEST}/node"
fi

echo "driver reports: $("${DEST}/node" "${DEST}/package/cli.js" --version)"

EXPECTED_REV="$("${DEST}/node" -p \
  "require('${DEST}/package/browsers.json').browsers.find(b => b.name === 'chromium').revision")"

BROWSERS_PATH="${PLAYWRIGHT_BROWSERS_PATH:-${HOME}/.cache/ms-playwright}"
REQUIRED_DIRS=("chromium-${EXPECTED_REV}" "chromium_headless_shell-${EXPECTED_REV}")

missing=()
if [ "${BROWSERS_PATH}" = "0" ]; then
  missing=("<PLAYWRIGHT_BROWSERS_PATH=0>")
else
  for dir in "${REQUIRED_DIRS[@]}"; do
    [ -d "${BROWSERS_PATH}/${dir}" ] || missing+=("${dir}")
  done
fi

if [ "${SKIP_BROWSER_INSTALL:-0}" = "1" ]; then
  echo "skipping chromium install (SKIP_BROWSER_INSTALL=1)"
elif [ ${#missing[@]} -eq 0 ]; then
  echo "chromium-${EXPECTED_REV} already present in ${BROWSERS_PATH}; skipping download"
else
  echo "missing in ${BROWSERS_PATH}: ${missing[*]}"
  echo "installing chromium..."
  "${DEST}/node" "${DEST}/package/cli.js" install chromium
fi

echo "done"
