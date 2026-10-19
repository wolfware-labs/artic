# Design: UI languages beside XAML

Issue: #305. Builds on: [naming review](naming-review.md) (#30, D20), [value objects](value-objects.md) (#32), [elements](element.md) (#42), [TextBlock](text-block.md) (#46). Unblocks: #43, #49.

An app picks its host, its renderer and its UI language. Host and renderer already sit behind traits: no layer depends on winit or vello. The language is separate by dependency too: no crate that defines types, layout, drawing or controls depends on `artic-xaml`, `artic-xaml-compiler` or `artic-xaml-runtime`; only the XAML crates themselves, the facade's `runtime-xaml` and `xaml-compiler` features, `artic-lsp` and `artic-cli` do. `docs/architecture.md` plans one more edge, `artic-style` → `artic-xaml`, for `Span` and `Diagnostic` in selector errors; this design drops it (section 5). Its names, though, reach into the layers below it. Every UI type derives `XamlType`, states an XML namespace, and marks its fields `#[xaml(...)]`; layout enums derive `XamlEnum`; the registry's traits and lists are named after XAML.

D20 keeps WPF framing out of the APIs while the markup keeps the XAML name. This design extends it: the layers below a language describe types in neutral terms, and only the XAML crates speak XAML. A new spec, `docs/spec/dsl.md`, states the contract any language follows, so someone can write their own or plug in one that exists. It keeps XAML's ideas (attached properties, content, values written as text, markup extensions, namespaces) and names them neutrally. XAML is the first language that follows it.

## 1. Names

Below the XAML layer, every name that says XAML changes:

| Before | After |
| --- | --- |
| `#[derive(XamlType)]`, the `XamlType` trait, its `__private` re-export that `#[element]` emits | `#[derive(UiType)]`, the `UiType` trait, the `__private` re-export |
| `#[xaml(...)]` on structs and fields, and the messages that name it, such as "at most one `#[xaml(content)]` member" | `#[ui(...)]`, and the same messages |
| `#[xaml(namespace = "...")]` and its "missing the XML namespace" error | removed (section 2) |
| `#[derive(XamlEnum)]` | `#[derive(UiEnum)]` |
| `#[derive(XamlProperties)]` | `#[derive(UiProperties)]` |
| `XamlCollection` | `UiCollection` |
| `XAML_TYPES`, the link-time list of types | `UI_TYPES` |
| `MarkupExtensionInfo`, `#[xaml(markup_extension(...))]`, `TypeInfo::markup_extension`, `TypeInfo::is_markup_extension()` | `ValueProviderInfo`, `#[ui(value_provider(...))]`, `TypeInfo::value_provider`, `TypeInfo::is_value_provider()` |
| `value_must_implement_fromstr_or_derive_xamltype`, `ViaXamlType` | `value_must_implement_fromstr_or_derive_uitype`, `ViaUiType` |
| `#[xaml_class]`, `#[xaml_static]` (planned in the spec, not built) | `#[ui_class]`, `#[ui_static]` |
| in `artic-macros`: the `derive_xaml_*` functions, the `xaml_type/`, `xaml_enum.rs` and `xaml_properties.rs` modules, `XamlField`, `xaml_fields`, `xaml_name` | `derive_ui_*`, `ui_type/`, `ui_enum.rs`, `ui_properties.rs`, `UiField`, `ui_fields`, `ui_name` |
| tests named after XAML: `stroke_defaults_match_xaml`, the controls' `*_is_xaml_content`, the trybuild cases `element_derives_xaml_type`, `xaml_on_plain_field`, `value_not_text_or_xaml_type`, `attached_value_not_text_or_xaml_type`, and `tests/xaml_enum.rs` | the same names with `ui` |
| docs that say "XAML" in `artic-core` (its `__private` and `ParseEnumError` docs, `event.rs`, `input.rs`), `artic-layout` (`geometry/parse.rs`, `text/font.rs`), `artic-render` (`color.rs`), `artic-registry`, `artic-macros`, `artic-element` and `artic-controls` | neutral wording, or a pointer to the XAML crates |

`UiType` says what the derive describes a type for: any UI language, markup or code, such as a `view!{}` macro. It's short enough for every field (`#[ui(affects = layout)]`, `#[ui(content)]`, `#[ui(attached(name = "Row", ...))]`). `Reflect` was set aside: it promises general reflection, which this isn't, and it collides with `bevy_reflect`. `Markup` assumes the language is markup.

`UiEnum` keeps the enum derive in the same family and leaves room to record variant names for tooling later, such as completion in the language server. Today it writes only a case-insensitive `FromStr` whose error lists the names.

