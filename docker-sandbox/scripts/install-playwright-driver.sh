#!/usr/bin/env bash
set -euo pipefail

DRIVER_VERSION="1.60.0"
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

echo "installing chromium..."
"${DEST}/node" "${DEST}/package/cli.js" install chromium

echo "done"
