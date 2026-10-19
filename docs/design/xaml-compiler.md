# Design: the XAML compiler

Issue: #49. Builds on: the [builder API](builder-api.md) (#21, #43), [UI languages beside XAML](dsl-neutral.md) (#305), the parser (#23 to #29), the registry (#17), the [linkme spike](../spikes/linkme-registry.md) (#6). Unblocks: #50 attached properties, #51 diagnostics in cargo output, #52 `ui_class`, #94 typed bindings.

The compiler turns each `.xaml` file into plain Rust before the app compiles. The app's `build.rs` calls it; the output calls the builders #43 generates, every name is checked against the registry at build time, and every literal is parsed at build time, so a release app neither looks a name up nor parses text at run time.

## Which types a view can use

The compiler runs inside `build.rs`, a separate program that Cargo builds and runs before the app. Its registry holds the types of the crates it links: its build-dependencies, each referenced once in `build.rs` with `use <crate> as _;` as the spike found. A type defined in the app crate itself doesn't exist yet when `build.rs` runs.

So a compiled view uses types from build-dependencies only. A name that resolves to nothing gets the usual unknown-type diagnostic, with a note when the document's namespace points at the app's own crate: move the type to a library crate and add it to `[build-dependencies]`. A type index read from source (#183) could become a second source of types later; the registry stays the only one here.

## Naming types in generated code

Generated code lives in the app crate, which depends on the `artic` facade, not on `artic_controls` or `artic_layout`, and the registry's paths name where an item is defined (`artic_controls::border::Border`), often in a private module. Rust has no way to learn where an item is re-exported, so the rule is a convention plus a map:

- A UI type, a value type parsed from text and a group's methods trait are each reachable at their crate's root, as `<crate>::<Name>`. Every artic crate re-exports its public items there already.
- `NamespaceMap`, which maps URIs to crates (`dsl-neutral.md` §3), also maps a crate to the Rust path the app reaches it by:

```rust,ignore
impl NamespaceMap {
  /// Generated code reaches `krate` as `path`, such as `artic_controls` as `::artic::controls`.
  pub fn path(&mut self, krate: &str, path: &str);
  /// The path for `krate`: the one set with `path`, or `::krate`.
  pub fn path_of(&self, krate: &str) -> String;
}
```

- The facade re-exports its crates as modules (`artic::controls`, `artic::element`, `artic::layout`, `artic::render`, `artic::core`), and `artic::xaml::namespaces()` sets their paths beside artic's URI. An app adds its control crates the same way: `alias` for the URI, `path` if the app reaches the crate by another name.
- A crate's name comes from the first segment of `TypeInfo::rust_path`, `PropertyGroup::METHODS` or `std::any::type_name`. A type that isn't at its crate's root fails to compile in the generated file, with the path in the error.

## Literals

Text such as `Padding="4,8"` is parsed in the build script and written out as an expression that constructs the value: `::artic::layout::Thickness::new(4.0, 8.0, 4.0, 8.0)`. A new trait in `artic-core`, below every crate that defines a value type, does the writing:

```rust,ignore
/// A value that can write the Rust expression that builds it, for code generated at build time.
pub trait ToRust {
  /// Writes an expression that evaluates to this value.
  fn to_rust(&self, out: &mut RustExpr);
}

/// The expression being written, with the paths generated code reaches crates by.
pub struct RustExpr { /* text, and a crate -> path lookup */ }

impl RustExpr {
  /// Writes the path of `T` as generated code names it: its crate's path, then its name.
  pub fn type_path<T: ?Sized>(&mut self);
  /// Writes `value` as a literal that reads back exactly, such as `4.0_f32`.
  pub fn f32(&mut self, value: f32);
  /// Writes `text` as an escaped string literal.
  pub fn str(&mut self, text: &str);
  /// Writes Rust source as is, such as `::new(` or `, `.
  pub fn raw(&mut self, source: &str);
}
```

- `artic-core` implements it for the numbers, `bool`, `char`, `String` and `Option<T>`. The value types implement it beside their `FromStr`: the geometry types, `Color`, `Brush` and its gradients, `BoxShadow`, the font types and `Classes`. `#[derive(UiEnum)]` writes it for every enum (`::artic::layout::Orientation::Horizontal`).
- `PropertyInfo` and `AttachedPropertyInfo` gain `literal: fn(&str, &mut RustExpr) -> Result<(), String>`, which the derive writes as "parse with `FromStr`, then `to_rust`". It runs in the build script, so a bad literal fails the build with the attribute's span and the `FromStr` error.
- A value type set from text must implement `ToRust`: the derive's value check, which already asks for `FromStr` or `UiType`, says "implement `ToRust` (or derive `UiEnum`) so views can be compiled". Values that never come from text don't need it: a `NodeId` property such as `DataContext`, or a value object built as an element (#57).
- `RustExpr` is a string builder, so `artic-core` stays free of `syn`. The compiler parses every expression it gets with `syn::parse_str::<syn::Expr>`; an impl that writes invalid Rust fails the build naming the property, never the app.

## What a compiled view gives the app

Each `views/<name>.xaml` becomes `$OUT_DIR/views/<name>.rs`, a module body with one struct and one function:

```rust,ignore
// Generated from views/login_view.xaml.

/// The nodes of `login_view.xaml`: its root and one handle per `x:Name`.
#[derive(Clone, Copy, Debug)]
pub struct View {
  pub root: Handle<::artic::controls::UserControl>,
  pub email: Handle<::artic::controls::TextBox>,
}

/// Builds `login_view.xaml` in `tree` and returns its nodes.
pub fn build(tree: &mut UiTree) -> View {
  let email = ::artic::controls::TextBox::new().watermark("Email").build(tree);
  let sign_in = ::artic::controls::Button::new().content("Sign in").build(tree);
  let panel = ::artic::controls::StackPanel::new()
    .spacing(8.0_f32)
    .margin(::artic::layout::Thickness::uniform(16.0_f32))
    .children([email.id(), sign_in.id()])
    .build(tree);
  let root = ::artic::controls::UserControl::new().child(panel).build(tree);
  View { root, email }
}
```

- `TextBox` and `Button` aren't built yet; the sample shows the shape once they are.
- One `let` per element, in post-order: children are built before the element that holds them, as the builder design says.
- A property becomes its builder method, named after the field (`PropertyInfo::field`). A group's property uses the same name through the group's methods trait, imported once per file with `use <path>::<Trait> as _;`, the trait found through `PropertyGroup::METHODS`; `PropertyGroupInfo` gains a `methods` field holding it.
- Content: a `ContentInfo::Property` content is that property's method; a `Child` or `Children` content is `.child(…)` or `.children([…])`, given the handles built before. Text content takes the node's `text`, or its `raw` text for a type marked `whitespace = "significant"`.
- `x:Name` adds a field to `View`, typed by the element. `x:Class` and `x:FieldModifier` are recorded for #52, which includes the file in the code-behind module, calls `build` and keeps the `View`; `x:Uid` is accepted and ignored until localization (#168).
- The file also references every crate that adds registry entries (below), so their types stay in the app's link.

The builder design's sample, `impl LoginView { fn build_view(&mut self, tree) }`, becomes this `View` and `build` pair; `#[ui_class]` (#52) wraps it.

## What the compiler reports

`compile_str` collects every problem in one pass as `artic-xaml` `Diagnostics`, each with its span:

| Input | Diagnostic |
| --- | --- |
| A type no crate of the namespace has, or found in two of them | unknown or ambiguous type, naming the namespace; the note about build-dependencies when the namespace points at the app crate |
| A property the type and its groups don't have | unknown property, naming the type |
| A literal its type's `FromStr` refuses | the `FromStr` error, at the attribute |
| An attached property | "attached properties come with #50" |
| A markup extension | "`{StaticResource}` comes with #112" (`{Binding}` #94, `{DynamicResource}` #113, `{TemplateBinding}` #85, `{x:Static}`, `{x:Null}`, `{x:Type}` #62); any other: unknown value provider |
| An object value for a property, such as `<Border.Background><SolidColorBrush/></Border.Background>` | "value objects come with #57" |
| A second `x:Name` with one name, or one that clashes with `root` | duplicate name |

Rendering them as `file:line:col` lines in `cargo build` output is #51; here `compile_dir` returns them and its error's `Display` renders them plainly.

## Writing the code

The compiler builds `quote!` tokens, parses the file with `syn` and formats it with `prettyplease`. The round trip checks every emitted expression, and the output is deterministic: elements in document order, crates and imports in a `BTreeSet`, no timestamps and no absolute paths, so the same input gives the same bytes on every run.

## The build script

```rust,ignore
// The app's build.rs, with artic (feature "xaml-compiler") and acme_controls as build-dependencies.
use acme_controls as _;

fn main() {
  let mut namespaces = artic::xaml::namespaces();
  namespaces.alias("https://acme.com/ui", &["acme_controls"]);
  artic::xaml::compile_dir("src/views", &namespaces).unwrap();
}
```

- `compile_dir` walks the directory in sorted order, writes one file per view under `$OUT_DIR/views/` mirroring the source layout, and prints `cargo:rerun-if-changed` for the directory and each file.
- It runs `cargo metadata` through the `CARGO` and `CARGO_MANIFEST_DIR` variables and lists the app's dependencies that depend on `artic-registry`; each generated file references them with `use <name> as _;`, using the name the app's `Cargo.toml` gives each one, so a control crate no view names still has its types in the app's registry.
- The facade's `xaml-compiler` feature adds `artic-xaml-compiler` and re-exports `compile_dir`; apps don't depend on the compiler crate directly.

## Testing

| Where | Tests |
| --- | --- |
| `artic-core` | `ToRust` for each primitive and `Option`; `RustExpr` writes floats that read back exactly and escaped strings |
| value-type crates | for each value type, `"text"` parsed and written with `to_rust` gives an expression that `syn` parses and that evaluates to the same value (a trybuild pass case compiles a few) |
| `artic-macros` | a `UiEnum` writes its variant's path; a property whose value type lacks `ToRust` fails with the message above |
| `artic-xaml` | `NamespaceMap::path` and `path_of` |
| `artic-xaml-compiler` | `insta` snapshots of the generated Rust for every #28 fixture that uses only supported constructs; `output_is_deterministic`; `unknown_element_reports_error`, `unknown_property_reports_error`, `bad_literal_reports_error`; the generated text never mentions `Registry` |
| integration crate | a `build.rs` compiles a `Border` / `StackPanel` / `TextBlock` view, a test builds it and checks the tree and the `View` handles; a control crate that no view names still has its types in the registry |

## Pull requests

| PR | Contents |
| --- | --- |
| 1 | this document, and the builder design's literal output and `View` shape |
| 2 | `ToRust` and `RustExpr` in `artic-core`; the impls; `UiEnum` writes it; `PropertyInfo::literal` and `AttachedPropertyInfo::literal`; the derive's `ToRust` message |
| 3 | `NamespaceMap::path`; the facade's crate modules and paths in `artic::xaml::namespaces()`; `PropertyGroupInfo::methods` |
| 4 | `compile_str`: resolving, the `View` struct and post-order builder calls, literals, content and collections, the diagnostics above, snapshots and `output_is_deterministic` |
| 5 | `compile_dir`, `rerun-if-changed`, `cargo metadata` and `use … as _;`, the facade's `xaml-compiler` feature, and the integration crate; closes #49 |
