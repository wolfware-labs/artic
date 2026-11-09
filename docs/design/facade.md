# Design: the facade, `artic::App` and `artic::run`

Issue: #55. Builds on: the [host seam](host.md) (#39), the [frame pipeline](frame.md) (#40), [surfaces](surface.md) (#48), [code types](ui-class.md) (#52), the [backend seams](backends.md) (#240). Unblocks: #122 (`Application` resources and styles), #80 (`App::with_blocking`), #110 (the style pass), #141 and #142 (mobile hosts), #166 (`cargo artic new`).

An app depends on one crate, `artic`. Its views and its code import nothing else, and its launcher adds a host and a renderer:

```rust,ignore
// The app crate (a library), the same for every target.
#[artic::ui_class("app.xaml")]
#[derive(Debug)]
pub struct HelloApp;

pub fn app() -> artic::App {
  artic::App::new(HelloApp::create)
}

// The launcher.
fn main() -> Result<(), Box<dyn std::error::Error>> {
  artic::run(artic_winit::Host::new()?, artic_vello::Renderer::new()?, my_app::app())?;
  Ok(())
}
```

## 1. Who sees what

| Who | Writes | Sees |
| --- | --- | --- |
| App code | views, code types, `pub fn app() -> artic::App` | `artic::…`: the modules, the root items (section 4), the prelude |
| Launchers | `artic::run(host, renderer, app)` | `artic::run`, `artic::Error`, a host and a renderer |
| The facade | builds the tree from the `App`, creates the text engine and the `Runner`, registers its frame hooks, calls `artic_platform::run` | `Runner`, `Hooks`, `artic_platform::run`, `artic_text::Engine` |

## 2. `App`

`App` is what an app crate hands its launchers: how to build the app's tree, not the tree itself.

```rust,ignore
impl App {
  /// The app whose tree `build` creates, with an `Application`, or a code type of one, at its root.
  pub fn new<T: ElementKind<Kind = Application>>(build: impl FnOnce(&mut UiTree) -> Handle<T> + 'static) -> App;
}
```

- **`run` builds the tree,** on the thread that runs the loop, just before it starts the host. `app()` is cheap and does nothing, so every launcher can call it, and builder methods that configure the app before it runs, such as #80's `with_blocking`, are added to `App` without changing what `app()` returns.
- **The root is an `Application` by type.** `ElementKind` (section 3) says which element type a type is: itself for every element, its root's type for a code type. `App::new` accepts a closure that builds an `Application` with its builder, a view without a code type through `|tree| app::build(tree).root`, and `HelloApp::create`, and rejects a function returning a `Handle<Border>` at compile time.
- **`Runner::new` no longer checks the type.** It asserts that `application` is an `Application` today, which a code type isn't. The node `run` passes is the application by construction, and the runner's checks on its children (only `Surface`s, none inside another) stay.

## 3. A code type on an `Application`

ui-class.md allowed a code type only on a `UserControl`, and `#[ui_class]` writes `UserControl`'s content, `child: Child`, into every code type. An `Application`'s content is `surfaces: Children`. The macro learns the root's content from the compiled view instead:

```rust,ignore
// $OUT_DIR/views/src/app.rs, for a view with a code type
/// The element type the code type takes the place of.
pub type Root = ::artic::element::Application;
/// The members of `Root` the code type has: its content.
#[allow(dead_code, reason = "only a code type's macro reads it")]
#[derive(Debug)]
pub struct Members {
  /// The root's content, which the code type takes in the root's place.
  pub surfaces: ::artic::element::Children,
}
```

