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
if [ -z "$files" ]; then
  echo "git ls-files found no YAML file under .github"
  exit 2
fi
IFS=$'\n' read -r -d '' -a paths <<<"$files"
# A step runs from its `- ` line to the next line indented no deeper than that dash.
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
  /^ *(- )?uses: ["\047]?actions\/checkout@/ {
    open = 1; persisted = 0; step = dash; where = FILENAME ":" FNR
  }
  open && /^ *persist-credentials: false *(#.*)?$/ { persisted = 1 }
  END { close_step() }
' "${paths[@]}")
status=$?
if [ "$status" -ne 0 ]; then
  echo "awk failed with status $status"
  exit "$status"
fi
if [ -n "$hits" ]; then
  echo "These checkouts keep the job's token; add \`persist-credentials: false\` under \`with:\`:"
  echo "$hits"
  exit 1
fi
