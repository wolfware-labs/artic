#!/usr/bin/env bash
# Fails when a crate below the XAML layer names XAML or writes one of its `x:` directives, such as
# `x:Name`. The XAML crates, the language server, the CLI, the facade's XAML feature and
# docs-check's list of every document are allowed to.
set -uo pipefail
cd "$(dirname "$0")/.."
allowed=(':!crates/artic-xaml/' ':!crates/artic-xaml-compiler/' ':!crates/artic-xaml-runtime/'
  ':!crates/artic-lsp/' ':!crates/artic-cli/' ':!crates/artic/src/xaml.rs' ':!crates/artic/Cargo.toml'
  ':!crates/artic/src/lib.rs' ':!crates/artic/tests/xaml.rs' ':!crates/docs-check/src/lib.rs')
names=$(git grep -n -i 'xaml' -- 'crates/' "${allowed[@]}")
names_status=$?
directives=$(git grep -n -E '(^|[^A-Za-z0-9_])x:[A-Z]' -- 'crates/' "${allowed[@]}")
directives_status=$?
for status in "$names_status" "$directives_status"; do
  if [ "$status" -gt 1 ]; then
    echo "git grep failed with status $status"
    exit "$status"
  fi
done
hits=$(printf '%s\n%s' "$names" "$directives" | sed '/^$/d')
if [ -n "$hits" ]; then
  echo "These lines name XAML or write its directives below the XAML layer:"
  echo "$hits"
  exit 1
fi
