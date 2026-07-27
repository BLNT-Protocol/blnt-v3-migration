#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export BLEND_MIGRATION_NETWORK=testnet
exec bash "${ROOT_DIR}/scripts/localnet-migration.sh" "$@"
