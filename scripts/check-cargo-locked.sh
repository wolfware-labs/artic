#!/usr/bin/env bash
# Fails when a workflow or action step runs a cargo command that resolves dependencies without
# `--locked` (or `--frozen`), so CI always builds the committed Cargo.lock, and when a workflow has no
# top-level `permissions:` block, so every job's token starts read-only whatever the repository
# setting. The commands: build, test, run, clippy, check, doc, llvm-cov, deny, `codspeed build`, and
# mutants, which takes it as `--cargo-arg=--locked`. A flag after a lone `--` goes to the program
# cargo runs, not to cargo, so it doesn't count. Reads every YAML file under .github, or the files
# given, with yq (mikefarah).
#
# It reads `run:` text a word at a time: a command starts a line or follows `;`, `&&`, `||` or `|`,
# after any `VAR=value` words and `do`, `then`, `else`, `time` or `!`; a backslash at a line's end
# joins the next line, and `#` after a blank starts a comment. Quotes and heredocs aren't parsed, so a
# cargo command built inside a quoted string or a heredoc isn't seen.
set -uo pipefail
set -f
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

# A workflow is a file with `jobs`; a composite action has none and no token of its own.
unscoped=$(yq eval --header-preprocess=false --no-doc '
  select(tag == "!!map" and has("jobs") and (has("permissions") | not))
  | filename + ": no top-level permissions block"
' "${paths[@]}")
status=$?
if [ "$status" -ne 0 ]; then
  echo "yq failed with status $status"
  exit "$status"
fi

# One line per `run:` string, steps and action inputs alike: file, line and the text in base64.
runs=$(yq eval --header-preprocess=false --no-doc '
  .. | select(tag == "!!map" and has("run")) | .run | select(tag == "!!str")
  | filename + "\t" + (line | tostring) + "\t" + (. | @base64)
' "${paths[@]}")
status=$?
if [ "$status" -ne 0 ]; then
  echo "yq failed with status $status"
  exit "$status"
fi

# Whether the cargo command in `words`, from the word after `cargo`, resolves without the lockfile.
unlocked() {
  local -a words=("$@")
  local i=0 sub flag
  while [ "$i" -lt "${#words[@]}" ] && [[ ${words[i]} == [+-]* ]]; do
    i=$((i + 1))
  done
  sub=${words[i]:-}
  i=$((i + 1))
  case "$sub" in
    build | test | run | clippy | check | doc | llvm-cov | deny) ;;
    codspeed) [ "${words[i]:-}" = build ] || return 1 ;;
    mutants)
      while [ "$i" -lt "${#words[@]}" ] && [ "${words[i]}" != -- ]; do
        case "${words[i]}" in
          --cargo-arg=--locked | --cargo-arg=--frozen | -C--locked | -C--frozen) return 1 ;;
          --cargo-arg | -C)
            flag=${words[i + 1]:-}
            [ "$flag" = --locked ] || [ "$flag" = --frozen ] && return 1
            ;;
        esac
        i=$((i + 1))
      done
      return 0
      ;;
    *) return 1 ;;
  esac
  while [ "$i" -lt "${#words[@]}" ] && [ "${words[i]}" != -- ]; do
    case "${words[i]}" in
      --locked | --frozen) return 1 ;;
    esac
    i=$((i + 1))
  done
  return 0
}

hits=()
while IFS=$'\t' read -r file line encoded; do
  [ -n "$file" ] || continue
  text=$(printf '%s' "$encoded" | base64 -d)
  text=$(printf '%s\n' "$text" | sed -e ':a' -e '/\\$/{N;s/\\\n/ /;ba' -e '}' -e 's/\(^\|[[:space:]]\)#.*//')
  commands=$(printf '%s\n' "$text" | sed -e 's/&&/\n/g' -e 's/||/\n/g' -e 's/;/\n/g' -e 's/|/\n/g')
  while IFS= read -r command; do
    read -r -a words <<<"$command"
    i=0
    while [ "$i" -lt "${#words[@]}" ]; do
      case "${words[i]}" in
        do | then | else | time | '!') ;;
        *) [[ ${words[i]} =~ ^[A-Za-z_][A-Za-z0-9_]*= ]] || break ;;
      esac
      i=$((i + 1))
    done
    [ "${words[i]:-}" = cargo ] || continue
    if unlocked "${words[@]:i+1}"; then
      hits+=("$file:$line: $(printf '%s' "${words[*]:i}")")
    fi
  done <<<"$commands"
done <<<"$runs"

failed=0
if [ -n "$unscoped" ]; then
  echo "$unscoped"
  failed=1
fi
if [ "${#hits[@]}" -gt 0 ]; then
  echo "These cargo commands resolve dependencies; add \`--locked\` (mutants: \`--cargo-arg=--locked\`):"
  printf '%s\n' "${hits[@]}"
  failed=1
fi
exit "$failed"
