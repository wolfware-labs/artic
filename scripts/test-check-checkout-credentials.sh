#!/usr/bin/env bash
# Runs check-checkout-credentials.sh on its fixtures: it names exactly the lines of keeps-token.yml
# marked `flagged`, and passes drops-token.yml.
set -uo pipefail
cd "$(dirname "$0")/.."
data=scripts/testdata/checkout-credentials
failed=0

output=$(scripts/check-checkout-credentials.sh "$data/keeps-token.yml")
status=$?
named=$(printf '%s\n' "$output" | sed -n "s|^$data/keeps-token.yml:||p")
expected=$(grep -n '# flagged$' "$data/keeps-token.yml" | cut -d: -f1)
if [ "$status" -ne 1 ] || [ "$named" != "$expected" ]; then
  echo "keeps-token.yml: expected exit 1 naming lines ${expected//$'\n'/ }, got exit $status:"
  echo "$output"
  failed=1
fi

output=$(scripts/check-checkout-credentials.sh "$data/drops-token.yml")
status=$?
if [ "$status" -ne 0 ]; then
  echo "drops-token.yml: expected exit 0, got exit $status:"
  echo "$output"
  failed=1
fi

exit "$failed"