- **The compiler writes both** for a view with a code type. It has the root's `TypeInfo`, whose `content` names the field and its `Child` or `Children` method. It writes the paths through `CratePaths`, as it writes every other path, so `::artic::element` above is the facade's default. A root whose content is a property, or a collection that is neither, can't be a code type's root.
- **`#[ui_class]` parses the compiled file** it already finds in `OUT_DIR`, and adds `Members`' fields to the struct, marked `#[ui(content)]`, in place of the hardcoded `child`. A `UserControl` code type gets `child: Child` as before. Every field it adds, `ui` and each of `Members`' fields, is a reserved name: a struct field with the same name is an error at that field, and a root whose content field is itself named `ui` is an error at the struct.
- **`ElementKind`** is a trait in `artic-element`: `trait ElementKind: Element { type Kind: Element; }`. `#[element]` writes `type Kind = Self`, and `#[ui_class]` writes `type Kind = <module>::Root`. The supertrait keeps a type that isn't an element out of `App::new`. The trait isn't sealed, since both macros write its impls in the app's crate; a hand-written impl that names `Application` is taken at its word, and the runner still checks that the node's children are `Surface`s.
- **`Application` gets `#[ui(code_type_root)]`,** the flag `UserControl` has. The flag's contract is unchanged: the type's behavior is the default `ElementImpl`, since the code type's own `impl ElementImpl` is empty.

`HelloApp` is then an element with `Application`'s members, at the root of the tree, and the runner presents its children as it presents an `Application`'s.

## 4. What the facade exports

The crates stay modules, as they are today: `controls`, `core`, `element`, `layout` and `render`, which compiled views name. Nothing is glob-re-exported. The root adds an explicit list of what an app's own code writes:

| Item | From | For |
| --- | --- | --- |
| `App`, `run`, `Error` | the facade; `Error` is `artic_platform::Error` | the app's entry and its launchers |
| `ui_class` | `artic-macros` | code types |
| `Signal`, `Memo`, `Property`, `effect` | `artic-core` | a code type's state |
| `Handle`, `UiTree`, `NodeId` | `artic-core` | reaching nodes |
| `element`, `Element`, `ElementImpl`, `ElementKind` | `artic-macros`, `artic-element` | an app's own elements |
| `UiType`, `UiEnum` | the derives from `artic-macros`; `UiType`'s trait from `artic-registry` | `UiType` names the derive and the trait it implements at one path, as serde's `Serialize` does; `UiEnum` is a derive only |

`artic-platform` and `artic-text` become dependencies of the facade but not modules of it: an app sees neither. Host adapters depend on `artic-platform` directly, and renderer adapters on `artic-render`.

**The prelude holds the traits whose methods a view's code calls,** as `std::prelude` holds mostly traits: the property groups' builder methods (`AppearanceMethods`, `IdentityMethods`, `InteractionMethods`, `LayoutMethods`, `PanelMethods`), with `Handle` and `UiTree`, which every builder call names. Types an app names, such as `Signal` or `App`, are imported by name from the root, so a glob import of the prelude never shadows an app's own `Signal` or `App`.

## 5. `run`

```rust,ignore
pub fn run<H: Host, R: Renderer + 'static>(host: H, renderer: R, app: App) -> Result<(), Error>;
```

1. Builds the tree with the app's `build`.
2. `Runner::new(tree, renderer, Box::new(artic_text::Engine::system()), root)`.
3. Registers the facade's frame hooks on the runner's `Hooks`, in one private function, `hooks::register(&mut Runner<R>)`. It registers none yet; #110's style pass is the first.
4. `artic_platform::run(host, runner)`, whose errors `run` returns unchanged: `Error::Unsupported` before the host starts when the renderer can't draw on its targets, then the first frame's `Error::Render` or the host's `Error::Host`.

`run` is generic over the host and the renderer (D24): `App` and `run` name no backend type.

## 6. `examples/hello`

A multi-file example in `crates/artic-examples`, so it compiles against the facade's public API and nothing else:

| File | Holds |
| --- | --- |
| `examples/hello/main.rs` | `fn app() -> artic::App` and the launcher `main` |
| `examples/hello/app.xaml`, `app.rs` | `HelloApp`: an `Application` with one `Surface` titled "Hello", holding a `Border` around a `StackPanel` of `TextBlock`s |
| `build.rs` | `artic::xaml::compile_dir("examples/hello", &artic::xaml::namespaces())` |

