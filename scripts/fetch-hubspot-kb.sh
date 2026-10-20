#!/usr/bin/env bash
# Fetch HubSpot's public default-property pages into a gitignored cache, then
# print each page's property labels with `hubspo-harvest kb`.
#
#   scripts/fetch-hubspot-kb.sh [cache-dir]       (default .cache/hubspot-kb)
#
# The pages give display labels only (no internal names, types or options);
# they are the provisional source of D-ONT-6's position tables until a portal
# exists (ISS-ORACLE-PORTAL). Nothing fetched here is committed.
set -euo pipefail

CACHE="${1:-.cache/hubspot-kb}"
BASE="https://knowledge.hubspot.com"
mkdir -p "$CACHE"

# object <TAB> page path
PAGES=$'contact\tproperties/hubspots-default-contact-properties
company\tproperties/hubspot-crm-default-company-properties
deal\tproperties/hubspots-default-deal-properties
ticket\tproperties/hubspots-default-ticket-properties
lead\tproperties/hubspots-default-lead-properties
product\tproperties/hubspots-default-product-properties
line_item\tproperties/hubspots-default-line-item-properties
activity\tproperties/hubspots-default-activity-properties'

while IFS=$'\t' read -r object path; do
  curl -fsS --retry 3 --max-time 60 "$BASE/$path" -o "$CACHE/$object.html"
  cargo run -q -p hubspo-harvest -- kb "$CACHE/$object.html" > "$CACHE/$object.tsv"
  echo "$object: $(wc -l < "$CACHE/$object.tsv") labels"
done <<< "$PAGES"

# Merge into the committed position table, append-only, then regenerate.
TABLE=data/hubspot/standard_properties.tsv
mkdir -p "$(dirname "$TABLE")"
touch "$TABLE"
merge() { # object kb-tsv [section ...]
  cargo run -q -p hubspo-harvest -- merge "$TABLE" "$@" > "$TABLE.next"
  mv "$TABLE.next" "$TABLE"
}
for object in contact company deal ticket lead product line_item; do
  merge "$object" "$CACHE/$object.tsv"
done
# The activity page lists shared properties first, then one section per kind.
GENERAL="General activity properties"
merge call          "$CACHE/activity.tsv" "$GENERAL" "Call properties"
merge meeting       "$CACHE/activity.tsv" "$GENERAL" "Meeting properties"
merge task          "$CACHE/activity.tsv" "$GENERAL" "Task properties"
merge note          "$CACHE/activity.tsv" "$GENERAL" "Note properties"
merge email         "$CACHE/activity.tsv" "$GENERAL" "Email properties"
merge communication "$CACHE/activity.tsv" "$GENERAL" "Communication properties"
merge postal_mail   "$CACHE/activity.tsv" "$GENERAL" "Postal mail properties"
cargo run -q -p hubspo-harvest -- emit "$TABLE" > crates/hubspo-generated/src/standard_properties.rs
echo "table: $(wc -l < "$TABLE") rows; regenerated crates/hubspo-generated/src/standard_properties.rs"
