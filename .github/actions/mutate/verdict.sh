#!/usr/bin/env bash
# Usage: verdict.sh OUT STATUS. Decides a mutants run from its report in OUT/mutants.out and
# cargo-mutants' exit STATUS. A missed mutant fails, whatever the status says, since a run with
# both missed mutants and timeouts may report either. A timeout (3) is listed and passes: the
# mutant hung the tests, so they noticed it. Any other status, such as a failing baseline, fails.
set -uo pipefail
report="$1/mutants.out"
status="$2"
if [ -s "$report/timeout.txt" ]; then
  echo "Timed out:"
  sed 's/^/- /' "$report/timeout.txt"
fi
if [ -s "$report/missed.txt" ]; then
  echo "Missed:"
  sed 's/^/- /' "$report/missed.txt"
  exit 2
fi
[ "$status" -eq 0 ] || [ "$status" -eq 3 ] || exit "$status"
