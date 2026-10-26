# Design: the view pipeline beside every UI language

Issue: #325. Builds on: the [XAML compiler](xaml-compiler.md) (#49), [UI languages beside XAML](dsl-neutral.md) (#305), the [language contract](../spec/dsl.md). Unblocks: #52's compiler changes ([code type design](ui-class.md)), #51 diagnostics in cargo output, #59 the runtime loader.

The compiler #49 built sits in the XAML crates, but most of it isn't XAML: diagnostics with source spans, checking a view's members against the registry, writing Rust, and driving a build script would be the same for any language. This design moves those parts into three crates no language owns. A language then writes one front end, from its syntax to a plain `Document`, and gets the compiler and, with #59, the loader from it. The XAML crates keep XAML's syntax and meaning only.

The generated code and every diagnostic stay the same: each snapshot's content and the fixture app stay unchanged; only the header insta writes above a moved snapshot, which names its source file, follows the move. The one difference is for a misconfigured path, [below](#writing-rust). A few names an app's build script can reach move; [What changes for an app](#what-changes-for-an-app) lists them.

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
| `artic-view` | `Document`, `check`, `CheckedView`, the `Language` trait, the field-name rule | `artic-registry`, `artic-diagnostics`, `unicode-ident` |
| `artic-view-compiler` | `emit`, literals, `CratePaths`, `compile_str`, `compile_dir`, `CompileError` | `artic-view`, `artic-registry` (with `literals`), `artic-diagnostics`, `syn`, `quote`, `proc-macro2`, `prettyplease`, `thiserror` |
| `artic-xaml` | the parser, the infoset, `NamespaceMap` (URIs only) | `artic-diagnostics` |
| `artic-xaml-lang` (was `artic-xaml-compiler`) | `Xaml`, the front end | `artic-xaml`, `artic-view`, `artic-registry`, `artic-diagnostics` |

`artic-diagnostics` has no dependencies, so `artic-style`'s selector errors and the parser use spans without the registry. `artic-view` has no `syn`, so the loader doesn't carry a code generator at run time. `artic-xaml` stays free of the registry, as `docs/architecture.md` requires, so its tests still need no registry. `artic-view-compiler` turns on the registry's `literals` feature, which `artic-xaml-compiler` turns on today.

The moved code keeps its behavior; its own tests and docs, which use XAML file names and messages as examples, are rewritten with neutral ones, so `scripts/check-ui-neutral.sh` passes over the new crates. The render snapshot keeps its rendered content; its file name and its `source:` header follow the new crate.

## The document

A front end turns its syntax into a `Document`: plain data, every type looked up, every member still as written, in the language's member order. It's what a test builds by hand.

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

/// The field a name gives in a view's `View`, such as `r#type` for `type`, or why it can't be one.
pub fn field_name(name: &str) -> Result<String, &'static str>;
```

The checker is `lower.rs`'s member half, unchanged in behavior, and needs no registry: each `TypeInfo` it holds answers for its own properties. It walks the document in member order, children where their content member is, and, for each element it reaches:

1. reports the type's problem and goes on: properties are checked only when the type was found; names and `Unsupported` members always; the elements in the content always when the type wasn't found, and as step 4 says when it was;
2. for a property, finds it by name or reports it unknown (an event's name gives "event handlers come with #82"), then reports the value's problem or a property set twice;
3. for a name, reports its problem, then a name `field_name` refuses, with the reason as help, then two elements with one name, so a name that's invalid twice is reported twice;
4. for content, applies the type's rule: text for a content property, where the first element or `Unsupported` node is reported and ends the content; children for `child` or `children`, where text is reported with the help "put the text in an element, such as a `TextBlock`", every child element counts toward "takes one child" whether its type was found or not, and the child taken is the only found one; nothing for a type that takes none, where text and elements are reported and the elements still checked; and, for a collection of values, "value collections come with #57" when it holds anything but blank text, the elements inside it never reached;
5. reports each `Unsupported` member.

An element whose type isn't found still has its name checked and the elements in its content checked, so an unknown type doesn't hide the problems below it. `field_name` is `names.rs`'s rule without `syn`: an identifier by Unicode's XID rules (`unicode-ident`), not `_`, a keyword written as a raw identifier, and `self`, `Self`, `crate`, `super`, `root` and a leading `r#` refused with today's three reasons. Its keyword list is checked in its tests against `syn`, and emission takes each field from it.

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
  /// Each valid name with its element's index, in the order the elements are built.
  pub names: Vec<(Text, usize)>,
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
- the prelude or a linked crate path that isn't a Rust path, at the root element.

Each name becomes the field `field_name` gives. A type path that isn't a Rust path leaves the element unbuilt, as today; the one difference from today is that the checker has already checked that element's members, so a misconfigured `CratePaths` or `using:` path can add property and content errors for it. No snapshot has such an element.

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
```

`compile_str` reads, checks and emits. When `read` returns no document it returns the language's diagnostics. Otherwise it gathers the diagnostics of the three stages, sorts them by where they start, keeping the order they were found in for one position, and returns them all if any is an error, or the Rust. A view with warnings only returns the Rust and drops them, as `Diagnostics::into_result` does today; #51 decides how cargo shows them. The generated file references every crate of `language.crates()` and `paths.crates_named()`, each once and sorted, as it references every crate the namespace map names today. `compile_dir` is `dir.rs` with the extension and the language passed in; the doc-hidden `compile_into`, which its tests use, moves with it.

`CratePaths` is the half of `NamespaceMap` that isn't XAML: `path`, `path_of` and the paths part of `crates_named` move here. `NamespaceMap` keeps `alias` and `crates`, plus a `crates_named` over the crates its URIs stand for, which `Xaml::crates` returns.

## The XAML front end

`artic-xaml-lang` holds `Xaml`, which implements `Language` with `extension` `xaml`. Its `read` parses the file and builds the `Document` from the infoset, member for member:

- each object element's type through a URI's crates or a `using:` module, or a problem: an undeclared URI (`once` set to `namespace:` and the URI), an unknown type with the linking hint, an ambiguous one ("use a `using:` namespace to pick one"), an `x:` primitive (#62), or a value provider written as an element;
- each property member, attribute or property element, as one `Member::Property`; a markup extension in it (the issue that adds it, or an unknown value provider) or an element in it (#57) is the value's problem;
- `x:Name` as `Member::Name`, or the problem "`x:Name` takes a name written as text"; `x:Key` (#111) and attached properties (#50) as `Unsupported`; `x:Class`, `x:FieldModifier` and `x:DataType` as nothing for now, as today, until #52 reads the first two into the document's code type and field visibility and #94 the third into its data type; `x:Uid` as nothing until localization (#168);
- content as `Member::Content`, its text with its MS-XAML form and its raw form. XAML's parser writes markup extensions only in attribute values, so XAML's content has no `Unsupported` node; the variant is for a language whose content can hold one.

Every message keeps its text and its span, and the checker reaches each problem where `lower.rs` reported it, so the snapshots don't change. `artic-xaml-compiler` becomes `artic-xaml-lang` because the loader will use the front end too; a crate named "compiler" would be wrong for it. The crate's fixture packages keep their names, so the generated `use … as _;` lines in their views don't change.

## What changes for an app

| Name | Feature | What it is |
| --- | --- | --- |
| `artic::xaml::compile_dir(dir, &namespaces)` | `xaml-compiler` | `artic::view::compile_dir(dir, &Xaml::new(namespaces.clone()), &artic::view::crate_paths())`: unchanged for an app that sets no crate paths of its own; one that does uses the long form below |
| `artic::xaml::namespaces()` | `xaml` | artic's URI only: `https://artic.rs/ui` → `artic_element`, `artic_controls` |
| `artic::xaml::Xaml` | `xaml-compiler` | the language, for the long form below |
| `artic::view::crate_paths()` | `view-compiler` | artic's crate paths: `artic_controls` → `::artic::controls` and the others |
| `artic::view::compile_dir`, `artic::view::CompileError` | `view-compiler` | the generic driver and its error |

`xaml-compiler` turns `view-compiler` on. Two names move, so a build script that used them changes:

- `NamespaceMap::path` becomes `CratePaths::path`. `NamespaceMap` no longer has the method, so a build script that called it fails to compile there rather than losing the path. A build script that maps a crate it renamed in `Cargo.toml`, as [the compiler design](xaml-compiler.md#naming-types-in-generated-code) describes, or one re-exported by another crate, sets it on `artic::view::crate_paths()` and calls the long form.
- `artic::xaml::CompileError` becomes `artic::view::CompileError`, so the driver's error has one name.

```rust,ignore
let mut paths = artic::view::crate_paths();
paths.path("acme_controls", "::acme::controls");
let xaml = artic::xaml::Xaml::new(artic::xaml::namespaces());
artic::view::compile_dir("src/views", &xaml, &paths)?;
```

## Layers

- `artic-diagnostics` joins layer 0, and `artic-xaml`, which now depends on it, moves to layer 1.
- `artic-view` is in layer 2; `artic-view-compiler` and `artic-xaml-lang` are in layer 3.
- `artic-lsp`, which depends on `artic-xaml-lang`, moves to layer 4, and `artic-cli`, planned to depend on `artic-lsp`, to layer 5.
- `artic-xaml-runtime` depends on neither new crate yet and stays in layer 2; #59 moves it when the loader uses them.
- `artic-xaml-lang`'s tests compile XAML end to end, so it dev-depends on `artic-view-compiler`, in the same layer; `docs/architecture.md` lists that beside its other dev-dependency rules. Cargo allows it, since `artic-view-compiler` doesn't depend on `artic-xaml-lang`.
- `scripts/check-ui-neutral.sh` allows `artic-xaml-lang` in place of `artic-xaml-compiler`, and covers the three new crates like every other.

## Testing

| Where | Tests |
| --- | --- |
| `artic-diagnostics` | the tests and the render snapshot that move from `artic-xaml`, with neutral file names in the examples |
| `artic-view` | `check` on hand-built documents: an unknown property, an event, a property set twice, a value's problem reported only once its property is found, each content rule, an element in a value collection never reached, a name invalid twice, two elements with one name, an element whose type isn't found with a checked child, a `once` problem reported once; `keywords_match_syn` |
| `artic-view-compiler` | `hand_built_view_compiles`: a test language whose `read` returns a document built in code compiles to Rust that `syn` parses and names the types through `CratePaths`; `CratePaths`' tests, moved from `artic-xaml`; `diagnostics_are_sorted_across_stages`; the `dir.rs` tests, run with the test language |
| `artic-xaml-lang` | every snapshot, the #28 fixture run and the fixture app, moved from `artic-xaml-compiler` with no change below any `.snap` header; the headers name the moved test and fixture paths; the test helper sets paths on a `CratePaths` instead of the namespace map |
| `artic` | the facade's map tests split: `namespaces()` holds the URI, `crate_paths()` the paths; `tests/xaml.rs`'s `defining_crates_have_facade_paths` reads `crate_paths()` under `view-compiler` |

## Changes to other documents

Documents that describe the code change with the pull request that changes it.

| Where | Change | PR |
| --- | --- | --- |
| `docs/architecture.md` | `artic-diagnostics` in layer 0, `artic-xaml` in layer 1; the layer roles; the planned span move for `artic-style` is done | 2 |
| `docs/architecture.md` | `artic-view` in layer 2; the UI-language seam's interface includes `Document` and `Language` | 3 |
| `docs/architecture.md` | `artic-view-compiler` and `artic-xaml-lang` in layer 3, `artic-lsp` in 4, `artic-cli` in 5; the crates allowed to say XAML; the dev-dependency rule; the dependency table and graph | 4 |
| `docs/spec/dsl.md` | the intro and a section on what artic gives a language: `Document`, `check`, `Language`, the compiler; §10 says which problems `check` reports for a language; §11's last paragraph names `CratePaths` for the paths and `NamespaceMap` for the URIs | 3 and 4 |
| `docs/design/xaml-compiler.md` | each part names the crate that now owns it; the API section shows the generic driver and the facade's short form; `NamespaceMap::path` becomes `CratePaths::path` | 4 |
| `docs/design/ui-class.md` | "the UI-language-neutral view compiler" becomes `artic-view` and `artic-view-compiler`; `artic-xaml-compiler` becomes `artic-xaml-lang`, including the fixture app's path | 4 |
| `docs/design/dsl-neutral.md` | `NamespaceMap` holds URIs, the paths are `CratePaths`, and `artic-xaml-compiler` is `artic-xaml-lang` | 4 |
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
| 4 | `artic-view-compiler`: emission, `CratePaths`, the generic `compile_str` and `compile_dir`, the test language; `NamespaceMap` keeps URIs only; `artic-xaml-compiler` becomes `artic-xaml-lang`; the facade's `artic::view`; closes #325 |

Each pull request keeps every snapshot's content and the fixture app unchanged.
