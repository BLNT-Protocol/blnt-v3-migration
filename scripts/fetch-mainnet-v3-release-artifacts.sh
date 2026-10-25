#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="${BLEND_MAINNET_V3_ARTIFACT_MANIFEST:-${ROOT_DIR}/deployments/mainnet-v3-next-artifacts.json}"
OUTPUT_DIR="${BLEND_MAINNET_V3_ARTIFACT_DIR:-${ROOT_DIR}/.mainnet-v3/redeployment-artifacts}"

for command in curl grep jq shasum stellar; do
    command -v "${command}" >/dev/null 2>&1 || {
        printf 'error: required command not found: %s\n' "${command}" >&2
        exit 1
    }
done

jq -e '
    .schema_version == 1 and
    .status == "prepared-not-deployed" and
    (.artifacts | type == "array" and length == 5) and
    ([.artifacts[].name] | unique | length) == 5 and
    ([.artifacts[].path] | unique | length) == 5 and
    ([.artifacts[] |
        (.name | type == "string" and length > 0) and
        (.path | type == "string" and test("^[A-Za-z0-9._-]+[.]wasm$")) and
        (.sha256 | type == "string" and test("^[0-9a-f]{64}$")) and
        (.source_repo | type == "string" and test("^github:[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")) and
        (.release_tag | type == "string" and length > 0) and
        (.url | type == "string" and startswith("https://github.com/"))
    ] | all)
' "${MANIFEST}" >/dev/null || {
    printf 'error: invalid release-artifact manifest: %s\n' "${MANIFEST}" >&2
    exit 1
}

mkdir -p "${OUTPUT_DIR}"

while IFS= read -r artifact; do
    name="$(jq -r '.name' <<<"${artifact}")"
    path="$(jq -r '.path' <<<"${artifact}")"
    expected_hash="$(jq -r '.sha256' <<<"${artifact}")"
    expected_source="$(jq -r '.source_repo' <<<"${artifact}")"
    url="$(jq -r '.url' <<<"${artifact}")"
    destination="${OUTPUT_DIR}/${path}"
    temporary="$(mktemp "${OUTPUT_DIR}/.${path}.XXXXXX")"

    printf 'Fetching %s\n' "${name}"
    if ! curl -fsSL --retry 3 --retry-all-errors --connect-timeout 10 \
        --max-time 120 --output "${temporary}" "${url}"
    then
        rm -f "${temporary}"
        printf 'error: failed to fetch %s\n' "${name}" >&2
        exit 1
    fi
    actual_hash="$(shasum -a 256 "${temporary}")"
    actual_hash="${actual_hash%% *}"
    if [[ "${actual_hash}" != "${expected_hash}" ]]; then
        rm -f "${temporary}"
        printf 'error: %s hash mismatch: expected %s, got %s\n' \
            "${name}" "${expected_hash}" "${actual_hash}" >&2
        exit 1
    fi
    if ! stellar contract info meta --wasm "${temporary}" 2>/dev/null | \
        grep -Fq "source_repo: ${expected_source}"
    then
        rm -f "${temporary}"
        printf 'error: %s source_repo metadata mismatch\n' "${name}" >&2
        exit 1
    fi
    mv "${temporary}" "${destination}"
    printf 'Verified %s %s\n' "${name}" "${actual_hash}"
done < <(jq -c '.artifacts[]' "${MANIFEST}")

printf 'Verified release artifacts in %s\n' "${OUTPUT_DIR}"
