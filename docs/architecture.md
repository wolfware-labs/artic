# Architecture

artic is a Cargo workspace of layered crates. This document is the reference for which crate may depend on which. The design behind each crate is in the [technical spec](spec/technical_spec.md).

## Seams

An app picks three things. Each sits behind an interface no layer below it knows the other side of.

| Seam | Interface | v1 |
| --- | --- | --- |
| Host | `Host` in `artic-platform` | `artic-winit` |
| Renderer | `Renderer` in `artic-render` | `artic-vello` |
| UI language | `Document` and `Language` in `artic-view`, over the registry and the tree, as [the contract](spec/dsl.md) states | XAML: `artic-xaml`, `artic-xaml-lang`, `artic-xaml-runtime` |

No crate below a UI language depends on it or names it. Only the XAML crates (`artic-xaml`, `artic-xaml-lang`, `artic-xaml-runtime`), `artic-lsp`, `artic-cli`, the facade's `xaml` module and `docs-check`'s list of documents say XAML or write its `x:` directives; `scripts/check-ui-neutral.sh` checks this in CI.

## Layers

A crate's layer is its depth: one more than the deepest artic crate it depends on, planned dependencies included, and 0 for a crate with none. So a crate depends only on crates in lower layers, and crates in the same layer never depend on each other.

| Layer | Crates | Role |
| --- | --- | --- |
| 0 | `artic-core`, `artic-diagnostics`, `artic-ident`, `artic-test-fonts` | Foundations with no artic dependencies: the reactive core and tree, spans and diagnostics, Rust identifier rules, and the test font |
| 1 | `artic-macros`, `artic-registry`, `artic-i18n`, `artic-xaml` | The proc macros, type metadata, localization, the XAML parser |
| 2 | `artic-layout`, `artic-style`, `artic-view`, `artic-xaml-runtime` | Geometry, layout and text values; styling; the view every UI language reads into, and its checks; the XAML loader |
| 3 | `artic-render`, `artic-text`, `artic-view-compiler`, `artic-xaml-lang` | The `Renderer` seam and scene; text shaping and fonts; the compiler every UI language uses; XAML as a UI language |
| 4 | `artic-element`, `artic-vello`, `artic-lsp` | The `Element` trait, its contexts and the property groups every control has; the default renderer adapter; the language server |
| 5 | `artic-controls`, `artic-platform`, `artic-cli` | Controls, the `Host` seam and frame pipeline, and the CLI |
| 6 | `artic-theme`, `artic-test`, `artic-winit` | The default theme, test utilities, and the default host adapter |
| 7 | `artic` | The facade that apps depend on |
| outside | `artic-examples`, `artic-registry-fixture-a`, `artic-registry-fixture-b`, `artic-registry-linking`, `artic-xaml-compiler-fixture-app`, `artic-xaml-compiler-fixture-controls`, `docs-check` | Test-only packages, `publish = false`: the examples, the fixtures and docs-check |

## Dependency graph

Arrows point from a crate to its dependency. A dotted arrow is optional (named by its feature) or planned.

```mermaid
flowchart TD
  artic -. planned .-> theme[artic-theme]
  artic --> controls[artic-controls]
  artic -. planned .-> platform[artic-platform]
  artic -. planned .-> text[artic-text]
  artic -. planned: xaml-runtime / xaml-hot-reload .-> runtime[artic-xaml-runtime]
  artic -. xaml .-> xaml
  artic -. view-compiler .-> viewc[artic-view-compiler]
  artic -. xaml-compiler .-> lang

  theme --> controls
  theme --> styles[artic-style]

  controls --> element[artic-element]
  controls --> styles

  platform --> element

  element --> render[artic-render]
  element --> registry[artic-registry]
  element --> macros[artic-macros]

  test[artic-test] --> platform
  test -. planned .-> styles
  test --> text
  test --> fonts[artic-test-fonts]

  winit[artic-winit] --> platform
  vello[artic-vello] --> render

  cli[artic-cli] -. planned .-> lsp[artic-lsp]
  lsp --> lang[artic-xaml-lang]

  render --> layout[artic-layout]
  text --> layout
  styles --> registry
  viewc --> view[artic-view]
  lang --> view
  view --> registry
  view --> diagnostics
  lang --> xaml[artic-xaml]
  xaml --> diagnostics[artic-diagnostics]
  runtime --> registry
  runtime --> xaml

  registry --> core[artic-core]
  layout --> core
  layout --> macros
  macros --> ident[artic-ident]
  xaml --> ident
  view --> ident
  i18n[artic-i18n] --> core
```

The graph omits edges that a lower edge already implies (for example `artic-controls -> artic-core`). The full list of direct dependencies:

