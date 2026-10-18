# Spike: linkme registry across targets

Issue: #6. Time box: 2 days.

The registry (§3 of the spec) is a `linkme` distributed slice that each crate fills through `#[derive(UiType)]`. Linkers can drop data that no code references, so this spike checks whether entries from several crates survive LTO, dead-code stripping, an iOS static library and an Android shared library.

## Setup

The code is in [`spikes/linkme-registry/`](../../spikes/linkme-registry/), a separate Cargo workspace outside the artic workspace.

| Crate | Role |
| --- | --- |
| `spike-registry` | Defines `ENTRIES: [Entry]` with `#[distributed_slice]` and adds one entry of its own |
| `spike-entries-a`, `spike-entries-b` | Each adds two entries, the way a control crate adds `TypeInfo` entries |
| `spike-app` | Desktop binaries. `linked` names both entry crates with `extern crate`, `use_underscore` with `use name as _;`, and `unreferenced` only lists them in `Cargo.toml` |
| `spike-ffi` | `staticlib` and `cdylib` exposing `spike_report()` to the C harnesses in `harness/` |

Every configuration should see 5 entries. Profiles: `dev`, `release`, and three release variants with `lto = "fat"`, `lto = "thin"` and `codegen-units = 1`.

| Tool | Version |
| --- | --- |
| Rust | 1.98.1 (`rust-toolchain.toml`) |
| `linkme` | 0.3.37 |
| Xcode | 26.6 (build 17F113), iOS simulator on `macos-latest` (arm64) |
| Android NDK | r29 (29.0.14206865), API level 26 |
| Android emulator | API 26, x86_64, on `ubuntu-latest` |
| CI run | [35950488249](https://github.com/wolfware-labs/artic/actions/runs/35950488249) |

## Results

### Desktop

| OS | Profile | `extern crate` | `use name as _;` | Not referenced |
| --- | --- | --- | --- | --- |
| Windows | dev | 5/5 | 5/5 | 1/5 |
| Windows | release | 5/5 | 5/5 | 1/5 |
| Windows | release + `lto = "fat"` | 5/5 | 5/5 | 1/5 |
| Windows | release + `lto = "thin"` | 5/5 | 5/5 | 1/5 |
| Windows | release + `codegen-units = 1` | 5/5 | 5/5 | 1/5 |
| macOS | dev | 5/5 | 5/5 | 1/5 |
| macOS | release | 5/5 | 5/5 | 1/5 |
| macOS | release + `lto = "fat"` | 5/5 | 5/5 | 1/5 |
| macOS | release + `lto = "thin"` | 5/5 | 5/5 | 1/5 |
| macOS | release + `codegen-units = 1` | 5/5 | 5/5 | 1/5 |
| Linux | dev | 5/5 | 5/5 | 1/5 |
| Linux | release | 5/5 | 5/5 | 1/5 |
| Linux | release + `lto = "fat"` | 5/5 | 5/5 | 1/5 |
| Linux | release + `lto = "thin"` | 5/5 | 5/5 | 1/5 |
| Linux | release + `codegen-units = 1` | 5/5 | 5/5 | 1/5 |

### iOS simulator, static library

The C harness links `libspike_ffi.a` with `xcrun clang -target arm64-apple-ios15.0-simulator` and runs in a booted simulator with `simctl spawn`. The linker flags are the ones an Xcode project passes.

| Profile | Plain link | `-dead_strip` | `-force_load` | `-force_load` + `-dead_strip` |
| --- | --- | --- | --- | --- |
| dev | 1/5 | 1/5 | 5/5 | 5/5 |
| release | 1/5 | 1/5 | 5/5 | 5/5 |
| release + `lto = "fat"` | 5/5 | 5/5 | 5/5 | 5/5 |
| release + `lto = "thin"` | 1/5 | 1/5 | 5/5 | 5/5 |
| release + `codegen-units = 1` | 1/5 | 1/5 | 5/5 | 5/5 |

### Android, shared library

Section check: the size of the `linkme_ENTRIES` section in `libspike_ffi.so` divided by the 32-byte entry size. Runtime: a native executable built with the NDK loads the library with `dlopen` on the emulator and calls `spike_report()`.

| Profile | `aarch64-linux-android` section check | `x86_64-linux-android` section check | x86_64 emulator, `dlopen` at runtime |
| --- | --- | --- | --- |
| dev | 5/5 | 5/5 | 5/5 |
| release | 5/5 | 5/5 | 5/5 |
| release + `lto = "fat"` | 5/5 | 5/5 | 5/5 |
| release + `lto = "thin"` | 5/5 | 5/5 | 5/5 |
| release + `codegen-units = 1` | 5/5 | 5/5 | 5/5 |

## Findings

1. `linkme` itself holds up. Whenever a crate is part of the link, its entries survive fat LTO, thin LTO, `codegen-units = 1`, `--gc-sections` on Android and `-dead_strip` on Apple targets. The entries are emitted with the `no_dead_strip` attribute on Mach-O and stay in the `linkme_ENTRIES` section on ELF.

2. rustc does not link a crate that nothing references. The `unreferenced` binary sees only the registry's own entry on every OS and every profile. The entry crates are dropped before the linker runs, so no linker flag can recover them. Naming the crate once is enough to keep it in the link, even when nothing else from it is used. `use name as _;` and `extern crate name;` give the same result in every profile. `use name as _;` is the form to prefer. Ordinary dependencies have not needed `extern crate` since the 2018 edition.

3. A static library loses entries at the final link. `ld` only loads archive members that resolve a symbol it needs, and nothing references the objects that hold the entries. The `-why_load` output and the link map confirm that those members are never loaded. Fat LTO avoids this by merging all crates into one object, so everything is loaded together. `-force_load` (or `-all_load`) loads every member and keeps all entries, including with `-dead_strip`.

   The archive must appear only once. When it is also passed as a plain input before `-force_load`, `ld` handles the plain input first and ignores the `-force_load`. An early run of this spike made that mistake and showed `-force_load` failing. `diag-ios.sh` has the link map and the load strategies that pinned it down.

4. The Android `cdylib` needs nothing extra. rustc drives that link itself and passes the crates so their sections are kept.

## Recommendation

Keep `linkme` (D11). No fallback mechanism is needed, but two rules must hold, and both can be enforced without adding a registration call for library authors (§3 rule 5).

1. Every crate that contributes registry entries must be linked.
   - The `artic` facade's `xaml` module references artic's control crates, so built-in controls are present in every app that uses it.
   - For app and third-party crates, the XAML compiler's `build.rs` (#49) finds every dependency that depends on `artic-registry` through `cargo metadata` and emits a `use name as _;` line for each into the generated code. Library authors still write only the derive.
   - Cost: a `cargo metadata` call in the build script, and crates that register types but are never used stay in the binary. Only crates that depend on the registry are affected.
   - Without this, a control that only appears in hot-reloaded or `runtime-xaml` views would be missing from the registry, and the loader would report it as an unknown type.

2. iOS static libraries must be linked with `-force_load`.
   - The Xcode shell that `cargo artic ios` generates (#144) passes `-force_load` for the Rust library in `OTHER_LDFLAGS` for every configuration, and does not list the library a second time.
   - Cost: every object in the archive is loaded. `-dead_strip` still removes unreferenced code, so the size impact should be small. Measure it when #144 lands.
   - Fat LTO alone would also work, but only for release builds, and debug builds would still lose entries.

Changes to fold into existing issues once this is accepted:

- #17: document the two rules next to the slice definition.
- #49: generate the `use name as _;` lines for registry dependencies.
- #144: pass `-force_load` for the Rust static library, once.
- #141: confirm the same result on a physical device build (this spike used the simulator).

## Limits

- iOS was tested on the simulator only. Device builds use the same linker, so the same behavior is expected.
- The `aarch64-linux-android` library was checked by section size, not run. GitHub-hosted runners cannot run an arm64 emulator. The x86_64 library, built and linked the same way, was run on an emulator.

## Reproducing

```sh
cd spikes/linkme-registry
./run-desktop.sh           # current OS
./run-ios.sh               # macOS with Xcode
./diag-ios.sh              # after run-ios.sh
./run-android-build.sh     # needs ANDROID_NDK_HOME
./run-android-device.sh    # needs a running x86_64 emulator
```

`.github/workflows/spike-linkme.yml` runs all of them on every change to the spike.
