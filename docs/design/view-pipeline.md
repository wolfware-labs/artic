# Design: the view pipeline beside every UI language

Issue: #325. Builds on: the [XAML compiler](xaml-compiler.md) (#49), [UI languages beside XAML](dsl-neutral.md) (#305), the [language contract](../spec/dsl.md). Unblocks: #52's compiler changes ([code type design](ui-class.md)), #51 diagnostics in cargo output, #59 the runtime loader.

The compiler #49 built sits in the XAML crates, but most of it isn't XAML: diagnostics with source spans, checking a view's members against the registry, writing Rust, and driving a build script would be the same for any language. This design moves those parts into three crates no language owns. A language then writes one front end, from its syntax to a plain `Document`, and gets the compiler and, with #59, the loader from it. The XAML crates keep XAML's syntax and meaning only.

Apart from a name's field, now in snake_case, which changes the `names.xaml` snapshot's fields and drops `#[allow(non_snake_case)]` from every compiled snapshot, and the help a duplicate name gets when two spellings give one field ([What changes for an app](#what-changes-for-an-app)), the generated code and every diagnostic stay the same: every other snapshot line and the fixture app stay unchanged, except the render snapshot of `artic-diagnostics`, whose test now uses a neutral file name and message; the header insta writes above a moved snapshot, which names its source file, follows the move. PR 4 changes three lines of every compiled file: the tree types come from `::artic::core::tree` in place of `::artic::prelude`, the file links `::artic::core` beside the other crates, and the `View` doc says "one handle per named element" in place of "per `x:Name`". Three differences touch only a misconfigured path, [below](#writing-rust): the help of "isn't a Rust path" names `CratePaths::path` in place of `NamespaceMap::path`, which goes; the element with such a path has its members checked; and a value provider written as an element with such a path is reported as a value provider. No snapshot has any of them; the one test of the message checks only its text, not its help. One difference touches names: a name is taken as written, so spaces or a comment around it make it invalid, where `syn`'s parser dropped them from the field, and `gen` gives `r#gen`. A few names an app's build script can reach move; [What changes for an app](#what-changes-for-an-app) lists them.

## The pipeline

```text
view.xaml
  │ artic-xaml: parse → infoset
  │ artic-xaml-lang: Xaml, the front end: walk the infoset, resolve URIs and using:, read x: directives
  ▼
Document                      artic-view: elements with their types looked up, members as written
  │ artic-view: check
  ▼
CheckedView                   artic-view: every member found, content placed, names checked
  ├─ artic-view-compiler: emit → Rust; compile_dir drives a build script
  └─ the runtime loader (#59): build → UiTree
```

| Crate | Holds | Depends on |
| --- | --- | --- |
| `artic-diagnostics` | `Span`, `SourceId`, `SourceMap`, `SourceFile`, `LineCol`, `Diagnostic`, `Diagnostics`, `Severity`, `Label`, `render`, moved from `artic-xaml` | none |
| `artic-view` | `Document`, `check`, `CheckedView`, the `Language` trait, the field-name rule | `artic-registry`, `artic-diagnostics`, `artic-ident` |
| `artic-view-compiler` | `emit`, literals, `CratePaths`, `compile_str`, `compile_dir`, `CompileError` | `artic-view`, `artic-registry` (with `literals`), `artic-diagnostics`, `syn`, `quote`, `proc-macro2`, `prettyplease`, `thiserror` |
| `artic-xaml` | the parser, the infoset, `NamespaceMap` (URIs only) | `artic-diagnostics`, `artic-ident` |
| `artic-xaml-lang` (was the XAML compiler crate) | `Xaml`, the front end | `artic-xaml`, `artic-view`, `artic-registry`, `artic-diagnostics` |

`artic-diagnostics` has no dependencies, so `artic-style`'s selector errors and the parser use spans without the registry. `artic-view` has no `syn`, so the loader doesn't carry a code generator at run time. `artic-xaml` stays free of the registry, as `docs/architecture.md` requires, so its tests still need no registry. `artic-view-compiler` turns on the registry's `literals` feature, which the XAML compiler crate turned on before.

The moved code keeps its behavior; its own tests and docs, which use XAML file names and messages as examples, are rewritten with neutral ones, so `scripts/check-ui-neutral.sh` passes over the new crates. The render snapshot's test uses a neutral file name and message, so its text changes with its file name and `source:` header; every other snapshot keeps its content.

## The document

A front end turns its syntax into a `Document`: plain data, the type of every element it holds looked up, every member still as written, in the language's member order. A type that isn't found becomes a `Problem` the checker reports only if it reaches that element, so an element inside a collection of values reports nothing, as today, and an element written as a property's value isn't held at all until #57. It's what a test builds by hand.

Today's compiler reports a problem only where it looks: a property's value is read only once the type and the property are found, and an element where its parent takes a value, such as inside a `TextBlock`, is never looked up at all ([what the compiler reports](xaml-compiler.md#what-the-compiler-reports)). A front end can't know those rules without copying them, so it records each problem where it occurs, as a `Problem`, and the checker reports the problems it reaches, in the order it reaches them. A problem it doesn't reach is dropped, as today.

```rust,ignore
/// A view as its language wrote it: types looked up, members not yet checked.
#[derive(Debug, Clone)]
pub struct Document {
  /// The view's root element.
  pub root: Element,
}

/// An element of the view.
#[derive(Debug, Clone)]
pub struct Element {
  /// The element's type, or what the checker reports if it reaches the element.
  pub ty: Result<TypeUse, Problem>,
  /// Where the element is written.
  pub span: Span,
  /// Its members, in the language's member order: for XAML, attributes, then property elements,
  /// then the content.
  pub members: Vec<Member>,
}

/// A type an element uses, and where the document reached it.
#[derive(Debug, Clone)]
pub struct TypeUse {
  /// The type's descriptor.
  pub info: &'static TypeInfo,
  /// The module the document named the type in, such as `artic_controls::border`, or `None` when
  /// the type is reached at its crate's root.
  pub module: Option<String>,
  /// Where the type's name is written.
  pub span: Span,
}

/// One member of an element.
#[derive(Debug, Clone)]
pub enum Member {
  /// A property set to text: `name` as written, the member's `span`, and the value or what the
  /// checker reports once it finds the property.
  Property { name: String, span: Span, value: Result<Text, Problem> },
  /// The name the view's code reaches the element by.
  Name(Result<Text, Problem>),
  /// What is written directly inside the element, every node kept, blank text included.
  Content { span: Span, nodes: Vec<Content> },
  /// A member the document can't hold yet, reported whenever the checker reaches the element.
  Unsupported(Problem),
}

/// Something written directly inside an element.
#[derive(Debug, Clone)]
pub enum Content {
  /// Text: `text` after the language's whitespace rules, `raw` as written. Blank text has an
  /// empty `text`.
  Text { text: String, raw: String, span: Span },
  /// A child element.
  Element(Element),
  /// A node the document can't hold yet, reported if the checker reads the content as text.
  Unsupported(Problem),
}

/// Text with the span it was written at.
#[derive(Debug, Clone)]
pub struct Text {
  pub text: String,
  pub span: Span,
}

/// A problem a front end found, which the checker reports if it reaches it.
#[derive(Debug, Clone)]
pub struct Problem {
  pub diagnostic: Diagnostic,
  /// A key reported only once per view, at the first use the checker reaches, prefixed by its
  /// kind, such as `namespace:` and the URI of an undeclared namespace.
  pub once: Option<String>,
}
```

- `Property::span` is where the member is named: an attribute's name, or a property element's start tag. An unknown property is underlined there. `value`'s span is that of the value's only node, or the member's when the value is several nodes or none; an empty property element sets `""`.
- `Content::span` runs from the first node to the last, the span a content property's text is set at.
- `module` keeps the module a document named, as XAML's `using:` does, so generated code names the type there. Without one, the type is reached at its crate's root, the [convention](xaml-compiler.md#naming-types-in-generated-code) every artic crate follows.
- `Content::Text` carries both forms because a type whose whitespace is significant (`Whitespace::Significant`) reads `raw`; the rules that make `text` are the language's own.
- The document holds only what artic can build today. A construct a later issue adds is an `Unsupported` problem until that issue adds it: attached properties (#50), an element as a property's value (#57), value providers (#94, #112), resource keys (#111). The code type and field visibility come with #52 and the data type with #94, each as a member of the document, so the loader (#59) gets them from the same document as the compiler.
- The types are `artic_view::document::Element` and the like; code that also uses `artic_element::Element` names the module.

## A language

```rust,ignore
/// A UI language: what reads its files into documents.
pub trait Language {
  /// The extension of its files, such as `xaml`.
  fn extension(&self) -> &str;

  /// Reads `file` into a document, looking each type up in `registry`. Returns `None` when the
  /// file can't be read as a document, with at least one diagnostic saying why.
  fn read(&self, sources: &SourceMap, file: SourceId, registry: &Registry) -> (Option<Document>, Diagnostics);

  /// The crates its configuration names, for XAML the crates its URIs stand for, which generated
  /// code references so the app links them. A crate a view reaches only by its Rust path, as
  /// XAML's `using:` does, is linked by the app itself, as today.
  fn crates(&self) -> Vec<&str>;
}
```

`read` returns a partial document, not a `Result`, so a view with an unknown type and a property set twice reports both in one build. Its own diagnostics are for problems that don't depend on where the checker looks; XAML has none once it has parsed, so it returns `(Some(document), Diagnostics::new())` or the parse errors. `Language` takes `&self` and returns owned values, so the CLI can pick a `&dyn Language` by a file's extension. The compiler takes any `Language`, and the loader will too; XAML's is `Xaml::new(namespaces)`.

## Checking

```rust,ignore
/// Checks `document`'s members: each property against its type's descriptor, the content against
/// the type's content rule, and the names.
pub fn check(document: &Document) -> (CheckedView, Diagnostics);

/// The field a name gives in a view's `View`: the name in snake_case, a keyword as a raw
/// identifier, such as `r#type` for `Type`; or why it can't be one.
pub fn field_name(name: &str) -> Result<String, &'static str>;
```

The checker is `lower.rs`'s member half, unchanged in behavior, and needs no registry: each `TypeInfo` it holds answers for its own properties. It walks the document in member order, children where their content member is, and, for each element it reaches:

1. reports the type's problem and goes on: properties are checked only when the type was found; names and `Unsupported` members always; the elements in the content always when the type wasn't found, and as step 4 says when it was;
2. for a property, finds it by name; an unknown name is reported (an event's name gives "event handlers come with #82") and ends that member, its value and "set twice" never checked; for a found property, reports the value's problem if it has one, and otherwise a property set twice;
3. for a name, reports its problem, then a name `field_name` refuses, with the reason as help, then two elements with one name, so a name that's invalid twice is reported twice;
4. for content, applies the type's rule: text for a content property, where the first element or `Unsupported` node is reported and ends the content; the text sets the property as a `Property` member would, so a property set by both is reported as set twice; children for `child` or `children`, where text is reported with the help "put the text in an element, such as a `TextBlock`", every child element counts toward "takes one child" whether its type was found or not, and the child taken is the only found one; nothing for a type that takes none, where text and elements are reported and the elements still checked; and, for a collection of values, "value collections come with #57" when it holds anything but blank text, the elements inside it never reached;
5. reports each `Unsupported` member.

An element whose type isn't found still has its name checked and the elements in its content checked, so an unknown type doesn't hide the problems below it. `field_name` is `names.rs`'s rule without `syn`, built on `artic-ident`: an identifier by Unicode's XID rules, not `_`, a keyword written as a raw identifier by `rust_identifier`, and `self`, `Self`, `crate`, `super`, `root` and a leading `r#` refused with today's three reasons. It converts the name to snake_case first (a word starts at a capital after a lowercase letter or a digit, and at the last capital of an acronym, and a run of underscores inside the name becomes one), and checks the field, so `Root` is refused and `Type` gives `r#type`; two names with one field are two elements with one name, with help naming both spellings. It takes the name as written, so `" email "` is refused where `syn` skipped the spaces. Its keyword list is checked in its tests against `syn`, except `gen`, reserved since Rust 2024, which `syn` still takes as an identifier: `gen` becomes `r#gen`, so the field compiles on any edition, where the `syn` rule gave `gen`. Emission takes each field from it.

`CheckedView` is today's view model without the Rust in it:

```rust,ignore
/// A view whose members are all found, ready for a back end.
#[derive(Debug, Clone)]
pub struct CheckedView {
  /// Every element whose type was found, children before their parent.
  pub elements: Vec<CheckedElement>,
  /// The root's index, or `None` when its type wasn't found.
  pub root: Option<usize>,
  /// Where the root element is written, for problems with the whole file.
  pub span: Span,
  /// Each valid name, its field and its element's index, in the order the elements are built.
  pub names: Vec<CheckedName>,
}

/// A valid name: what the view writes and the field a back end gives the element.
#[derive(Debug, Clone)]
pub struct CheckedName {
  /// The name as the view writes it.
  pub name: Text,
  /// The field it gives, unique in the view.
  pub field: String,
  /// The named element's index.
  pub element: usize,
}

/// An element ready to build: its type, its properties and its children.
#[derive(Debug, Clone)]
pub struct CheckedElement {
  pub ty: TypeUse,
  /// Each property with the text it's set to, in member order.
  pub properties: Vec<(PropertyRef, Text)>,
  pub content: CheckedContent,
}

/// The children an element's content method takes.
#[derive(Debug, Clone)]
pub enum CheckedContent {
  None,
  Child(usize),
  Children(Vec<usize>),
}
```

Values stay text. The compiler writes each one as Rust at build time, and the loader will set it with the property's text thunk, so `CheckedView` holds no expression and `artic-view` needs no `syn`. A value its type refuses is therefore reported by the back end that converts it: the compiler at build time, the loader when it loads.

## Writing Rust

`artic-view-compiler` turns a `CheckedView` into the module body #49 writes today. Emission always runs, on a partial view too, so its problems are reported with the others; `compile_str` keeps the code only when no stage reported an error. It reports:

- a property's group trait path that isn't a Rust path, at each property that uses the trait, before that property's value, as today: a trait is recorded only once its path parses;
- a value that can't be written as Rust, its type's refusal, and a `ToRust` impl that writes invalid Rust, at the value's span;
- a type path that isn't a Rust path, at the type's name;
- the tree module's path or a linked crate path that isn't a Rust path, at the root element.

Each "isn't a Rust path" error keeps its message and its help says "check the paths set with `CratePaths::path`", since `NamespaceMap::path` goes.

Each name becomes the field its `CheckedName` holds, the one `field_name` gives. A type path that isn't a Rust path is an error, so `emit` returns no code for the view and no reference to that element is ever written; the one difference from today is that the checker has already checked that element's members, so a misconfigured `CratePaths` or `using:` path can add property and content errors for it. No snapshot has such an element.

```rust,ignore
/// The path generated code reaches each crate by.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CratePaths { /* crate → path */ }

impl CratePaths {
  /// Generated code reaches `krate` as `path`, such as `artic_controls` as `::artic::controls`.
  pub fn path(&mut self, krate: &str, path: &str);
  /// The path for `krate`: the one set with `path`, or `::krate`.
  pub fn path_of(&self, krate: &str) -> String;
  /// Every crate given a path, sorted.
  pub fn crates_named(&self) -> impl Iterator<Item = &str>;
  /// Where generated code reaches the item at `rust_path`: its crate's path and the item's name.
  pub fn item(&self, rust_path: &str) -> String;
  /// Where generated code reaches `module`: its crate's path, then the rest of `module`.
  pub fn module(&self, module: &str) -> String;
}

/// The module body for `view`, compiled from the file at `path`, referencing every crate of
/// `crates` so the app links it.
/// The code is `None` when the view has no root or emission found an error.
pub fn emit(view: &CheckedView, path: &Path, paths: &CratePaths, crates: &[&str]) -> (Option<String>, Diagnostics);

/// Compiles one file to the Rust module body for it.
pub fn compile_str(
  sources: &SourceMap,
  file: SourceId,
  language: &(impl Language + ?Sized),
  registry: &Registry,
  paths: &CratePaths,
) -> Result<String, Diagnostics>;

/// Compiles every file of `language` under `dir` to `$OUT_DIR/views/<path under dir>.rs`, for a
/// build script. #52 changes the output to the path in the crate.
pub fn compile_dir(dir: impl AsRef<Path>, language: &(impl Language + ?Sized), paths: &CratePaths) -> Result<(), CompileError>;

/// `compile_dir`'s work with every input passed in, for a tool that isn't a build script.
pub fn compile_into(
  dir: &Path,
  out: &Path,
  language: &(impl Language + ?Sized),
  registry: &Registry,
  paths: &CratePaths,
) -> Result<Vec<PathBuf>, CompileError>;
```

`compile_str` reads, checks and emits. When `read` returns no document it returns the language's diagnostics. Otherwise it gathers the diagnostics of the three stages, sorts them by where they start, keeping the order they were found in for one position, and returns them all if any is an error, or the Rust. A view with warnings only returns the Rust and drops them, as `Diagnostics::into_result` does today; #51 decides how cargo shows them. The generated file references every crate of `language.crates()` and `paths.crates_named()`, each once and sorted, as it references every crate the namespace map names today. `compile_dir` is `dir.rs` with the extension and the language passed in. It reads `OUT_DIR` and `CARGO_MANIFEST_DIR`, and no other part of the compiler reads the environment. `compile_into`, which its tests use, moves with it and becomes documented: it takes every input `compile_dir` reads, the output directory and the registry included, and reads no environment.

`CratePaths` is the half of `NamespaceMap` that isn't XAML: `path`, `path_of` and the paths part of `crates_named` move here. `item` and `module` translate a Rust path through it, with `artic-registry`'s `crate_of` and `crate_and_item`, the only readers of a Rust path's ends. Generated code names `Handle` and `UiTree` as `module("artic_core::tree")`, so the facade re-exports `artic_core` as `artic::core` and maps it. `NamespaceMap` keeps `alias` and `crates`, plus a `crates_named` over the crates its URIs stand for, which `Xaml::crates` returns.

## The XAML front end

`artic-xaml-lang` holds `Xaml`, which implements `Language` with `extension` `xaml`. Its `read` parses the file and builds the `Document` from the infoset, member for member:

- each object element's type through a URI's crates or a `using:` module, or a problem: an undeclared URI (`once` set to `namespace:` and the URI), an unknown type with the linking hint when the type's crate is the one `with_app_crate` names, an ambiguous one ("use a `using:` namespace to pick one"), an `x:` primitive (#62), or a value provider written as an element;
- each property member, attribute or property element, as one `Member::Property`; a markup extension in it (the issue that adds it, or an unknown value provider) or an element in it (#57) is the value's problem;
- `x:Name` as `Member::Name`, or the problem "`x:Name` takes a name written as text"; `x:Key` (#111) and attached properties (#50) as `Unsupported`; `x:Class`, `x:FieldModifier` and `x:DataType` as nothing for now, as today, until #52 reads the first two into the document's code type and field visibility and #94 the third into its data type; `x:Uid` as nothing until localization (#168);
- content as `Member::Content`, its text with its MS-XAML form and its raw form. XAML's parser writes markup extensions only in attribute values, so XAML's content has no `Unsupported` node; the variant is for a language whose content can hold one.

Every message keeps its text and its span, and the checker reaches each problem where `lower.rs` reported it, so the snapshots don't change. The XAML compiler crate becomes `artic-xaml-lang` because the loader will use the front end too; a crate named "compiler" would be wrong for it. The crate's fixture packages keep their names, so the generated `use … as _;` lines in their views don't change.

## What changes for an app

| Name | Feature | What it is |
| --- | --- | --- |
| `artic::xaml::compile_dir(dir, &namespaces)` | `xaml-compiler` | `artic::view::compile_dir(dir, &Xaml::new(namespaces.clone()).with_app_crate(name), &artic::view::crate_paths())`, with `name` read from `CARGO_PKG_NAME`: unchanged for an app that sets no crate paths of its own; one that does uses the long form below |
| `artic::xaml::namespaces()` | `xaml` | artic's URI only: `https://artic.rs/ui` → `artic_element`, `artic_controls` |
| `artic::xaml::Xaml` | `xaml-compiler` | the language, for the long form below; `Xaml::with_app_crate(name)` names the crate whose build script runs, for the build-dependency help |
| `artic::view::crate_paths()` | `view-compiler` | artic's crate paths: `artic_controls` → `::artic::controls` and the others, `artic_core` → `::artic::core` included, which generated code reaches `Handle` and `UiTree` by |
| `artic::view::compile_dir`, `artic::view::CompileError` | `view-compiler` | the generic driver and its error |
| `artic::view::CratePaths` | `view-compiler` | the paths, for the long form below |

`xaml-compiler` turns `view-compiler` on. Two names move, so a build script that used them changes:

- `NamespaceMap::path` becomes `CratePaths::path`. `NamespaceMap` no longer has the method, so a build script that called it fails to compile there rather than losing the path. A build script that maps a crate it renamed in `Cargo.toml`, as [the compiler design](xaml-compiler.md#naming-types-in-generated-code) describes, or one re-exported by another crate, sets it on `artic::view::crate_paths()` and calls the long form.
- `artic::xaml::CompileError` becomes `artic::view::CompileError`, so the driver's error has one name.

```rust,ignore
let mut paths = artic::view::crate_paths();
paths.path("acme_controls", "::acme::controls");
let xaml = artic::xaml::Xaml::new(artic::xaml::namespaces()).with_app_crate(env!("CARGO_PKG_NAME"));
artic::view::compile_dir("src/views", &xaml, &paths)?;
```

An app that renamed its `artic` dependency, such as `ui = { package = "artic" }`, used to set `path("artic", "::ui")` on the map. Generated code no longer names the facade: it names each crate through its path, so the app starts from `artic::view::crate_paths()` and sets all five again, `artic_controls`, `artic_core`, `artic_element`, `artic_layout` and `artic_render`, such as `paths.path("artic_core", "::ui::core")`.

Beside the move, a name gives its field in snake_case, with each run of underscores inside it written as one, so the generated `View` needs no `#[allow(non_snake_case)]`: app code that read `view.Email` reads `view.email`. That changes the fields of the `names.xaml` snapshot and drops the `allow` line from every compiled snapshot. Two names with one field, such as `Email` and `email`, are reported as two elements with one name, with help naming both spellings.

## Layers

- `artic-diagnostics` joins layer 0, and `artic-xaml`, which now depends on it, moves to layer 1.
- `artic-view` is in layer 2; `artic-view-compiler` and `artic-xaml-lang` are in layer 3.
- `artic-lsp`, which depends on `artic-xaml-lang`, moves to layer 4, and `artic-cli`, planned to depend on `artic-lsp`, to layer 5. These moves land with PR 3: the XAML compiler crate starts depending on `artic-view` there, so it moves to layer 3, and `artic-lsp` and `artic-cli`, which depend on it, move above it.
- `artic-xaml-runtime` depends on neither new crate yet and stays in layer 2; #59 moves it when the loader uses them.
- `artic-xaml-lang`'s tests compile XAML end to end, so it dev-depends on `artic-view-compiler`, in the same layer, and on the facade (`xaml`, `view-compiler`), whose real paths its snapshots pin; `docs/architecture.md` lists that beside its other dev-dependency rules. Cargo allows these dev-only loops: `artic-xaml-lang` itself depends on neither, and the facade depends on it only with `xaml-compiler`.
- `scripts/check-ui-neutral.sh` allows `artic-xaml-lang` in place of the XAML compiler crate, and covers the three new crates like every other.

## Testing

| Where | Tests |
| --- | --- |
| `artic-diagnostics` | the tests and the render snapshot that move from `artic-xaml`, with neutral file names in the examples |
| `artic-view` | `check` on hand-built documents: an unknown property, an event, a property set twice, a value's problem reported only once its property is found, each content rule, an element in a value collection never reached, a name invalid twice, two elements with one name, an element whose type isn't found with a checked child, a `once` problem reported once; `keywords_match_syn` |
| `artic-view-compiler` | `hand_built_view_compiles`: a test language whose `read` returns a document built in code compiles to Rust that `syn` parses and names the types through `CratePaths`; `CratePaths`' tests, moved from `artic-xaml`; `diagnostics_are_sorted_across_stages`; the `dir.rs` tests, run with the test language |
| `artic-xaml-lang` | every snapshot, the #28 fixture run and the fixture app, moved from the XAML compiler crate with no change below any `.snap` header other than PR 4's three lines; the headers name the moved test and fixture paths; the test helper sets paths on a `CratePaths` instead of the namespace map |
| `artic` | the facade's map tests split: `namespaces()` holds the URI, `crate_paths()` the paths; `tests/xaml.rs`'s `defining_crates_have_facade_paths` reads `crate_paths()` under `view-compiler` |

## Changes to other documents

Documents that describe the code change with the pull request that changes it.

| Where | Change | PR |
| --- | --- | --- |
| `docs/architecture.md` | `artic-diagnostics` in layer 0, `artic-xaml` in layer 1; the layer roles; the planned span move for `artic-style` is done | 2 |
| `docs/architecture.md` | `artic-view` in layer 2, the XAML compiler crate in layer 3, `artic-lsp` in 4, `artic-cli` in 5; the UI-language seam's interface includes `Document` and `Language`; `check-ui-neutral.sh` catches `x:` directives | 3 |
| `docs/architecture.md` | `artic-view-compiler` and `artic-xaml-lang` in layer 3; the crates allowed to say XAML; the dev-dependency rule; the dependency table and graph | 4 |
| `docs/spec/dsl.md` | the intro and a section on what artic gives a language: `Document`, `check`, `Language`, the compiler; §10 says which problems `check` reports for a language; §11's last paragraph names `CratePaths` for the paths and `NamespaceMap` for the URIs | 3 and 4 |
| `docs/design/xaml-compiler.md` | each part names the crate that now owns it; the API section shows the generic driver and the facade's short form; `NamespaceMap::path` becomes `CratePaths::path` | 4 |
| `docs/design/ui-class.md` | "the UI-language-neutral view compiler" becomes `artic-view` and `artic-view-compiler`; the XAML compiler crate becomes `artic-xaml-lang`, including the fixture app's path | 4 |
| `docs/design/dsl-neutral.md` | `NamespaceMap` holds URIs, the paths are `CratePaths`, and the XAML compiler crate is `artic-xaml-lang` | 4 |
| `docs/spec/technical_spec.md` §2 and §5, `README.md` | the crate lists and the build pipeline name the new crates | 2 to 4, each for its crates |

## Not in this design

- The language server's type index read from source (#180, #183): the document holds `&'static TypeInfo` from the link-time registry, and an index that isn't linked would need descriptors of its own. That design decides how they reach a `Document`.
- The runtime loader (#59) and where it lives.

## Pull requests

| PR | Contents |
| --- | --- |
| 1 | this document |
| 2 | `artic-diagnostics`, moved from `artic-xaml`; every user updated |
| 3 | `artic-view`: `Document`, `check`, `CheckedView`, `Language`; the XAML compiler reads into a `Document` and emits from a `CheckedView` |
| 4 | `artic-view-compiler`: emission, `CratePaths`, the generic `compile_str` and `compile_dir`, the test language; `NamespaceMap` keeps URIs only; the XAML compiler crate becomes `artic-xaml-lang`; the facade's `artic::view`; closes #325 |

Each pull request keeps every snapshot's content and the fixture app unchanged, apart from that render snapshot in PR 2, the snake_case fields above, and PR 4's three lines of every compiled file: the tree types' `use`, the link to `::artic::core` and the `View` doc.
