#!/usr/bin/env bash
# Fails when an `actions/checkout` step keeps the job's token in .git/config: every checkout sets
# `persist-credentials: false` under `with:`, so no later step, build scripts and tests included,
# can read it. Reads every YAML file under .github, or the files given, with yq (mikefarah).
set -uo pipefail
cd "$(dirname "$0")/.."
if [ "$#" -gt 0 ]; then
  paths=("$@")
else
  # `:(glob)` keeps `**` meaning any depth whatever GIT_*_PATHSPECS says.
  files=$(git ls-files -- ':(glob).github/**/*.yml' ':(glob).github/**/*.yaml')
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
fi
# A step is a map with `uses`; GitHub matches owner and repository without regard to case.
hits=$(yq eval --header-preprocess=false --no-doc '
  .. | select(tag == "!!map" and has("uses"))
  | select(.uses | tostring | test("(?i)^actions/checkout@"))
  | select((.with."persist-credentials" | tostring | downcase) != "false")
  | filename + ":" + (line | tostring)
' "${paths[@]}")
status=$?
if [ "$status" -ne 0 ]; then
  echo "yq failed with status $status"
  exit "$status"
fi
if [ -n "$hits" ]; then
  echo "These checkouts keep the job's token; add \`persist-credentials: false\` under \`with:\`:"
  echo "$hits"
  exit 1
fi
exit 0
