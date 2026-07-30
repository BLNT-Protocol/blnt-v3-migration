#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="${ROOT_DIR}/blend-contracts-v2"
ARTIFACT_DIR="${ROOT_DIR}/deployed-artifacts/mainnet"
MANIFEST="${ARTIFACT_DIR}/manifest.json"

for command in git jq shasum; do
    command -v "${command}" >/dev/null 2>&1 || {
        printf 'error: required command not found: %s\n' "${command}" >&2
        exit 1
    }
done

[[ -f "${SOURCE_DIR}/Cargo.toml" ]] || {
    printf 'error: initialize the v2 submodule with git submodule update --init\n' >&2
    exit 1
}
[[ -f "${MANIFEST}" ]] || {
    printf 'error: missing deployed artifact manifest: %s\n' "${MANIFEST}" >&2
    exit 1
}

expected_commit="$(jq -er '.source.v2_baseline_commit' "${MANIFEST}")"
expected_repository="$(jq -er '.source.v2_repository' "${MANIFEST}")"
configured_repository="$(
    git config -f "${ROOT_DIR}/.gitmodules" \
        --get submodule.blend-contracts-v2.url
)"
[[ "${configured_repository}" == "${expected_repository}" ]] || {
    printf 'error: v2 submodule URL is %s; expected %s\n' \
        "${configured_repository}" "${expected_repository}" >&2
    exit 1
}
actual_commit="$(git -C "${SOURCE_DIR}" rev-parse HEAD)"
[[ "${actual_commit}" == "${expected_commit}" ]] || {
    printf 'error: v2 submodule is at %s; expected %s\n' \
        "${actual_commit}" "${expected_commit}" >&2
    exit 1
}
source_status="$(git -C "${SOURCE_DIR}" status --porcelain)"
[[ -z "${source_status}" ]] || {
    printf 'error: v2 source submodule contains local changes:\n%s\n' \
        "${source_status}" >&2
    exit 1
}

while IFS=$'\t' read -r relative_path expected_hash; do
    artifact="${ARTIFACT_DIR}/${relative_path}"
    [[ -f "${artifact}" ]] || {
        printf 'error: missing deployed artifact: %s\n' "${artifact}" >&2
        exit 1
    }
    hash_line="$(shasum -a 256 "${artifact}")"
    actual_hash="${hash_line%% *}"
    [[ "${actual_hash}" == "${expected_hash}" ]] || {
        printf 'error: deployed artifact %s has SHA-256 %s; expected %s\n' \
            "${relative_path}" "${actual_hash}" "${expected_hash}" >&2
        exit 1
    }
done < <(jq -er '.artifacts[] | [.path, .sha256] | @tsv' "${MANIFEST}")

printf 'Verified v2 source %s and %s deployed mainnet artifacts.\n' \
    "${actual_commit}" "$(jq -er '.artifacts | length' "${MANIFEST}")"
