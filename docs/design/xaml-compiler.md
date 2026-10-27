# Design: the XAML compiler

Issue: #49. Builds on: the [builder API](builder-api.md) (#21, #43), [UI languages beside XAML](dsl-neutral.md) (#305), the parser (#23 to #29), the registry (#17), the [linkme spike](../spikes/linkme-registry.md) (#6). Unblocks: #50 attached properties, #51 diagnostics in cargo output, #52 `ui_class`, #94 typed bindings.

The compiler turns each `.xaml` file into plain Rust before the app compiles. The app's `build.rs` calls it; the output calls the builders #43 generates, every name is checked against the registry at build time, and every literal is parsed at build time, so a release app neither looks a name up nor parses text at run time.

## Which types a view can use

The compiler runs inside `build.rs`, a separate program that Cargo builds for the host and runs before the app. Its registry holds the types of the crates it links: its build-dependencies, each referenced once in `build.rs` with `use <crate> as _;`, as the spike found. A type defined in the app crate itself doesn't exist yet when `build.rs` runs.

So a compiled view uses types from build-dependencies only. A name that resolves to nothing gets the unknown-type diagnostic, with a note when the namespace points at the app's own crate: move the type to a library crate and add it to `[build-dependencies]`. The app's crate is the one `Xaml::with_app_crate` names, with `-` read as `_`; `artic::xaml::compile_dir` sets it from `CARGO_PKG_NAME`. A type index read from source (#183) could become a second source of types later; the registry stays the only one here.

Build-dependencies are compiled for the host, with their own features: a type behind a `cfg` the target has and the host lacks isn't in the build script's registry. That is the same limit, stated once in the guide.

## Naming types in generated code

Generated code lives in the app crate, which depends on the `artic` facade, not on `artic_controls` or `artic_layout`. The registry's paths name where an item is defined (`artic_controls::border::Border`), often in a private module, and Rust can't learn where an item is re-exported. So the rule is a convention plus a map:

- A UI type, a value type set from text and a group's methods trait are each reachable at their crate's root, as `<crate>::<Name>`. `artic-controls` and `artic-element` already are; `artic-layout` re-exports its geometry and text values at its root (`Thickness`, `CornerRadius`, `FontFamily`, `TextWrapping`, …). A test in the facade, which links every artic crate, checks against the registry that every one of them is reachable there. A third-party control crate follows the same rule.
- `CratePaths`, in `artic-view-compiler`, maps a crate to the path the app reaches it by. `NamespaceMap` in `artic-xaml` maps URIs to crates (`dsl-neutral.md` §3) and holds no paths:

```rust,ignore
impl CratePaths {
  /// Generated code reaches `krate` as `path`, such as `artic_controls` as `::artic::controls`.
  /// A second call for the same crate replaces the first.
  pub fn path(&mut self, krate: &str, path: &str);
  /// The path for `krate`: the one set with `path`, or `::krate`.
  pub fn path_of(&self, krate: &str) -> String;
  /// Every crate given a path, in sorted order.
  pub fn crates_named(&self) -> impl Iterator<Item = &str>;
  /// Where generated code reaches the item at `rust_path`: its crate's path and the item's name.
  pub fn item(&self, rust_path: &str) -> String;
  /// Where generated code reaches `module`: its crate's path, then the rest of `module`.
  pub fn module(&self, module: &str) -> String;
}
```

- The facade re-exports `artic_controls`, `artic_core`, `artic_element`, `artic_layout` and `artic_render` as `artic::controls`, `artic::core`, `artic::element`, `artic::layout` and `artic::render`, and `artic::view::crate_paths()` sets those paths. `Handle` and `UiTree`, which every generated file names, come from `artic_core::tree` through the paths, as `::artic::core::tree`. An app adds its control crates with `NamespaceMap::alias` for the URI, and sets `CratePaths::path` when its `Cargo.toml` renames the crate.
- A type found through a URI is written as `<crate path>::<Name>`: the crate from the first segment of `TypeInfo::rust_path`, the name from `TypeInfo::name`. A type found through `using:my_crate::views` is written with the module path the document gave, its crate translated by `CratePaths::module`: `<path of my_crate>::views::Gauge`, since the author named that module and `type_by_rust_path` found the type there. A type off the written path fails to compile in the generated file, with the path in the error. The crate that defines a type or a group's methods trait is the one written, so an app maps it with `CratePaths::path` when it reaches that crate through another, such as a control crate that flattens a group from its core crate.

## Literals

Text such as `Padding="4,8"` is parsed in the build script and written out as an expression that constructs the value: `::artic::layout::Thickness::new(4.0_f32, 8.0_f32, 4.0_f32, 8.0_f32)`. A new trait in `artic-core`, below every crate that defines a value type, does the writing:

```rust,ignore
/// A value that can write the Rust expression that builds it, for code generated at build time.
pub trait ToRust {
  /// Writes an expression that evaluates to this value.
  fn to_rust(&self, out: &mut RustExpr);
}

/// The expression being written, and the paths generated code reaches crates by.
pub struct RustExpr { /* text, and a crate -> path lookup */ }

impl RustExpr {
  /// Writes the path of an item at `path` in crate `krate`, such as `Thickness` in
  /// `artic_layout`, as generated code reaches it: `::artic::layout::Thickness`.
  pub fn type_path(&mut self, krate: &str, path: &str);
  /// Writes `value` as a literal that reads back exactly: `4.0_f32`, `-0.0_f32`, `f32::NAN`,
  /// `f32::INFINITY`.
  pub fn f32(&mut self, value: f32);
  /// Writes `text` as an escaped string literal.
  pub fn str(&mut self, text: &str);
  /// Writes Rust source as is, such as `::new(` or `, `.
  pub fn raw(&mut self, source: &str);
}
```

- An impl names its own type with `out.type_path(env!("CARGO_CRATE_NAME"), "Thickness")`: the crate name it was compiled as, and its public path. Standard types are written in full with `raw` (`::core::num::NonZeroU32::new(3).unwrap()`). `RustExpr` never guesses a path from `std::any::type_name`, whose output Rust doesn't fix.
- `artic-core` implements it for the numbers, `bool`, `char`, `String` and the `NonZero` integers. The value types implement it beside their `FromStr`: the geometry types, `Color`, `Brush` and its gradients, `BoxShadow`, `FontFamily`, `FontWeight` and `Classes`, which gain the public constructors their output calls (`FontFamily::from_names`, `FontWeight` as a constant per weight, `Classes::from` a list). `#[derive(UiEnum)]` writes it for every enum (`::artic::layout::Orientation::Horizontal`).
- `PropertyInfo` gains `literal: Option<fn(&str, &mut RustExpr) -> Result<(), String>>`. For a `Property<Option<V>>` it parses and writes `V`, which is what the builder method takes. The derive writes it beside the value check, with the same dispatch: a value type that parses from text gets "parse with `FromStr`, then `to_rust`"; a `NodeId` or a `UiType` value, which never comes from text, gets a function that returns the error "this property takes an element, not text".
- A value type set from text must implement `ToRust`. The value check's bound becomes `FromStr + ToRust`, and its method, whose name is the error message, becomes `value_must_parse_from_text_and_implement_torust_or_derive_uitype`.
- The registry exposes a `literal!` macro to the derive, defined as `Some($f)` when `artic-registry`'s `literals` feature is on and `None` when it's off. `artic-view-compiler` turns the feature on; Cargo's resolver keeps build-dependency features apart from the app's, so only the build script's registry holds the functions, and a release app keeps neither them nor the `ToRust` code they reach.
- `artic-registry` re-exports `ToRust` and `RustExpr` at its root, since `PropertyInfo::literal` names them, so `artic-view-compiler` builds a `RustExpr` through the registry without depending on `artic-core` ([architecture](../architecture.md)).
- `RustExpr` is a string builder, so `artic-core` stays free of `syn`. The compiler parses every expression it gets with `syn::parse_str::<syn::Expr>`; an impl that writes invalid Rust fails the build naming the property, never the app.
- A literal is checked on the host. A `usize` that fits the host and not a 32-bit target fails to compile in the generated file, where rustc reports the literal out of range.

## What a compiled view gives the app

Each `src/views/<name>.xaml` becomes `$OUT_DIR/views/<name>.rs`, at its path under the directory `compile_dir` reads (#52 moves it to its path in the crate, `$OUT_DIR/views/src/views/<name>.rs`), a module body with one constant, one struct and one function:

```rust,ignore
// Generated from src/views/login_view.xaml.
use ::artic::core::tree::{Handle, UiTree};
use ::artic::controls as _;
use ::artic::core as _;
use ::artic::element::LayoutMethods as _;

/// The Rust type `x:Class` names, which `#[ui_class]` checks against its struct.
pub const CODE_TYPE: Option<&str> = None;

/// The nodes of `login_view.xaml`: its root and one handle per named element.
#[derive(Clone, Copy, Debug)]
pub struct View {
  pub root: Handle<::artic::controls::UserControl>,
  pub email: Handle<::artic::controls::TextBox>,
}

/// Builds `login_view.xaml` in `tree` and returns its nodes.
pub fn build(tree: &mut UiTree) -> View {
  let __e0 = ::artic::controls::TextBox::new().placeholder("Email").build(tree);
  let __e1 = ::artic::controls::Button::new().content("Sign in").build(tree);
  let __e2 = ::artic::controls::StackPanel::new()
    .spacing(8.0_f32)
    .margin(::artic::layout::Thickness::uniform(16.0_f32))
    .children([__e0.id(), __e1.id()])
    .build(tree);
  let __e3 = ::artic::controls::UserControl::new().child(__e2).build(tree);
  View { root: __e3, email: __e0 }
}
```

- The sample compiles spec §4's example view, whose root element is a `UserControl`; the compiler adds no wrapper, and `View::root` is typed by whatever the document's root element is, such as `Handle<StackPanel>` for a view rooted in a panel.
- `TextBox` and `Button` aren't built yet; the sample shows the shape once they are.
- One `let` per element, in post-order, named `__e0`, `__e1`, … in that order, so no name in the view can shadow `tree` or another local.
- A property becomes its builder method, named after the field (`PropertyInfo::field`). A group's property uses the same name through the group's methods trait. `PropertyGroupInfo` gains `methods: &'static str`, the trait's defining path from `PropertyGroup::METHODS`. The compiler never writes that path, which may name a private module: it takes the crate (first segment) and the trait's name (last segment), and since the trait sits at its crate's root, imports it once per file as `use <crate path>::<Trait> as _;` with the crate's path from `CratePaths`, such as `use ::artic::element::LayoutMethods as _;`.
- Content comes last, after the property methods, because `.child` and `.children` return a `WithChildren` that has only `build`. `ContentInfo::Collection` gains `method: Option<ContentMethod>`, the method the derive generated (`ContentMethod::Child` or `ContentMethod::Children`) or `None` for a collection of values; a `ContentInfo::Property` content is that property's method, given text or a literal. Text content takes the node's `text`, or its `raw` text for a type marked `whitespace = "significant"`.
- `x:Name` adds a field to `View`, typed by the element, named by the name in snake_case, each run of underscores inside it written as one: `SubmitButton` gives `submit_button`, `URLField` gives `url_field`, `first__name` gives `first_name`. The name is taken as written and its field must be a Rust identifier; a field that is a keyword becomes a raw identifier (`r#type`), and a field `self`, `crate`, `super` or `root` is rejected. Two names with one field, such as `Email` and `email`, are a duplicate, and the error's help names both spellings.
- `CODE_TYPE` is `None` for a view without `x:Class`, as here. With one, it is `Some("<crate>::<path>")` and the root is the code type.
- `x:Class` makes the view's root its code type, and `x:FieldModifier` sets a named node's field visibility ([code type design](ui-class.md)); `x:DataType` is recorded for #94; `x:Uid` is accepted and ignored until localization (#168).
- The file references every crate the namespace map names and every crate `CratePaths` names, with `use <path> as _;`, so their types stay in the app's link (below).

The block above is a view without a code type. The builder design's sample is a view with one: its `build` returns the code type at the root, `View` holds only the named nodes, and `CODE_TYPE` names the type ([code type design](ui-class.md#building-it)).

## What the compiler reports

`compile_str` collects every problem in one pass as `artic-diagnostics` `Diagnostics`, each with its span:

| Input | Diagnostic |
| --- | --- |
| A type no crate of the namespace has, or found in two of them; a `using:` path with no type | unknown or ambiguous type, naming the namespace; the build-dependency note when the namespace points at the app crate |
| A property the type and its groups don't have | unknown property, naming the type |
| A literal its type's `FromStr` refuses | the `FromStr` error, at the attribute |
| Text for a property that takes an element | "this property takes an element, not text" |
| An event, such as `Click="on_click"` | "event handlers come with #82" |
| An attached property | "attached properties come with #50" |
| A markup extension | "`{StaticResource}` comes with #112" (`{Binding}` #94, `{DynamicResource}` #113, `{TemplateBinding}` #85, `{x:Static}`, `{x:Null}`, `{x:Type}` #62); any other: unknown value provider |
| `<x:String>` and the other `x:` primitives | "XAML primitives come with #62" |
| An object value for a property, such as `<Border.Background><SolidColorBrush/></Border.Background>` | "value objects come with #57" |
| `x:Key` outside a resource dictionary | "resources come with #111" |
| Text inside a type with no content, or text in a collection of elements | the type takes no text content |
| A second child for a `Child` content | the type takes one child |
| A collection content of values, such as gradient stops | "value collections come with #57" |
| A value provider written as an element, the root included | the type is a value provider, not an element |
| An element inside a type with no content | the type takes no child elements |
| A property given both as an attribute and as content | the property is set twice |
| A namespace URI the map doesn't declare | no namespace, once per URI, with help to declare it |
| An `x:Name` whose field isn't an identifier or is `self`, `crate`, `super` or `root`, or that gives the same field as another name | invalid or duplicate name; for two spellings of one field, help naming both |

An element in a value position (a property element, a content property, a value collection) is a value, which #57 resolves; until then it's reported as such and its type isn't looked up. Diagnostics are reported in source order.

Rendering them as `file:line:col` lines in `cargo build` output is #51. Here `compile_dir` stops on the first file with errors and returns them; its error's `Display` renders them plainly.

## The API

`artic-view-compiler` compiles the views of any UI language; `Xaml` in `artic-xaml-lang` is the language for `.xaml` files:

```rust,ignore
/// Compiles one document of `language` to the Rust module body for it.
pub fn compile_str(
  sources: &SourceMap,
  file: SourceId,
  language: &(impl Language + ?Sized),
  registry: &Registry,
  paths: &CratePaths,
) -> Result<String, Diagnostics>;

/// Compiles every file of `language` under `dir` into `$OUT_DIR/views/`, for a build script.
pub fn compile_dir(dir: impl AsRef<Path>, language: &(impl Language + ?Sized), paths: &CratePaths) -> Result<(), CompileError>;

/// Compiles every file of `language` under `dir` into `out`, mirroring the layout, and returns the
/// views read, sorted.
pub fn compile_into(
  dir: &Path,
  out: &Path,
  language: &(impl Language + ?Sized),
  registry: &Registry,
  paths: &CratePaths,
) -> Result<Vec<PathBuf>, CompileError>;
```

A build script reaches them through the facade: `artic::view::compile_dir(dir, &xaml, &paths)`, with `xaml` from `artic::xaml::Xaml::new(namespaces)` and `paths` from `artic::view::crate_paths()`. The short form `artic::xaml::compile_dir(dir, &namespaces)` does the same with artic's paths and sets `Xaml::with_app_crate` from `CARGO_PKG_NAME`; a build script using the long form passes `env!("CARGO_PKG_NAME")` itself. `compile_into` is `compile_dir`'s work with every input passed in, for a tool that isn't a build script: it reads no environment and prints nothing for Cargo.

`compile_dir` builds the registry with `Registry::from_link_time()`, reads each file into one `SourceMap` and calls `compile_str`. `CompileError` holds the `SourceMap` and the `Diagnostics`, or an I/O error with its path. It also reports the registry's duplicates and a missing `OUT_DIR`. `compile_str` writes the file's path into the output, so `compile_dir` names each view by its path in the crate, and writes it at that path under `$OUT_DIR/views/` ([code type design](ui-class.md#the-file-argument)). A `dir` outside the crate names its views relative to `dir`.

## Writing the code

`artic-view-compiler` builds `quote!` tokens, parses the file with `syn` and formats it with `prettyplease`. The round trip checks every emitted expression, and the output is deterministic: elements in document order, crates and imports in a `BTreeSet`, files in sorted order, no timestamps and no absolute paths.

## The build script

```rust,ignore
// The app's build.rs. artic (feature "xaml-compiler") and acme_controls are build-dependencies, so the
// compiler's registry has their types, and also normal dependencies, since the generated code compiles
// in the app and names them.
use acme_controls as _;

fn main() {
  let mut namespaces = artic::xaml::namespaces();
  namespaces.alias("https://acme.com/ui", &["acme_controls"]);
  artic::xaml::compile_dir("src/views", &namespaces).unwrap();
}
```

- `compile_dir` walks the directory in sorted order, writes one file per view under `$OUT_DIR/views/` at the view's path in the crate, which `#[ui_class]` finds it by ([code type design](ui-class.md#the-file-argument)), and prints `cargo:rerun-if-changed` for the directory and each file.
- Linking: each generated file references every crate in `namespaces.crates_named()` and `paths.crates_named()` through its path, so a control crate the map names keeps its types in the app's registry even when no view uses them. Every crate the map names is therefore a normal dependency of the app as well as a build-dependency; one that isn't fails to compile in the generated file, at its `use` line. A crate the map doesn't name can't be reached by a view; an app that wants its types for the runtime loader references it itself, as `dsl-neutral.md` §3 says. No `cargo metadata` runs inside the build.
- The facade's `xaml-compiler` feature adds `artic-xaml-lang` and turns on `view-compiler`, which adds `artic-view-compiler` as `artic::view`; apps don't name either crate. A build-dependency on the facade compiles it once more for the host, the usual cost of a code generator.

## Testing

| Where | Tests |
| --- | --- |
| `artic-core` | `ToRust` for each primitive and `NonZero` integer; `RustExpr::f32` writes NaN, the infinities and `-0.0` as constants and other values so they read back exactly; `str` escapes quotes, backslashes and newlines |
| value-type crates | for each value type, text parsed and written with `to_rust` gives an expression that `syn` parses; a trybuild pass case compiles a few and compares them with the parsed values |
| `artic-macros` | a `UiEnum` writes its variant's path; a property whose value type lacks `ToRust` fails with the message above; `literal` is `None` without the feature |
| `artic-xaml` | `NamespaceMap::crates_named` |
| `artic-view-compiler` | `CratePaths::path`, `path_of`, `crates_named`, `item`, `module` |
| `artic` | every UI type, methods trait and property value type in the registry is reachable at its crate's root; every crate defining a UI type or group has a facade path; the facade's paths build a control |
| `artic-xaml-lang` | `insta` snapshots of the generated Rust for every #28 fixture that uses only supported constructs; `output_is_deterministic`; `unknown_element_reports_error`, `unknown_property_reports_error`, `bad_literal_reports_error` and one test per row of the table above; a `syn` walk of each output finding no `Registry`, as #57's `compiled_output_has_no_loader_values` does |
| integration crate | a `build.rs` compiles a `Border` / `StackPanel` / `TextBlock` view; a test builds it and checks the tree and the `View` handles; a control crate the map names and no view uses still has its types in the registry |

## Changes to other documents and issues

| Where | Change |
| --- | --- |
| `builder-api.md` | the sample's literal output and the `View` shape point here; `.watermark` becomes `.placeholder`, the spec's name |
| `value-objects.md` | "how the constant is emitted is #49's design" becomes a `ToRust` constructor |
| `dsl-neutral.md` §3, spec §2 and §5 | `build.rs` uses the facade's `xaml-compiler` feature and `artic::xaml::compile_dir`; the facade features allowed to say XAML gain `xaml-compiler`; `CompileOptions` is dropped |
| #49 | literals become constructors, not `FromStr` calls; linking uses the namespace map, not `cargo metadata` |

## Pull requests

| PR | Contents |
| --- | --- |
| 1 | this document and the changes above |
| 2 | `ToRust` and `RustExpr` in `artic-core`, re-exported by `artic-registry`; the impls and the constructors they call; `UiEnum` writes it; `PropertyInfo::literal`, the `literals` feature and `literal!`; the value check's new bound and message |
| 3 | root re-exports and their tests; `NamespaceMap::path`, `path_of`, `crates_named`; the facade's crate modules and `UiTree` in the prelude; `PropertyGroupInfo::methods`, `ContentInfo::Collection::method` |
| 4 | `compile_str`: resolving, the `View` struct and post-order builder calls, literals, content and collections, the diagnostics above, snapshots and `output_is_deterministic` |
| 5 | `compile_dir`, `rerun-if-changed`, linking through the map, the facade's `xaml-compiler` feature, and the integration crate; closes #49 |

`AttachedPropertyInfo` gets its `literal` with #50, its first user.
