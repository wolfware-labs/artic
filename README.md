# artic

A cross-platform UI framework for Rust. Views are written in XAML, all logic is Rust, and one GPU renderer draws the same pixels on Windows, macOS, Linux, iOS and Android.

## Goals

- Views use XAML syntax (a defined subset of MS-XAML 2012) with CSS-like style selectors.
- The design follows Rust, not a class hierarchy: controls are structs composed from shared state, nodes live in a tree and are referenced by id, and values convert through `FromStr` and `TryFrom`.
- Bindings are typed and checked at compile time. There is no runtime reflection.
- Release builds compile XAML to plain Rust in `build.rs`, so there is no parsing at startup. With `--features hot-reload`, views reload from disk while the app runs.
- Signals propagate every change. View-models and `{Binding}` are built on them.
- `vello` on `wgpu` draws every platform. Layouts adapt to screen size and input mode, not to the OS.
- Every control exposes an `accesskit` node.
- Host, renderer and UI language are each pluggable. XAML is the first language; [the contract](docs/spec/dsl.md) says what another one follows.

## A view

```xml
<UserControl xmlns="https://artic.rs/ui"
             xmlns:x="https://artic.rs/markup"
             x:Class="app::views::login::LoginView"
             x:DataType="app::vm::LoginVm">
  <StackPanel Spacing="8" Margin="16">
    <TextBox Text="{Binding Email, Mode=TwoWay}" Placeholder="Email"/>
    <Button Classes="primary" Content="Sign in" Command="{Binding SignIn}"/>
  </StackPanel>
</UserControl>
```

```rust,ignore
#[derive(ViewModel, Default)]
pub struct LoginVm {
    email: Signal<String>,
}

impl LoginVm {
    #[command]
    async fn sign_in(&self, cx: &VmCx) -> Result<(), AuthError> {
        // ...
    }
}
```

## Project structure

```text
.
├── Cargo.toml            Workspace manifest, shared package metadata and lints
├── rust-toolchain.toml   Pinned Rust toolchain
├── crates/               One directory per crate (see below)
└── docs/
    ├── architecture.md   Crate layers and the dependency graph
    ├── design/           Design notes per feature, such as elements and the builder API
    └── spec/
        └── technical_spec.md
```

## Crates and layers

The workspace is layered. A crate depends only on crates in lower layers, so the core can be built and tested without the controls, and the controls without a platform host.

| Layer | Crate | Responsibility |
| --- | --- | --- |
| 0 | `artic-core` | Signals, effects, `Property<T>`, the node tree and trees, the UI dispatcher |
| 0 | `artic-diagnostics` | Source files, spans and diagnostics for every UI language |
| 0 | `artic-test-fonts` | The font artic's tests shape text with, as byte constants |
| 0 | `artic-ident` | Rust identifier rules the code generators share: case conversions, keywords, and how a name is written as an identifier |
| 1 | `artic-macros` | The derive and attribute macros, such as `#[element]`, `#[derive(UiType)]` and `#[derive(UiEnum)]` |
| 1 | `artic-registry` | Type metadata that lets UI languages name Rust types and properties |
| 1 | `artic-i18n` | Optional localizer backed by Fluent `.ftl` files |
| 1 | `artic-xaml` | XAML parser and markup extension grammar |
| 2 | `artic-layout` | Measure and arrange protocol, geometry and text types, panels |
| 2 | `artic-style` | Selectors, styles, setters, resource dictionaries |
| 2 | `artic-view` | The view every UI language reads its files into, and its checks |
| 2 | `artic-xaml-runtime` | Runtime loader for hot reload and `runtime-xaml` |
| 3 | `artic-render` | `Renderer` trait and retained scene (backends are adapter crates) |
| 3 | `artic-text` | Text shaping and line breaking with `parley`, fonts with `fontique` |
| 3 | `artic-view-compiler` | Build-time compiler from any UI language to Rust |
| 3 | `artic-xaml-lang` | XAML as a UI language, read into the view the compiler takes |
| 4 | `artic-element` | The `Element` trait and its contexts, the property groups every control has (`layout`, `appearance`, `interaction`, `identity`), and the table that finds a node's element |
| 4 | `artic-lsp` | Language server for `.xaml` files |
| 4 | `artic-vello` | Default renderer adapter: `vello` on `wgpu` |
| 5 | `artic-controls` | Templated controls such as `Button`, `TextBox` and `ListBox` |
| 5 | `artic-platform` | The `Host` seam, `artic::run` and the frame pipeline (hosts are adapter crates) |
| 5 | `artic-cli` | The `cargo artic` command |
| 6 | `artic-theme` | Default token-based theme and control templates |
| 6 | `artic-test` | Headless host and test helpers |
| 6 | `artic-winit` | Default host adapter: windows, input and OS services through `winit` |
| 7 | `artic` | Facade crate that apps depend on |

```mermaid
flowchart BT
  L0["Layer 0: artic-core, artic-diagnostics, artic-ident, artic-test-fonts"]
  L1["Layer 1: artic-macros, artic-registry, artic-i18n, artic-xaml"]
  L2["Layer 2: artic-layout, artic-style, artic-view, artic-xaml-runtime"]
  L3["Layer 3: artic-render, artic-text, artic-view-compiler, artic-xaml-lang"]
  L4["Layer 4: artic-element, artic-vello, artic-lsp"]
  L5["Layer 5: artic-controls, artic-platform, artic-cli"]
  L6["Layer 6: artic-theme, artic-test, artic-winit"]
  L7["Layer 7: artic"]
  L0 --> L1 --> L2 --> L3 --> L4 --> L5 --> L6 --> L7
```

[docs/architecture.md](docs/architecture.md) has the exact dependency edges and the rules behind them.

### How a view becomes pixels

1. `artic-xaml` parses a `.xaml` file into an infoset.
2. In a normal build, `artic-xaml-lang` reads the infoset into a view and `artic-view-compiler` turns it into Rust from `build.rs`. With hot reload, `artic-xaml-runtime` builds the same tree at runtime. A conformance test keeps the two paths identical.
3. `artic-core` holds the tree and the signals. Bindings and styles write to properties through one invalidation path.
4. Each frame runs only the dirty stages: style, layout (`artic-layout`), scene building and rendering (`artic-render`), then the accessibility update (`artic-platform`).

## Platforms

| Platform | Minimum |
| --- | --- |
| Windows | 10 |
| macOS | 12 |
| Linux | X11 and Wayland |
| iOS | 15 |
| Android | 8 (API 26) with Vulkan 1.1 |

App-facing APIs are target-neutral, so new targets such as the web can be added without changes to app code.

## Building

The toolchain is pinned in `rust-toolchain.toml`, and `rustup` installs it on first use.

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

## Documentation

- [Technical spec](docs/spec/technical_spec.md): the design of the framework.
- [Architecture](docs/architecture.md): crate layers and dependency rules.
- [Design notes](docs/design/): one per feature, such as [elements](docs/design/element.md), the [builder API](docs/design/builder-api.md) and [backends](docs/design/backends.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, the checks CI runs and the pull request process.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above. Contributors also sign the [Contributor License Agreement](CLA.md), which lets Wolfware distribute their contributions under other licenses too.
