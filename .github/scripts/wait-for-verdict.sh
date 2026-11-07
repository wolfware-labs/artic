#!/usr/bin/env bash
# Waits for an app's check run on a commit and passes or fails with it, so a job can stand in for a
# check the ruleset can't require directly (see ci.yml's header).
#
#   wait-for-verdict.sh <check name> <app id>
#
# Reads REPO (owner/name), SHA and GH_TOKEN; DEADLINE_S (default 1200) and POLL_S (default 20).
# In CI it also reads:
# - RUN_ID and RUN_ATTEMPT: only a check run started after this attempt of the workflow counts, so
#   a rerun waits for its own verdict instead of taking the last attempt's;
# - FORK, "true" for a pull request from a fork, whose upload gets no token, so no verdict can come:
#   it warns and passes;
# - UPSTREAM_JOB and UPSTREAM_RESULT, the job that feeds the app: one that didn't succeed fails here
#   at once instead of at the deadline.
set -euo pipefail

name=${1:?usage: wait-for-verdict.sh <check name> <app id>}
app=${2:?usage: wait-for-verdict.sh <check name> <app id>}

if [ "${FORK:-false}" = true ]; then
  echo "::warning::no $name verdict on a pull request from a fork: its upload gets no token"
  exit 0
fi
if [ -n "${UPSTREAM_RESULT:-}" ] && [ "$UPSTREAM_RESULT" != success ]; then
  echo "::error::$UPSTREAM_JOB was $UPSTREAM_RESULT, so there's no $name verdict to wait for"
  exit 1
fi

since=1970-01-01T00:00:00Z
if [ -n "${RUN_ID:-}" ]; then
  since=$(gh api "repos/$REPO/actions/runs/$RUN_ID/attempts/${RUN_ATTEMPT:-1}" --jq .run_started_at)
fi

deadline=$((SECONDS + ${DEADLINE_S:-1200}))
while :; do
  # The newest matching run started since `since`: "pending" until it's there and completed, then
  # "pass" or "fail" and the reason.
  verdict=$(SINCE=$since gh api -X GET "repos/$REPO/commits/$SHA/check-runs" -f check_name="$name" -f app_id="$app" \
    --jq '[.check_runs[] | select(.started_at >= env.SINCE)] | sort_by(.started_at, .id) | last
      | if . == null or .status != "completed" then "pending"
        elif (.conclusion | IN("success", "neutral", "skipped")) then "pass \(.conclusion)"
        else "fail \(.conclusion): \(.html_url)" end')
  echo "$(date -u +%H:%M:%S) $verdict"
  case $verdict in
    pass*) exit 0 ;;
    fail*) echo "::error::$name: ${verdict#fail }"; exit 1 ;;
  esac
  if ((SECONDS >= deadline)); then
    echo "::error::no $name verdict for $SHA since $since after ${DEADLINE_S:-1200} s; did its upload run?"
    exit 1
  fi
  sleep "${POLL_S:-20}"
done
