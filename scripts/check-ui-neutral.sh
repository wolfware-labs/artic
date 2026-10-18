#!/usr/bin/env bash
# Fails when a crate below the XAML layer names XAML. The XAML crates, the language server, the
# CLI and the facade's XAML feature are allowed to.
set -euo pipefail
cd "$(dirname "$0")/.."
hits=$(git grep -n -i 'xaml' -- 'crates/' \
  ':!crates/artic-xaml/' ':!crates/artic-xaml-compiler/' ':!crates/artic-xaml-runtime/' \
  ':!crates/artic-lsp/' ':!crates/artic-cli/' ':!crates/artic/src/xaml.rs' ':!crates/artic/Cargo.toml' \
  ':!crates/artic/src/lib.rs' || true)
if [ -n "$hits" ]; then
  echo "These lines name XAML below the XAML layer:"
  echo "$hits"
  exit 1
fi