| Crate | Depends on | Dev-dependencies |
| --- | --- | --- |
| `artic-core` | none | none |
| `artic-macros` | `artic-ident` | `artic-core`, `artic-element`, `artic-registry` |
| `artic-diagnostics` | none | none |
| `artic-ident` | none | none |
| `artic-xaml` | `artic-diagnostics`, `artic-ident` | none |
| `artic-test-fonts` | none | none |
| `artic-registry` | `artic-core` | `artic-macros` |
| `artic-layout` | `artic-core`, `artic-macros` | none |
| `artic-i18n` | `artic-core` | none |
| `artic-render` | `artic-core`, `artic-layout` | none |
| `artic-text` | `artic-layout` | `artic-test-fonts` |
| `artic-style` | `artic-core`, `artic-registry` | none |
| `artic-view` | `artic-registry`, `artic-diagnostics`, `artic-ident` | `artic-core` |
| `artic-view-compiler` | `artic-view`, `artic-registry`, `artic-diagnostics` | `artic-core` |
| `artic-xaml-lang` | `artic-xaml`, `artic-view`, `artic-registry`, `artic-diagnostics` | `artic`, `artic-core`, `artic-macros`, `artic-view-compiler` |
| `artic-xaml-runtime` | `artic-xaml`, `artic-registry`, `artic-core` | none |
| `artic-element` | `artic-core`, `artic-registry`, `artic-layout`, `artic-render`, `artic-macros` | `artic-text` |
| `artic-controls` | `artic-core`, `artic-registry`, `artic-layout`, `artic-render`, `artic-element`, `artic-style`, `artic-macros` | `artic-test` |
| `artic-platform` | `artic-core`, `artic-layout`, `artic-render`, `artic-element` | `artic-test-fonts`, `artic-text` |
| `artic-cli` | `artic-xaml` (planned #197), `artic-xaml-lang` (planned #197), `artic-lsp` (planned #197) | none |
| `artic-lsp` | `artic-xaml`, `artic-xaml-lang` | none |
| `artic-theme` | `artic-controls`, `artic-style` | none |
| `artic-test` | `artic-core`, `artic-layout`, `artic-render`, `artic-text`, `artic-test-fonts`, `artic-element`, `artic-platform`, `artic-registry` (planned), `artic-style` (planned) | none |
| `artic-winit` | `artic-render`, `artic-platform` | none |
| `artic-vello` | `artic-layout`, `artic-render` | `artic-controls`, `artic-core`, `artic-element`, `artic-test`, `artic-test-fonts`, `artic-text` |
| `artic` | `artic-core`, `artic-element`, `artic-layout`, `artic-render`, `artic-controls`, `artic-platform` (planned #55), `artic-text` (planned #55), `artic-style` (planned #106), `artic-theme` (planned #106), `artic-xaml` (feature `xaml`), `artic-view-compiler` (feature `view-compiler`), `artic-xaml-lang` (feature `xaml-compiler`), `artic-xaml-runtime` (planned #66, feature `xaml-runtime`) | `artic-macros`, `artic-registry` |

## Rules

- `artic-controls` does not depend on `artic-platform`. Input event types live in `artic-core`, and the facade will connect the platform host to the tree (#55). Controls stay testable without a window and portable to future targets.
- The `Element` trait and its contexts (`RenderCx`, `HitTestCx`, `EventCx`, `AccessCx`) live in `artic-element`, together with gesture recognizers. `Application` and `Surface`, the roots the host presents, live there too, with the `Desktop` attached properties, so `artic-platform` reads them directly ([surface design](design/surface.md)). Controls implement `Element`; the platform host drives any `dyn Element` without knowing the concrete controls. The property groups every control has (`Layout`, `Appearance`, `Interaction`, `Identity`) live there too, so a control library outside artic doesn't need `artic-controls` and its built-in controls; `#[element]` stays with the macros.
- `artic-layout` defines `Layoutable`, `MeasureCx`, `ArrangeCx` and `LayoutManager` over `Layoutable`; `Element` extends `Layoutable`, so layout never names `Element`. Text measurement reaches layout through `TextEngine`, a trait defined in `artic-layout` and implemented by `artic-text` with `parley`, the same for every renderer. The shaped `Paragraph` is an `artic-layout` type too, so the scene carries it and renderers draw it without depending on `artic-text` ([text design](design/text.md)).
- `artic-platform` owns the frame pipeline (dispatcher, style pass, layout, scene build, render, accesskit). Stages that need crates the platform cannot see, such as the style pass or OS theme → root pseudo-classes, will be registered as hooks by the facade (#55). The platform never depends on `artic-style`, `artic-controls` or `artic-theme`.
- Tree behaviours that call `Element` (routed-event dispatch, hit testing, focus navigation) live in `artic-element`; `artic-core` keeps only the data types they use.
- OS services that controls use (clipboard, IME requests, cursor) are traits in `artic-core`. Hosts provide them as capabilities on `HostContext` ([host design](design/host.md)), the facade will register them (#55), and controls reach them through their context or app services, never through the `Host`.
- Code that needs concrete controls but serves the runtime or tooling (hot-reload overlay and state restore, debug overlay) lives in the facade.
- Test data that a crate below `artic-test` needs too, such as the test font, lives in a data-only crate with no dependencies (`artic-test-fonts`), not in `artic-test`, so the crate's tests can use it without a cycle.
- The crates under `crates/artic-registry/fixtures/` are test-only (`publish = false`) and sit outside the layers. Two register two UI types each (`Alpha` or `Beta`, and `Twin`); `artic-registry-linking` holds the tests that check which of them reach the registry.
- `crates/artic-examples` is test-only (`publish = false`) and sits outside the layers: its examples (`frames`, `smoke`, `primitives`) run artic through the `winit` host and the `vello` renderer, which no crate in the layers may depend on. `examples/hello` joins it with #55.
- The crates under `crates/artic-xaml-lang/fixtures/` are test-only (`publish = false`) and sit outside the layers: `app` compiles its views from its build script through the facade, and `controls` is a control crate its map names and no view uses.
- `artic-macros` is a proc-macro crate that depends only on `artic-ident`. The code it generates names `artic::__private`, `artic_registry::__private` or, for `UiEnum`, `artic_core::__private`, whichever the calling crate depends on.
- Dev-dependencies follow the layers too, except the edges in the table below.
- Rust identifier rules (snake, screaming snake and Pascal case, keywords, what an identifier is, and how a name is written as one) live in `artic-ident`, below every crate that writes Rust from a name, so each converts and checks a name the same way.
- Spans and diagnostics live in `artic-diagnostics`, below every UI language, so `artic-style` can report selector errors with spans without naming one.
- `crates/docs-check` is test-only (`publish = false`) and sits outside the layers, like the registry fixtures. It compiles the Rust code in the documents.
- `artic-xaml` does not depend on the registry. Type references stay unresolved in the infoset, so parser tests need no registry.
- Release builds will link `artic-xaml-runtime` only when the app enables `xaml-runtime`; `xaml-hot-reload` is for development builds. Compiled views never touch the loader.
- The facade's default features add no UI language, host or renderer. Each one comes with an opt-in feature named after it, such as `xaml`, so adding a language or a renderer never costs an app that doesn't use it. The facade depends on a crate only to re-export it.
- App-facing crates stay free of native-only dependencies, so a web target can be added later without app changes (D24 in the spec).
- Backend adapters (`artic-winit`, `artic-vello`, and any third-party adapter) depend only on the public API of `artic-render` and `artic-platform`. Nothing in artic depends on an adapter: each target's launcher names its host and renderer ([backend design](design/backends.md)). `artic-render` and `artic-platform` have no backend dependencies (`wgpu`, `vello`, `winit` and the OS crates live in adapters). Its library depends only on `artic-render` and `artic-platform`. Examples that need both a host and a renderer live in `artic-examples`, so no adapter depends on another.

### Dev-dependencies against the layers

| Crate | Dev-depends on | Why |
| --- | --- | --- |
| any crate | `artic-test` | Tests use its headless layout, scenes and test text engine, as long as `artic-test` doesn't depend back on the crate. Control tests use it today; theme and language-server tests will, with `TestHost` (#101). |
| `artic-macros` | `artic-element`, `artic-registry` | Its own tests use crates that use the macros. Cargo allows the loop: `artic-macros` itself depends only on `artic-ident`. |
| `artic-registry` | `artic-macros` | Its tests derive the types they register. |
| `artic-vello` | `artic-controls`, `artic-element` | Control goldens belong to each renderer ([golden images](design/golden-images.md)). |
| `artic-xaml-lang` | `artic`, `artic-view-compiler` | Its snapshots pin the facade's real paths (`xaml`, `view-compiler`). Cargo allows the loops: `artic-xaml-lang` depends on neither, and the facade depends on it only with `xaml-compiler`. |

## Why this differs from the original §2 diagram

The first version of the spec drew `artic-core -> artic-layout -> artic-controls -> artic-style/platform/render`. Read with the stated arrow direction, that makes the core depend on layout and layout depend on controls, which contradicts "each layer depends only on the ones below it". The graph above is the corrected version, and the spec now points here.

## Checking the graph

```sh
cargo test -p docs-check --test layers
```

The test reads `cargo metadata` and the tables above, and fails on an edge one side has that the other lacks, a crate whose layer isn't its depth, a dev-dependency against the layers that the exceptions table doesn't list, a crate in the layers that depends on an adapter, and a facade dependency it doesn't re-export or that adds a seam's crate by default. It also checks the README's table of crates and layers. `cargo deny check bans` keeps `winit`, `vello` and `wgpu` in their adapters. A pull request that adds an edge updates this document in the same change.
