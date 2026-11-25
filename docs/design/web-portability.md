# Design: web portability

Issue: #198. Spec: §1 (the Web/WASM non-goal, D24), §10 (`Host`), §11 (runtime-agnostic contract).

artic has no web host in v1, but a later version must add one without changing app code (D24). Two things keep that door open: the app-facing crates compile for `wasm32-unknown-unknown`, checked by the `Wasm` CI job, and every app-facing API that a browser can't serve as it stands has an issue that owns the gap. This page lists both.

## What the `Wasm` job checks

The job runs `cargo check --target wasm32-unknown-unknown` on each of these crates on its own, since Cargo unifies the features of the crates one command checks and a crate could pass only because another one turned a feature on:

| Crate | Notes |
| --- | --- |
| `artic-core` | |
| `artic-diagnostics` | |
| `artic-ident` | |
| `artic-i18n` | |
| `artic-layout` | |
| `artic-text` | `fontique` has no system font source on wasm; see font loading below. |
| `artic-xaml` | |

A `cargo check` catches a dependency that doesn't compile for the target. It doesn't catch one that compiles and fails at run time: `std::thread::spawn` and `std::time::Instant::now` both compile and fail in a browser. Those are in the audit below. Backend crates that do compile for wasm, `winit` and `wgpu` among them, are kept out of the portable crates by the `cargo deny` bans in `deny.toml`, which the `Deny` job enforces: only `artic-winit` reaches `winit`, directly or through `accesskit_winit`, only `artic-vello` reaches `vello`, and only `vello` depends on the `wgpu` crate. The bans name crates, so a crate that took one of wgpu's own parts (`wgpu-core`, `wgpu-hal`, `wgpu-types`) directly would get past them.

## Not checked yet

| Crate | Why | Owner |
| --- | --- | --- |
| `artic-registry` | `UI_TYPES` is gathered with `linkme`, which has no wasm32 support (`distributed_slice is not implemented for this platform`). | #241 |
| `artic-element` | `ELEMENT_TYPES`, which layout and the runner read at run time to find a node's `Element`, is a `linkme` slice too. | #241 |
| `artic`, `artic-render`, `artic-style`, `artic-controls`, `artic-theme`, `artic-view`, `artic-platform`, `artic-xaml-runtime` | They depend on `artic-registry` or `artic-element`; `artic-render` derives `UiType` on its brushes, which are value types. | #241 |

Once #241 gives both slices a wasm mechanism, these crates join the job.

Native-only by design, and never checked: `artic-winit` and `artic-vello` (backend adapters, [architecture](../architecture.md)), `artic-view-compiler` and `artic-xaml-lang` (build scripts), `artic-macros` (a proc macro, which runs on the build machine), `artic-cli`, `artic-lsp`, and the test and example crates.

## Audit of the app-facing APIs

| API | Status on the web | Owner |
| --- | --- | --- |
| Entry point | Ready. `Host::run` takes a `'static` app and may return at once: a host whose loop can't block installs its callbacks and returns `Ok(())`. | #39 (closed) |
| Frame time | Ready. `AppHandler::frame` gets a `Duration` from an epoch the host picks, never an `Instant`, which panics on `wasm32-unknown-unknown`. | #39 (closed) |
| Font loading | Works with bundled fonts. `Engine::with_fonts` takes font bytes and needs no file system; `Engine::system` finds no fonts in a browser, so a web launcher passes its fonts. | #37 (closed) |
| Clipboard | Not built. The spec makes reading async so a browser can serve it (§12). | #91 |
| File dialogs | Not built. The spec makes them async from v1, for the same reason (§1). | #172 |
| Default `BlockingSpawner` | Not built. A default pool of threads fails on `wasm32-unknown-unknown`, which has none; a web host needs another default. | #80 |
| Timers | Not built. They must read the host's time, as frames do, not `Instant::now`. | #79 |
| File paths in resource loading | Not built. `Image` sources and `ResourceDictionary` sources must load through the host, not `std::fs`, which has no files in a browser. | #71, #111 |
| Type registry and element lookup | Not on wasm: `linkme` (above). | #241 |
