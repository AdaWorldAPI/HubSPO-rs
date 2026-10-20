#!/usr/bin/env bash
# Fetch HubSpot's public object-definition pages and activity guides into a
# gitignored cache, merge their standard properties into the committed
# position table (append-only), and regenerate hubspo-generated.
#
#   scripts/fetch-hubspot-properties.sh [cache-dir]   (default .cache/hubspot-properties)
#
# Object-definition pages give each property's internal name, type, field
# type, group, sensitivity, hidden flag and label. The activity guides give
# internal names only. This is the provisional source of D-ONT-6 until a
# portal exists (ISS-ORACLE-PORTAL). Nothing fetched here is committed.
set -euo pipefail

CACHE="${1:-.cache/hubspot-properties}"
BASE="https://developers.hubspot.com/docs/api-reference/latest/crm"
TABLE=data/hubspot/standard_properties.tsv
mkdir -p "$CACHE" "$(dirname "$TABLE")"
touch "$TABLE"

harvest() { # object kind(objdef|guide) url file
  local object=$1 kind=$2 url=$3 file=$4
  curl -fsS --retry 3 --max-time 60 "$url" -o "$CACHE/$file"
  cargo run -q -p hubspo-harvest -- "$kind" "$CACHE/$file" > "$CACHE/$object.tsv"
  cargo run -q -p hubspo-harvest -- merge "$TABLE" "$object" "$CACHE/$object.tsv" > "$TABLE.next"
  mv "$TABLE.next" "$TABLE"
  echo "$object: $(wc -l < "$CACHE/$object.tsv") properties ($kind)"
}

# object <TAB> page slug, in the table's object order.
OBJECTS=$'contact\tcontacts
company\tcompanies
deal\tdeals
ticket\ttickets
lead\tleads
product\tproducts
line_item\tline-items
communication\tcommunications
appointment\tappointments
cart\tcarts
commerce_payment\tcommerce-payments
commerce_subscription\tcommerce-subscriptions
discount\tdiscounts
feedback_submission\tfeedback-submissions
fee\tfees
goal\tgoals
invoice\tinvoices
order\torders'
while IFS=$'\t' read -r object slug; do
  harvest "$object" objdef "$BASE/objects/$slug/object-definition" "$slug.html"
done <<< "$OBJECTS"

ACTIVITIES=$'call\tcalls
meeting\tmeetings
task\ttasks
note\tnotes
email\temails
postal_mail\tpostal-mail'
while IFS=$'\t' read -r object slug; do
  harvest "$object" guide "$BASE/activities/$slug/guide.md" "$slug.md"
done <<< "$ACTIVITIES"

cargo run -q -p hubspo-harvest -- emit "$TABLE" > crates/hubspo-generated/src/standard_properties.rs
echo "table: $(wc -l < "$TABLE") rows; regenerated crates/hubspo-generated/src/standard_properties.rs"
