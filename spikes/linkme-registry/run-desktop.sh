#!/usr/bin/env bash
# Builds and runs both app binaries under every profile on the current OS.
# Prints one markdown table row per configuration.
set -euo pipefail
cd "$(dirname "$0")"

os="$(uname -s)"
case "$os" in
  MINGW*|MSYS*|CYGWIN*) os="Windows" ;;
  Darwin) os="macOS" ;;
esac

for profile in dev release fat thin cgu1; do
  for bin in linked use_underscore unreferenced; do
    out="$(cargo run -q --locked --profile "$profile" --bin "$bin")"
    n="$(sed -E 's/^entries=([0-9]+).*/\1/' <<<"$out")"
    result="pass"; [ "$n" = "5" ] || result="FAIL"
    echo "| $os | $profile | $bin | $n/5 | $result |"
  done
done
