#!/usr/bin/env bash
# Fetch the API-reference overview page of each HubSpot API version into a
# gitignored cache and write the documented operation surface to the
# committed table `data/hubspot/api_surface.tsv` (D-API-SURFACE).
#
#   scripts/fetch-hubspot-surface.sh [cache-dir]   (default .cache/hubspot-surface)
#
# Each page embeds its version tab's whole navigation, so one page per
# version is enough. Only operation coordinates and titles are committed.
set -euo pipefail

CACHE="${1:-.cache/hubspot-surface}"
BASE="https://developers.hubspot.com/docs/api-reference"
OUT=data/hubspot/api_surface.tsv
mkdir -p "$CACHE" "$(dirname "$OUT")"

pages=()
for tab in legacy latest 2027-03-beta; do
  curl -fsS --retry 3 --max-time 90 "$BASE/$tab/overview" -o "$CACHE/$tab.html"
  pages+=("$CACHE/$tab.html")
done
cargo run -q -p hubspo-harvest -- surface "${pages[@]}" > "$OUT.next"
mv "$OUT.next" "$OUT"
echo "surface: $(wc -l < "$OUT") operations"
cut -f1 "$OUT" | sort | uniq -c