XAML's concepts keep their names where they already say what they do without XAML, and no widely known neutral name exists, which is D20's rule:

| XAML concept | In artic |
| --- | --- |
| namespace | namespace: the crate a type comes from (section 2) |
| attached property | attached property |
| content property | content |
| markup extension (`{Binding Name}`) | value provider: a type that computes a property's value when the UI is built, such as a binding or a resource lookup. A code language calls it like a function; "markup extension" assumed markup |
| type converter | a value written as text, through `FromStr` |
| collection element | `UiCollection` |

`#[element]`, `.xaml` files, MS-XAML syntax, the `x:` directives, the XAML crates' names, and `artic-lsp`, `artic-cli` and the facade's `runtime-xaml` and `xaml-compiler` features, which are about XAML, don't change.

A glossary at the top of `dsl.md` keeps neighboring names apart: `UiType` (a type a language can name) and `UiTree` (the node tree), `UI_TYPES` (types for languages) and `ELEMENT_TYPES` (how layout and rendering reach an element), a value provider (computes a property's value) and a value object (a type written as a value, [value objects](value-objects.md)).

## 2. Namespaces

A namespace is a named group of type names, so two libraries can each have a `Button` and a language can say which it means. Today each type states an XML namespace URI, `#[xaml(namespace = "https://artic.rs/ui")]`: 8 places in the crates, more in tests and fixtures.

A type's namespace is the crate it comes from. Nothing declares it and nothing stores it: `TypeInfo::namespace()` returns the first segment of `TypeInfo::rust_path`, the path the derive already records with `module_path!()`, so the two can't disagree, and a `TypeInfo` written by hand gets it the same way. A dependency renamed in a `Cargo.toml` doesn't change it, since `module_path!()` uses the defining crate's own name. The `namespace` field goes.

`rust_path` is where the type is defined, such as `artic_controls::border::Border`, not a path it's re-exported under. Its doc says otherwise today (`artic_controls::Border`) and is corrected; `dsl.md` states the rule.

Two types in one crate may share a name when their Rust paths differ, such as `my_app::home::Page` and `my_app::settings::Page`. The registry accepts both; only a Rust path registered twice is an error when it's built. A lookup by name reports the ambiguity:

```rust,ignore
pub enum Lookup {
  Found(&'static TypeInfo),
  Ambiguous(Vec<&'static TypeInfo>),
  Missing,
}

impl Registry {
  pub fn type_by_name(&self, namespace: &str, name: &str) -> Lookup;
  pub fn type_by_rust_path(&self, path: &str) -> Option<&'static TypeInfo>;
}
```

A language that reaches a type through its Rust path, as XAML's `using:my_app::home` does, finds each one. Moving a type to another crate changes its namespace; code that names it by path changes anyway, and the alias that covered the old crate is updated where it's declared.

A namespace stays a crate rather than a name each type picks: picking one per type would bring back an attribute on every type, and a hand-written string can drift from the crate.

## 3. The XAML layer

The XAML back ends that look types up, the loader and the compiler, aren't built yet: `artic-xaml` parses, `artic-xaml-runtime` and `artic-xaml-compiler` are empty, and the spec makes resolving types their first step. So this design gives the XAML layer the map they'll use, without a registry:

```rust,ignore
/// Which crates each namespace URI stands for.
pub struct NamespaceMap { /* URI -> crate names */ }

impl NamespaceMap {
  /// Adds `crates` to what `uri` stands for. A crate it already stands for is kept once, so
  /// declaring an alias twice changes nothing.
  pub fn alias(&mut self, uri: &str, crates: &[&str]);
  /// The crates `uri` stands for, or `None` for a URI nobody declared.
  pub fn crates(&self, uri: &str) -> Option<&[String]>;
  /// Generated code reaches `krate` as `path`, such as `artic_controls` as `::artic::controls`.
  pub fn path(&mut self, krate: &str, path: &str);
  /// The path for `krate`: the one set with `path`, or `::krate`.
  pub fn path_of(&self, krate: &str) -> String;
  /// Every crate the map names, through a URI or a path, each once and sorted.
  pub fn crates_named(&self) -> impl Iterator<Item = &str>;
}
```

It lives in `artic-xaml`, beside the parser, and depends on nothing:

- `using:my_crate::views` keeps working as the spec describes, and needs no entry: the back end joins the module and the local name into a Rust path, `my_crate::views::Gauge`, and calls `Registry::type_by_rust_path`, so it finds exactly the type in that module even when another module of the crate has one of the same name.
- Nothing is built in. The `artic` facade, which already depends on every artic crate with UI types, owns artic's entry: `artic::xaml::namespaces() -> NamespaceMap` returns a map holding `https://artic.rs/ui` → `artic_element`, `artic_controls` (and the style crates when they define types), added through `alias`, the call an app uses for its own URIs, such as `https://acme.com/ui` → `acme_controls`. A new artic crate is added where the crates are composed.
- Both back ends start from that map. The runtime loader takes it when the app sets it up. The compiler runs in the app's `build.rs`, which can't see anything the app does at run time, so the app passes it the same map there: `build.rs` calls `artic::xaml::namespaces()` through its build-dependency on `artic` with the `xaml-compiler` feature, which turns `xaml` on (adding `artic-xaml`) and adds the compiler, then adds its own aliases and calls `artic::xaml::compile_dir` ([XAML compiler design](xaml-compiler.md)). The facade's `runtime-xaml` and `hot-reload` features turn `xaml` on, so the loader has the same function. One function gives both paths the same names.
- A URI that maps to several crates finds a name in each; a name found in two of them is ambiguous, and the back end reports it.
- The registry only knows the crates the app links: `UI_TYPES` is collected at link time, so a crate the app depends on but never references is left out, as `docs/architecture.md` notes. Every crate a document reaches, through an alias or a `using:` prefix, must therefore be linked. The facade's `xaml` module refers to each crate artic's URI names (`use artic_controls as _;`), so they're linked in every app that uses it; an app that names a crate it doesn't otherwise use references it once with `use acme_controls as _;`. That line uses the name the app's `Cargo.toml` gives the dependency, while documents, aliases and `using:` paths use the crate's own name, the one in its types' Rust paths: a dependency renamed to `acme` is linked with `use acme as _;` and still named `acme_controls` in a document. A back end that finds no type in a URI's crates, or no type at a `using:` path whose crate registered nothing, says so, and names that line as the likely fix. The same holds for the compiler, which resolves names in the app's `build.rs`, a separate program with its own link: as spec §5 says, `build.rs` lists the facade and the app's control crates as build-dependencies, and references each one it reaches the same way, so their types are in the registry it builds.
- `https://artic.rs/markup`, the `x:` directives, stays XAML's own.

A control library therefore never depends on or mentions XAML, and neither the parser nor the registry knows artic's crate layout. The loader (#43's builder path, #49's generated code) resolves a type by its prefix. A `using:` prefix gives a Rust path, looked up with `Registry::type_by_rust_path`. A URI prefix goes through the map to crates, then one `Registry::type_by_name(crate, name)` call on the app's single registry for each of those crates: one `Found` across them is the type; two, or an `Ambiguous`, is ambiguous; none is missing.

## 4. The language spec

`docs/spec/dsl.md` states the contract, with the registry and the tree as the only interface, by what a language does:

1. **Who it's for, and the two paths.** A language is a front end that builds artic's tree through the registry. It interprets at run time, as the XAML loader will, or generates Rust at build time, as the XAML compiler will. Both paths build the same tree. A glossary keeps neighboring names apart (section 1).
2. **Finding types** by namespace (crate) and name, or by the Rust path where the type is defined; ambiguity, and what a language does with it. A language's own aliases, such as XAML's URIs, belong to the language.
3. **Creating nodes** with `TypeInfo::new`.
4. **Properties**: lookup by name, values written as text (`FromStr`, `ValueError`), values that are objects (`UiType`), and the precedence a language sets at (local).
5. **Attached properties**: declared by an owner type, the target rule (any node, or children of the owner), and setting one on a node.
6. **Content and children**: the content kinds (one child, a collection, text), `UiCollection`, and whitespace.
7. **Events**: lookup by name, and how a handler name binds to code.
8. **Value providers**: positional and named arguments, and when they resolve. Bindings, resources, type references, statics and null are all value providers.
9. **Language features** a language may offer, stated neutrally: node names (`x:Name`), resource keys (`x:Key`), the code type behind a view (`x:Class`), the data type compiled bindings check against (`x:DataType`), and typed literals (`x:String` and the other primitives).
10. **Errors**: what to report, with which source location.
11. **XAML's mapping**: each construct in spec §4's supported table, pointing at the section it uses.

## 5. Documents

The goal goes into the spec and the architecture:

- spec §1 gains a locked decision: host, renderer and UI language are each pluggable, and the layers below the language don't name it;
- spec §2 and `docs/architecture.md` show the language layer beside the host and renderer adapters;
- spec §3 describes the registry in neutral terms and points at `dsl.md`; its "Namespaces" subsection, which is about XAML, moves to §4, and "a back end reports a URI no registered type declares" becomes "a URI the namespace map doesn't know";
- spec §4 stays XAML's;
- `docs/architecture.md` drops the planned `artic-style` → `artic-xaml` edge: when selector errors need a source span, the span and diagnostic types move to a crate below every language, so `artic-style` names no language.

After the rename, every document is checked against the code and updated to it: `docs/spec/technical_spec.md`, `docs/architecture.md`, every file in `docs/design/` and `docs/spikes/`, and the crates' module docs. The pull request lists each file with what it needed, or that it was checked and needed nothing. Design documents describe the code as it is, so they use the new names too; the decisions they record keep their reasons.

Two checks keep the documents in step with the code from then on, over everything in `docs/` that git tracks:

- **Rust code in the documents compiles.** A crate that isn't published, `docs-check`, includes each document with `#[doc = include_str!("../../../docs/...")]` (paths relative to the source file), so `cargo test --doc -p docs-check` compiles every Rust code block and a renamed API breaks the build. To make that hold:
  - blocks fenced with no language are compiled as Rust by rustdoc, so each one gets a language (`text`, `xml`, `bash`); `xml`, `bash`, `mermaid` and `text` blocks are skipped;
  - a block that is a sketch, not meant to compile, is marked `rust,ignore` in the document itself, and stays marked;
  - a block that needs imports gets them as hidden `# use ...` lines, which rustdoc hides and GitHub shows; a document whose example would read worse with them marks the block `ignore` instead;
  - each block is its own test binary (`docs-check` sets `edition = "2021"`, or marks blocks `standalone_crate`), since edition 2024 merges doctests into one binary and their `UiType`s would share one link-time list;
  - `docs-check` depends on every crate the blocks use, and is left out of the `cargo doc -D warnings` job, since markdown prose such as `<Button/>` and bare URLs isn't rustdoc's.
- **Links resolve.** lychee runs in CI over `docs/`, offline, with `--include-fragments` and `--root-dir` set to the repository, so a renamed file or heading breaks the build without any network call. Offline it skips absolute GitHub links; headings with backticks, dashes or a repeated title are where its anchors could differ from GitHub's, so the check is tried on such a heading before it's trusted.

Design documents keep their "Documents that change" tables.

## 6. Testing

| Where | Tests |
| --- | --- |
| `artic-registry` | `namespace_is_the_first_segment_of_the_rust_path`, `same_name_in_one_crate_is_ambiguous_by_name_and_found_by_path`, `same_rust_path_twice_is_an_error`, `missing_name_is_missing`; `same_name_in_two_crates_is_found_in_each` in the real-crate fixtures (`fixtures/types-a`, `types-b`, `linking/tests`): their `#[xaml(namespace = ...)]` attributes are removed, so their types' namespaces become their crate names, `artic_registry_fixture_a` and `artic_registry_fixture_b`; each fixture also gains a type named `Twin`, beside its `Alpha` or `Beta`, and the test finds `Twin` in each crate by name and by Rust path |
| `artic-macros` | the derive tests and trybuild cases under the new names; `derived_namespace_is_the_crate`; every new or renamed `.stderr` checked for `$RUST/` lines |
| `artic-xaml` | `NamespaceMap`: `alias_maps_a_uri_to_crates`, `alias_adds_to_a_uri`, `repeated_alias_is_kept_once`, `unknown_uri_is_none`; the fixture corpus and its snapshots unchanged |
| `artic` | `facade_maps_the_artic_uri` (with the `xaml` feature on) |
| workspace | every test and golden passes; nothing below the XAML layer names XAML: a `grep` over `crates/` whose allow-list is `artic-xaml*`, `artic-lsp`, `artic-cli` and the facade's `runtime-xaml` and `xaml-compiler` features and their doc lines |
| `docs-check` | every Rust block in `docs/` compiles; lychee passes |

## 7. Pull requests

| PR | Contents | Done when |
| --- | --- | --- |
| A | this document | approved |
| B | sections 1 to 3: the renames, the namespace from the Rust path, `Lookup`, `NamespaceMap` and the facade's alias, the trybuild cases and fixtures updated | the workspace tests, goldens and XAML fixtures pass |
| C | sections 4 and 5: `docs/spec/dsl.md`, the goal in the spec and the architecture, the check of every document, `docs-check` and lychee in CI; closes #305 | both checks pass in CI |

B renames everything in one pull request, so the code never holds a mix of old and new names.
