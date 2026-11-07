#!/usr/bin/env bash
# Waits for an app's verdict on a commit and passes or fails with it, so a job can stand in for a
# check the ruleset can't require directly (see ci.yml's header).
#
#   wait-for-verdict.sh check <name> <app-id>   a check run, such as CodSpeed's
#   wait-for-verdict.sh status <prefix> <name>  every commit status starting with <prefix>, once <name>
#                                               exists, such as Codecov's
#
# Reads REPO (owner/name), SHA and GH_TOKEN; DEADLINE_S (default 1200) and POLL_S (default 20).
# In CI it also reads FORK ("true" for a pull request from a fork, whose upload gets no token, so
# no verdict can come: it warns and passes) and UPSTREAM_JOB and UPSTREAM_RESULT, the job that
# feeds the app (one that didn't succeed fails here at once instead of at the deadline).
set -euo pipefail

mode=${1:?usage: wait-for-verdict.sh check <name> <app-id> | status <prefix> <name>}
if [ "${FORK:-false}" = true ]; then
  echo "::warning::no verdict on a pull request from a fork: its upload gets no token"
  exit 0
fi
if [ -n "${UPSTREAM_RESULT:-}" ] && [ "$UPSTREAM_RESULT" != success ]; then
  echo "::error::$UPSTREAM_JOB was $UPSTREAM_RESULT, so there's no verdict to wait for"
  exit 1
fi

deadline=$((SECONDS + ${DEADLINE_S:-1200}))
poll=${POLL_S:-20}

# Prints "pending" while the verdict isn't in yet, otherwise "pass" or "fail" and the reason.
check() {
  local name=$1 app=$2
  gh api -X GET "repos/$REPO/commits/$SHA/check-runs" -f check_name="$name" -f app_id="$app" \
    --jq '[.check_runs[] | select(.status == "completed")] | sort_by(.completed_at) | last
      | if . == null then "pending"
        elif (.conclusion | IN("success", "neutral", "skipped")) then "pass \(.conclusion)"
        else "fail \(.conclusion): \(.html_url)" end'
}

# Each context's latest status. `gh --jq` takes no --arg, so the prefix and name go in through
# the environment.
status() {
  PREFIX=$1 NAME=$2 gh api "repos/$REPO/commits/$SHA/status" --jq '
    [.statuses[] | select(.context | startswith(env.PREFIX))] as $all
    | if ($all | map(.context) | index(env.NAME)) == null or ($all | any(.state == "pending")) then "pending"
      elif ($all | all(.state == "success")) then "pass \($all | map(.context) | join(", "))"
      else "fail \($all | map(select(.state != "success") | "\(.context) \(.state): \(.target_url)") | join("; "))"
      end'
}

while :; do
  case $mode in
    check) awaited=${2:?name}; verdict=$(check "$2" "${3:?app id}") ;;
    status) awaited=${3:?name}; verdict=$(status "${2:?prefix}" "$3") ;;
    *) echo "unknown mode: $mode" >&2; exit 2 ;;
  esac
  echo "$(date -u +%H:%M:%S) $verdict"
  case $verdict in
    pass*) exit 0 ;;
    fail*) echo "::error::${verdict#fail }"; exit 1 ;;
  esac
  if ((SECONDS >= deadline)); then
    echo "::error::no verdict for ${awaited} on $SHA after ${DEADLINE_S:-1200} s; did its upload run?"
    exit 1
  fi
  sleep "$poll"
done
