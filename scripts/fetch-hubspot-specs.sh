#!/usr/bin/env bash
# Fetch HubSpot's public API catalog and the OpenAPI document of every API
# the harvester selects, into a gitignored cache. Nothing fetched here is
# committed: committed tests use synthetic fixtures only.
#
#   scripts/fetch-hubspot-specs.sh [cache-dir]     (default .cache/hubspot-specs)
#   cargo run -q -p hubspo-harvest -- report .cache/hubspot-specs > harvest.json
#
# Which version is fetched is decided by `hubspo-harvest select` (the policy
# is in crates/hubspo-harvest/src/catalog.rs), never by this script.
set -euo pipefail

CACHE="${1:-.cache/hubspot-specs}"
INDEX_URL="https://api.hubspot.com/public/api/spec/v1/specs"

mkdir -p "$CACHE/specs"
curl -fsS --retry 3 --max-time 60 "$INDEX_URL" -o "$CACHE/index.json"

fetched=0
failed=0
while IFS=$'\t' read -r slug version _stage url; do
  out="$CACHE/specs/${slug}@${version}.json"
  if [[ -s "$out" ]]; then
    continue
  fi
  if curl -fsS --retry 3 --max-time 60 "$url" -o "$out.part"; then
    mv "$out.part" "$out"
    fetched=$((fetched + 1))
  else
    rm -f "$out.part"
    echo "failed: $slug@$version $url" >&2
    failed=$((failed + 1))
  fi
done < <(cargo run -q -p hubspo-harvest -- select "$CACHE/index.json")

echo "fetched $fetched specs into $CACHE/specs ($failed failed)"
# A failure is not fatal: `report` lists every missing spec by name.
