#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

export BLEND_MIGRATION_NETWORK=testnet
export BLEND_MIGRATION_XLM_ASSET_MODE=native
export BLEND_MIGRATION_RPC_URL="${BLEND_PRODUCTION_TESTNET_RPC_URL:-https://soroban-testnet.stellar.org}"
export BLEND_TESTNET_WORK_DIR="${BLEND_PRODUCTION_TESTNET_WORK_DIR:-${ROOT_DIR}/.testnet-production}"
export BLEND_MIGRATION_CONFIG_DIR="${BLEND_PRODUCTION_TESTNET_CONFIG_DIR:-${BLEND_TESTNET_WORK_DIR}/stellar-config}"
export BLEND_MIGRATION_STATE_FILE="${BLEND_PRODUCTION_TESTNET_STATE_FILE:-${BLEND_TESTNET_WORK_DIR}/state.json}"
export BLEND_MIGRATION_OPERATOR_IDENTITY="${BLEND_PRODUCTION_TESTNET_OPERATOR_IDENTITY:-blend-v3-production-testnet-operator}"
export BLEND_MIGRATION_DEPOSITOR_IDENTITY="${BLEND_PRODUCTION_TESTNET_DEPOSITOR_IDENTITY:-blend-v3-production-testnet-depositor}"
# Bid above the minimum on public testnet; Stellar charges only the inclusion
# fee required by the network. The value remains deployment-configurable.
export STELLAR_INCLUSION_FEE="${BLEND_PRODUCTION_TESTNET_INCLUSION_FEE:-${STELLAR_INCLUSION_FEE:-10000}}"

exec bash "${ROOT_DIR}/scripts/localnet-migration.sh" "$@"
