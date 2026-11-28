#!/usr/bin/env bash
# Runs check-cargo-locked.sh on its fixtures: it names exactly the lines of unlocked.yml marked
# `flagged`, names no-permissions.yml for its missing permissions block and write-all.yml,
# write-scope.yml and null-permissions.yml for top-level permissions above read, and passes
# locked.yml, read-only.yml and the composite action.yml.
set -uo pipefail
cd "$(dirname "$0")/.."
data=scripts/testdata/cargo-locked
failed=0

output=$(scripts/check-cargo-locked.sh "$data/unlocked.yml")
status=$?
named=$(printf '%s\n' "$output" | sed -n "s|^$data/unlocked.yml:\([0-9]*\):.*|\1|p" | sort -nu)
expected=$(grep -n '# flagged$' "$data/unlocked.yml" | cut -d: -f1)
if [ "$status" -ne 1 ] || [ "$named" != "$expected" ]; then
  echo "unlocked.yml: expected exit 1 naming lines ${expected//$'\n'/ }, got exit $status:"
  echo "$output"
  failed=1
fi

output=$(scripts/check-cargo-locked.sh "$data/no-permissions.yml")
status=$?
if [ "$status" -ne 1 ] || [ "$output" != "$data/no-permissions.yml: no top-level permissions block" ]; then
  echo "no-permissions.yml: expected exit 1 naming its missing permissions block, got exit $status:"
  echo "$output"
  failed=1
fi

for writes in write-all.yml write-scope.yml null-permissions.yml; do
  output=$(scripts/check-cargo-locked.sh "$data/$writes")
  status=$?
  if [ "$status" -ne 1 ] || [ "$output" != "$data/$writes: top-level permissions grant more than read" ]; then
    echo "$writes: expected exit 1 naming its top-level permissions, got exit $status:"
    echo "$output"
    failed=1
  fi
done

for passing in locked.yml action.yml read-only.yml; do
  output=$(scripts/check-cargo-locked.sh "$data/$passing")
  status=$?
  if [ "$status" -ne 0 ]; then
    echo "$passing: expected exit 0, got exit $status:"
    echo "$output"
    failed=1
  fi
done

exit "$failed"