- `artic-examples` gains `artic` as a dependency and, with `xaml-compiler`, a build-dependency. `hello` imports only `artic` and, in `main`, the two adapters and the `Report` every example's `main` returns.
- The surface's content is written inline, not as a `UserControl` view of its own, since a code type can't be an element of another view yet (ui-class.md, "Not in this design").
- `hello_view_tree_shape` is a `#[test]` in the example, run by `cargo test -p artic-examples` with `test = true` on its `[[example]]`: it builds `HelloApp` into a tree with no window and checks the element types down the tree.
- `check-ui-neutral.sh` allows `crates/artic-examples/examples/hello/` and the build script's line: an app is free to use XAML.
- CI builds `hello` on the three desktops, as it builds every example, and runs it on macOS with `ARTIC_HELLO_SECONDS=3`: `main` then starts a thread that ends the process with code 0 after that long, and a host or renderer error before then ends `main` with that error. Windows' runners have only WARP, on which vello refuses to start, so `hello` can't run there; `frames` gets past that with a renderer that draws nothing, which `hello` would have to name, and it names only `artic-vello`. Ubuntu's runners have no display.

Changing the title at runtime needs app code that runs after the tree is built (event handlers, #82, or a view-model, #94), so that criterion moves to #82. The runner's title reconcile is covered by its own tests and by `frames`.

## 7. Tests

| Test | Checks |
| --- | --- |
| `run_returns_unsupported` | a renderer that accepts nothing: `Err(Error::Unsupported)` before the host's `run` is called |
| `run_returns_the_host_error` | a fake host whose loop fails: `Err(Error::Host)` with its error |
| `run_returns_ok` | a fake host that starts, draws a frame and ends: `Ok(())`, and the frame drew the app's surface |
| `app_accepts_a_code_type` | `App::new(HelloApp::create)`, through the fixture app, runs like a plain `Application` |
| the facade's tests | rewritten on `artic::…` paths only, except those that check the crates' roots and the registry, and `run`'s fake host, which implements `artic_platform`'s traits as a host adapter does |
| `ui_class`'s tests | a code type on an `Application` gets `surfaces: Children`; a struct field named `surfaces` is an error there |
| the compiler's tests | `Root` and `Members` in the snapshots of views with a code type; an `Application` root with a code type compiles |

## Not in this design

- `App::with_blocking` (#80), `Application.Resources` and `Styles` (#122), theme loading (#116).
- A host or renderer chosen by the facade: launchers name them ([backend design](backends.md)).
- Registering OS services (clipboard, IME, cursor) as app services, once their traits exist.

## Changes to other documents and issues

| Where | Change |
| --- | --- |
| Spec §2 | the `artic-platform` row points here; with PR 2, the `artic` row lists `App` and `run` as present |
| `architecture.md` | with PR 2, the facade's dependencies gain `artic-platform`, `artic-text` and `artic-registry`, and the facade may depend on a crate to run the app as well as to re-export it; with PR 4, `hello` is one of `artic-examples`' examples |
| `ui-class.md` | a code type's root is a type with `code_type_root`, `UserControl` or `Application`; the compiled file's `Root` and `Members`; "`App` opening its first window from a code type" points here |
| `surface.md` | the sample's `Application` can have an `x:Class`; the facade row points here |
| `host.md`, `frame.md` | the facade rows point here |
| `view-pipeline.md`, `xaml-compiler.md` | a view with a code type gets `Root` and `Members`, and its root's content must be `Child` or `Children` |
| Doc comments in code | with PR 2, `artic-platform`'s `host.rs` and `artic-winit`'s crate docs name `artic::run` as current |
| #55 | the runtime title change moves to #82; CI runs `hello` on macOS only |
| #82 | gains #55's runtime title criterion |

## Pull requests

| PR | Contents |
| --- | --- |
| 1 | this document and the changes above that aren't marked for a later PR |
| 2 | `ElementKind`, written by `#[element]`; the root re-exports, the prelude, `App`, `run` and its hooks function, `Runner::new` without the type check, the facade's tests on `artic::` paths |
| 3 | `Root` and `Members` in compiled views, `#[ui_class]` reading them and writing `ElementKind`, `code_type_root` on `Application` |
| 4 | `examples/hello`, `hello_view_tree_shape` and CI; closes #55 |
