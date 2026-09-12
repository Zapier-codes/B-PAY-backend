#! /usr/bin/env bash

set -euo pipefail

if [[ "${CI:-false}" != "true" && "${GITHUB_ACTIONS:-false}" != "true" ]]; then
  echo "This script is to be run in a GitHub Actions runner only. Exiting."
  exit 1
fi

mkdir -p ~/.local/bin

# If a cache restore (see the "Cache CI tool binaries" step in ci.yml)
# already placed a working jq here, skip the download entirely -- this
# is what lets a cache hit avoid the network call that flaked with
# "Could not resolve host: github.com" on 2026-09-12.
if [[ -x ~/.local/bin/jq ]] && ~/.local/bin/jq --version >/dev/null 2>&1; then
  echo "jq already present at ~/.local/bin/jq (cache hit), skipping download"
  echo "$HOME/.local/bin" >> "${GITHUB_PATH}"
  exit 0
fi

if [[ "${RUNNER_OS}" == 'Linux' && "${RUNNER_ARCH}" == 'X64' ]]; then
  JQ_ARCH='amd64'
elif [[ "${RUNNER_OS}" == 'Linux' && "${RUNNER_ARCH}" == 'ARM64' ]]; then
  JQ_ARCH='arm64'
else
  echo "::error::Unsupported runner OS or architecture"
  exit 1
fi

# Download latest `jq` binary
curl \
  --fail \
  --silent \
  --show-error \
  --location \
  --output ~/.local/bin/jq \
  "https://github.com/jqlang/jq/releases/latest/download/jq-linux-${JQ_ARCH}"

chmod +x ~/.local/bin/jq

# Update PATH
echo "$HOME/.local/bin" >> "${GITHUB_PATH}"
