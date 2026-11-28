#!/usr/bin/env bash
# Builds libspike_ffi.so for Android under every profile and counts the linkme
# section entries in each library. Requires the Android NDK (ANDROID_NDK_HOME
# or ANDROID_NDK_LATEST_HOME). Prints one markdown table row each.
set -euo pipefail
cd "$(dirname "$0")"
ndk="${ANDROID_NDK_HOME:-${ANDROID_NDK_LATEST_HOME:?NDK not found}}"
bin="$(echo "$ndk"/toolchains/llvm/prebuilt/*/bin)"
api=26
entry_size=32   # two &'static str fields on a 64-bit target

mkdir -p out
for target in aarch64-linux-android x86_64-linux-android; do
  rustup target add "$target" >/dev/null
  env_target="$(tr 'a-z-' 'A-Z_' <<<"$target")"
  export "CARGO_TARGET_${env_target}_LINKER=$bin/${target}${api}-clang"
  for profile in dev release fat thin cgu1; do
    cargo build -q --locked -p spike-ffi --target "$target" --profile "$profile"
    dir="$profile"; [ "$profile" = dev ] && dir=debug
    so="target/$target/$dir/libspike_ffi.so"
    cp "$so" "out/$target-$profile.so"
    # Section header columns after the name: Type Address Off Size ...
    hex="$("$bin/llvm-readelf" -S -W "$so" | awk '{ for (i = 1; i <= NF; i++) if ($i == "linkme_ENTRIES") print $(i + 4) }')"
    if [ -z "$hex" ]; then
      echo "no linkme_ENTRIES section in $so; linkme sections:" >&2
      "$bin/llvm-readelf" -S -W "$so" | grep -i linkme >&2 || true
      hex=0
    fi
    n=$(( 16#$hex / entry_size ))
    result="pass"; [ "$n" = "5" ] || result="FAIL"
    echo "| Android $target (cdylib, section check) | $profile | gc-sections | $n/5 | $result |"
  done
done
"$bin/x86_64-linux-android${api}-clang" harness/dlopen_main.c -o out/dlopen-x86_64
