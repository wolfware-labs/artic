#!/usr/bin/env bash
# Fails when an `actions/checkout` step under .github keeps the job's token in .git/config: every
# checkout sets `persist-credentials: false`, so no later step, build scripts and tests included,
# can read it.
set -uo pipefail
cd "$(dirname "$0")/.."
files=$(git ls-files -- '.github/*.yml' '.github/*.yaml')
status=$?
if [ "$status" -ne 0 ]; then
  echo "git ls-files failed with status $status"
  exit "$status"
fi
# A step runs from its `- ` line to the next line indented no deeper than that dash.
# shellcheck disable=SC2086 # one workflow path per word
hits=$(awk '
  function indent(s) { match(s, /^ */); return RLENGTH }
  function close_step() {
    if (open && !persisted) print where
    open = 0
  }
  FNR == 1 { close_step(); dash = -1 }
  /^[[:space:]]*$/ || /^[[:space:]]*#/ { next }
  {
    i = indent($0)
    if (open && i <= step) close_step()
    if ($0 ~ /^ *- /) dash = i
  }
  /^ *(- )?uses: actions\/checkout@/ {
    open = 1; persisted = 0; step = dash; where = FILENAME ":" FNR
  }
  open && /^ *persist-credentials: false *(#.*)?$/ { persisted = 1 }
  END { close_step() }
' $files)
if [ -n "$hits" ]; then
  echo "These checkouts keep the job's token; add \`persist-credentials: false\` under \`with:\`:"
  echo "$hits"
  exit 1
fi
