#!/usr/bin/env bash
# Fails when a workflow or action step runs a cargo command without `--locked` (or `--frozen`) after
# the subcommand, so CI always builds the committed Cargo.lock, and when a workflow has no top-level
# `permissions:` block, so every job's token starts read-only whatever the repository setting. It
# fails closed: every subcommand needs the flag except those that never resolve dependencies (fmt,
# `codspeed run`, machete, version, help, and `--version`, `-V` or `--help` alone), and mutants
# takes it as `--cargo-arg=--locked`. A flag after a lone `--` goes to the program cargo runs, and
# one before the subcommand reaches only cargo's built-in commands, so neither counts. Reads every
# YAML file under .github, or the files given, with yq (mikefarah), aliases expanded.
#
# It reads `run:` text a word at a time: `#` after a blank starts a comment, then a backslash at a
# line's end joins the next line; each line, and the text between `;`, `&&`, `||`, `|` and `&`, is a
# command, checked from its first word `cargo`, so a prefix (`VAR=value`, `if`, `timeout 600`, a `(`
# or `$(`) doesn't hide it, and an unquoted `cargo build` in an `echo` counts too. Not read: quotes
# and heredocs (a cargo command built inside one isn't seen) and scripts a step calls.
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
  explode(.) | .. | select(tag == "!!map" and has("run")) | .run | select(tag == "!!str")
  | filename + "\t" + (line | tostring) + "\t" + (. | @base64)
' "${paths[@]}")
status=$?
if [ "$status" -ne 0 ]; then
  echo "yq failed with status $status"
  exit "$status"
fi

# Whether the cargo command in `words`, from the word after `cargo`, may resolve without the
# lockfile.
unlocked() {
  local -a words=("$@")
  local i=0 sub flag
  while [ "$i" -lt "${#words[@]}" ]; do
    case "${words[i]}" in
      -Z | -C | --config | --color) i=$((i + 2)) ;;
      [+-]*) i=$((i + 1)) ;;
      *) break ;;
    esac
  done
  sub=${words[i]:-}
  i=$((i + 1))
  case "$sub" in
    '' | fmt | machete | version | help) return 1 ;;
    codspeed) [ "${words[i]:-}" != run ] || return 1 ;;
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
  text=$(printf '%s\n' "$text" | sed -e 's/\(^\|[[:space:]]\)#.*//' | sed -e ':a' -e '/\\$/{N;s/\\\n/ /;ba' -e '}')
  commands=$(printf '%s\n' "$text" | sed -e 's/&&/\n/g' -e 's/||/\n/g' -e 's/[;|&]/\n/g' -e 's/[(){}`]/ /g')
  while IFS= read -r command; do
    read -r -a words <<<"$command"
    i=0
    while [ "$i" -lt "${#words[@]}" ] && [ "${words[i]}" != cargo ]; do
      i=$((i + 1))
    done
    [ "$i" -lt "${#words[@]}" ] || continue
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
  echo "These cargo commands may resolve dependencies; add \`--locked\` (mutants: \`--cargo-arg=--locked\`):"
  printf '%s\n' "${hits[@]}"
  failed=1
fi
exit "$failed"
