#!/usr/bin/env bash
# Fails when a workflow or action step runs a cargo command without `--locked` (or `--frozen`) after
# the subcommand, so CI always builds the committed Cargo.lock, and when a workflow has no top-level
# `permissions:` block, so every job's token starts read-only whatever the repository setting. It
# fails closed: every subcommand needs the flag except those that never resolve dependencies (fmt,
# `codspeed run`, version, help, `--version`, `-V` or `--help` alone, and machete with nothing
# before a lone `--` but plain paths and its switches that read no metadata: `--skip-target-dir`,
# `--fix`, `--no-ignore`, `--version`, `--help`, where a path holds only letters, digits and
# `_ . / : -`; any other option or word may expand to `--with-metadata`, which runs cargo metadata
# and has no `--locked`, so it's flagged), and mutants takes it as `--cargo-arg=--locked`. A flag
# after a lone `--` goes to the program cargo runs, and one before the subcommand reaches only
# cargo's built-in commands, so neither counts. Reads every YAML file under .github, or the files
# given, with yq (mikefarah), aliases expanded.
#
# It reads `run:` text a word at a time: `#` at a word's start outside quotes begins a comment
# (`strip_comments`), then a backslash at a line's end joins the next line; each line, and the text
# between `;`, `&&`, `||`, `|` and `&`, is a command, checked from its first word whose name after
# the last `/` or `\` is `cargo` or `cargo.exe` in any letter case, so a prefix (`VAR=value`,
# `if`, `timeout 600`, a `(` or `$(`) or a path (`~/.cargo/bin/cargo`, `.\tools\cargo`) doesn't
# hide it, and an unquoted `cargo build` in an `echo` counts too. Not read: a cargo command inside
# quotes or a heredoc, cargo called by a quoted path (`"$HOME/.cargo/bin/cargo" build`), and scripts
# a step calls. It reads plain commands, not every shell form: a `#` right after `)`, `;`, `&` or
# `|` isn't taken for a comment, while one after an escaped blank, inside `$'...'` or in `${x# #}`
# is, and an apostrophe in a heredoc body leaves a quote open for the rest of the step. Its job is
# to catch a missing `--locked`; it isn't a shell parser.
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

# Removes shell comments from `run:` text on stdin: a `#` at the start of a line or after a blank,
# outside single and double quotes. A backslash escapes the next character outside quotes and inside
# double quotes, never inside single quotes; a quote stays open across lines.
strip_comments() {
  awk '
    {
      out = ""
      n = length($0)
      for (i = 1; i <= n; i++) {
        c = substr($0, i, 1)
        if (c == "\\" && quote != "\047") {
          out = out c substr($0, i + 1, 1)
          i++
          continue
        }
        if (quote == "") {
          if (c == "#" && (i == 1 || substr($0, i - 1, 1) ~ /[ \t]/)) break
          if (c == "\"" || c == "\047") quote = c
        } else if (c == quote) {
          quote = ""
        }
        out = out c
      }
      print out
    }
  '
}

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
    '' | fmt | version | help) return 1 ;;
    codspeed) [ "${words[i]:-}" != run ] || return 1 ;;
    machete)
      while [ "$i" -lt "${#words[@]}" ] && [ "${words[i]}" != -- ]; do
        case "${words[i]}" in
          --skip-target-dir | --fix | --no-ignore | --version | --help) ;;
          -* | *[!A-Za-z0-9_./:-]*) return 0 ;;
        esac
        i=$((i + 1))
      done
      return 1
      ;;
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
  text=$(printf '%s\n' "$text" | strip_comments | sed -e ':a' -e '/\\$/{N;s/\\\n/ /;ba' -e '}')
  commands=$(printf '%s\n' "$text" | sed -e 's/&&/\n/g' -e 's/||/\n/g' -e 's/[;|&]/\n/g' -e 's/[(){}`]/ /g')
  while IFS= read -r command; do
    read -r -a words <<<"$command"
    i=0
    while [ "$i" -lt "${#words[@]}" ]; do
      case "${words[i]##*[/\\]}" in
        [Cc][Aa][Rr][Gg][Oo] | [Cc][Aa][Rr][Gg][Oo].[Ee][Xx][Ee]) break ;;
      esac
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
