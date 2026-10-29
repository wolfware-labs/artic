# UI language contract

A UI language is a front end that builds artic's tree. XAML is the first; a `view!{}` macro, another markup or a visual designer could be others. This document states what any of them does, with artic's registry and tree as the only interface, so a language can be added without changing artic. A language reads its files into `artic-view`'s `Document` and gets the checks below and, with [§12](#12-what-artic-gives-a-language), the compiler. The layers below a language never name it ([spec §1](technical_spec.md#1-overview-and-locked-decisions)).

Some of the contract is built and some comes with XAML's back ends. Each section says which.

## Glossary

| Name | What it is | Not to be confused with |
| --- | --- | --- |
| UI type | A Rust type a language can name, described by a `TypeInfo`. `#[derive(UiType)]` implements the `UiType` trait | `UiTree`, the tree of nodes a language builds |
| `UI_TYPES` | The link-time list of every UI type in the binary | `ELEMENT_TYPES`, which layout and rendering use to find a node's element |
| namespace | The crate a type is defined in | a Rust module; an XML namespace URI, which is XAML's alias for one or more namespaces |
| value provider | A type that computes a property's value when the UI is built, such as a binding | a value object, a type written as a property's value ([value objects](../design/value-objects.md)) |
| attached property | A property one type declares and other nodes hold, such as `Grid.Row` | a property group, such as `Layout`, which a type embeds as its own |
| content | What a type receives when a language gives it children or text without naming a property | |

## 1. Two paths

**The registry both paths read and the XAML compiler (#49) are built. The runtime loader comes with #58.**

A language interprets its source when the app runs, or generates Rust when the app builds. Both build the same tree.

- **At run time** the language reads the registry: it looks types and properties up by name and calls the function pointers in their descriptors. XAML's runtime loader will work this way, for hot reload and the `xaml-runtime` feature.
- **At build time** the language reads the registry while it generates code, and the code calls the types directly, such as `Border::default()` and `border.padding.set(...)`. Nothing is looked up when the app runs. XAML's compiler will work this way, from the app's `build.rs`.

`build.rs` is a separate program, and its registry holds only the crates it links. A build-time language lists the crates it reaches as build-dependencies and names each one, as [Finding types](#2-finding-types) explains.

## 2. Finding types

**Built.**

A type is found by namespace and name, or by the Rust path where it's defined:

```rust
use artic_controls as _;
use artic_registry::{Lookup, Registry};

let registry = Registry::from_link_time().expect("no Rust path is registered twice");

let border = registry.type_by_name("artic_controls", "Border").found().expect("Border is linked");
assert_eq!(border.rust_path, "artic_controls::border::Border");
assert_eq!(border.namespace(), "artic_controls");

assert!(registry.type_by_rust_path("artic_controls::border::Border").is_some());
assert!(matches!(registry.type_by_name("artic_controls", "Gauge"), Lookup::Missing));
```

- A type's namespace is its crate: `TypeInfo::namespace()` returns the first segment of `TypeInfo::rust_path`. Nothing declares it. A dependency renamed in `Cargo.toml` keeps its crate's own name here.
- `rust_path` is where the type is defined, such as `artic_controls::border::Border`, never a path it's re-exported under.
- Names are exact and case-sensitive.
- Two types of one crate may share a name in different modules. `type_by_name` then returns `Lookup::Ambiguous` with both, sorted by Rust path, and the language reports it so the author can name the type by path. Only these make `Registry::from_link_time` fail, each reported as a `Duplicate`: a Rust path registered twice, a type with two properties of one name (counting the groups it embeds), and an owner that declares one attached property twice.
- The registry holds only the crates the binary links. A crate the app depends on but never names is dropped before linking. A language that finds nothing in a crate it expected says so and suggests `use that_crate as _;`, written with the name the app's `Cargo.toml` gives the dependency.

How a language spells a namespace is its own business. XAML maps URIs to crates and reaches modules with `using:` ([XAML's mapping](#11-xamls-mapping)).

## 3. Creating nodes

**Built.**

`TypeInfo::new` creates a node with its default values and returns its id:

```rust
# use artic_controls as _;
# use artic_registry::Registry;
use artic_core::tree::UiTree;

# let registry = Registry::from_link_time().unwrap();
# let border = registry.type_by_name("artic_controls", "Border").found().unwrap();
let mut tree = UiTree::new();
let node = (border.new)(&mut tree);
assert!(tree.get_as::<artic_controls::Border>(node).is_some());
```

A build-time language writes the constructor instead: `Border::new()` and the builder methods from [the builder API](../design/builder-api.md), finished by `build(tree)`.

## 4. Properties

**Lookup is built. Setting a value by name comes with the runtime loader (#58).**

`TypeInfo::property` finds a property among the type's own and those of the groups it embeds, such as `Width` from the `Layout` group every control has. The `PropertyRef` it returns names the group's field, so generated code can write `border.layout.width`:

```rust
# use artic_controls as _;
# use artic_registry::Registry;
# let registry = Registry::from_link_time().unwrap();
# let border = registry.type_by_name("artic_controls", "Border").found().unwrap();
let padding = border.property("Padding").expect("Border has Padding");
assert_eq!(padding.group, None);
assert_eq!(padding.info.field, "padding");

let width = border.property("Width").expect("every control has Width");
assert_eq!(width.group, Some("layout"));
```

A value reaches a property in one of four forms:

- **Text**, parsed with the value type's `FromStr`. A failure is `ValueError::Parse`, carrying the type's own message.
- **An object** of a type that is itself a `UiType`, such as a brush: built as a node, then converted into the property's type ([value objects](../design/value-objects.md)).
- **A child node**, for a property whose type is `NodeId`, or `Option<NodeId>`, filled in by the node written inside it.
- **Null**, for a property whose type is `Option<T>`. Any other refuses it with `ValueError::NotNullable`.

The derive refuses, at compile time, a property whose type takes none of these.

A language sets values at the `Local` precedence: above styles and bindings, below a running animation ([spec §6](technical_spec.md#propertyt)). Generated code calls `Property::set`, which writes `Local`. The runtime path will call each property's access functions, typed functions the derive emits, which do the same ([loader values](../design/loader-values.md)).

## 5. Attached properties

**Lookup is built. Setting one by name comes with the runtime loader (#58).**

An owner type declares them with `#[ui(attached(...))]`, and a language sets them on other nodes, as XAML's `Grid.Row="1"`. `Registry::attached_property` finds one by owner and name. Its `target` says where it may go: `AttachedTarget::Any`, or `AttachedTarget::ChildrenOfOwner`, the logical children of a node of the owner type, as for `Grid.Row`. The language checks the target and reports a misplaced one.

```rust
# use artic_controls as _;
# use artic_registry::{AttachedTarget, Registry};
# let registry = Registry::from_link_time().unwrap();
let text = registry.type_by_name("artic_controls", "TextElement").found().unwrap();
let size = registry.attached_property(text, "FontSize").expect("TextElement owns FontSize");
assert_eq!(size.target, AttachedTarget::Any);
```

The value lives on the target node, which creates it on first use under the owner's `AttachedKey` (`UiTree::attached_property`), so styles and local values compete as they do for any property.

## 6. Content and children

**Built.**

Children or text that a language gives a type without naming a property go to its content, `TypeInfo::content`:

- `ContentInfo::Property`: a property takes it, set like any property, such as a content control's `Content` once one is built.
- `ContentInfo::Collection`: a field takes child nodes in order through `add_child`, which calls the field's `UiCollection::push_child`: a panel's `Children`, or a `Child` field such as `Border`'s, which takes one.
- `None`: the type takes no content, and the language reports any as an error. `TextBlock` is one for now: its `Text` is an ordinary property, and its inline runs (#47) will be its content.

```rust
# use artic_controls as _;
# use artic_registry::{ContentInfo, Registry};
# let registry = Registry::from_link_time().unwrap();
let panel = registry.type_by_name("artic_controls", "StackPanel").found().unwrap();
assert!(matches!(panel.content, Some(ContentInfo::Collection { .. })));

let border = registry.type_by_name("artic_controls", "Border").found().unwrap();
assert!(matches!(border.content, Some(ContentInfo::Collection { .. })));

let text = registry.type_by_name("artic_controls", "TextBlock").found().unwrap();
assert!(text.content.is_none());
```

Text follows `TypeInfo::whitespace`. With `Collapse`, runs of whitespace become one space and leading and trailing whitespace is dropped; with `Significant`, text stays as written. The language applies it; XAML's rules are in [spec §4](technical_spec.md#whitespace).

A child the tree can't link is `ValueError::InvalidChild`, a second child where one is taken is `ValueError::OneChildOnly`, and a node of the wrong type is `ValueError::WrongNode`.

## 7. Events

**Lookup is built. Subscribing comes with event routing, and binding a handler name to code with `#[ui_class]` (#52).**

`TypeInfo::events` lists each event's name and the Rust path of its arguments (`EventInfo`). A language binds a handler name to a method of the view's code type and checks the method against the arguments type. [Spec §9](technical_spec.md#9-events-routing-commands-code-behind) describes how XAML does it.

## 8. Value providers

**The descriptors are built. Resolving a value provider comes with the runtime loader and the compiler.**

A value provider computes a property's value when the UI is built: a binding, a resource lookup, a type reference, a static item, or null. Its type declares its arguments with `#[ui(value_provider(positional(...)))]`, and `TypeInfo::value_provider` holds them as `ValueProviderInfo`: `named` lists every argument, each a property of the type, and `positional` lists, in order, those that may also be given by position. A markup language writes one inline, as XAML's `{Binding Name, Mode=TwoWay}`; a code language calls it like a function. Resolving one gives one of the four forms in [Properties](#4-properties).

## 9. Language features

**Node names are built with the XAML compiler (#49). The code type and field visibility come with `#[ui_class]` (#52), the data type with #94, resource keys with #111 and typed literals with #62.**

These aren't in the registry. A language offers them in its own syntax, with these meanings:

| Feature | Meaning | In XAML |
| --- | --- | --- |
| Node names | A name the view's code reaches a node by, as a snake_case field | `x:Name` |
| Resource keys | The key of an entry in a resource dictionary | `x:Key` |
| Code type | The Rust type that holds a view's handlers and named nodes, marked `#[ui_class]`, and is the view's root element ([design](../design/ui-class.md)) | `x:Class` |
| Data type | The type compiled bindings are checked against | `x:DataType` |
| Typed literals | A primitive value written with its type | `x:String`, `x:Int32`, `x:Double`, `x:Boolean` |
| Field visibility | The visibility of the field generated for a named node | `x:FieldModifier` (`x:Uid` is accepted and has no meaning yet) |

## 10. Errors

**`Lookup` and `ValueError` are built; positions come with each back end.**

A language reports each of these at its location in the source:

- a type it can't find (`Lookup::Missing`), with the linking hint from [Finding types](#2-finding-types), or an ambiguous one, listing the Rust paths;
- a property, attached property or event the type doesn't have, naming the type;
- a value that doesn't parse or fit (`ValueError`);
- an attached property set where its target rule forbids it;
- content for a type that takes none, or a second child where one is taken.

A language that reads into a `Document` reports its own syntax errors, and records as the document's `Problem`s the types it can't find and the constructs it can't hold yet, such as an attached property. A back end reports a value that doesn't parse or fit. `artic_view::check` reports the rest the same way for every language: an unknown property or an event set as one, a property set twice (text inside an element sets its content property, so `Text` written both ways counts), content a type doesn't take, a second child, and a name that isn't a field or names two elements.

`ValueError` carries no position; the language adds it. A build-time language reports these as errors from `build.rs`; a run-time one returns them to the app.

## 11. XAML's mapping

**The parser, the namespace map and the compiler are built; the runtime loader that also uses this mapping comes with #58.**

Each construct [spec §4](technical_spec.md#supported) supports, and the section it uses:

| XAML | Uses |
| --- | --- |
| Object element, `<Button/>` | [2](#2-finding-types), through the namespace map or `using:`; [3](#3-creating-nodes) |
| Attribute property, `Width="100"` | [4](#4-properties), as text |
| Property element, `<Button.Content>…</Button.Content>` | [4](#4-properties), as an object or a child node |
| Content property, `<Button>Click</Button>` | [6](#6-content-and-children) |
| Collection content, `<StackPanel><A/><B/></StackPanel>` | [6](#6-content-and-children), `ContentInfo::Collection` |
| Attached property, `Grid.Row="1"` | [5](#5-attached-properties) |
| Type-converted text, `Margin="4,8"` | [4](#4-properties), text through `FromStr` |
| Markup extension, `{Binding Path=Name}` | [8](#8-value-providers) |
| Event, `Click="on_click"` | [7](#7-events) |
| `x:Name`, `x:Key`, `x:Class`, `x:DataType`, primitives | [9](#9-language-features) |
| `x:Type`, `x:Static`, `x:Null` | [8](#8-value-providers): a type reference, a `#[ui_static]` item, null |
| `x:Uid`, `x:FieldModifier` | [9](#9-language-features), field visibility |
| XML comments, CDATA, `xml:space` | [6](#6-content-and-children) for whitespace; the rest is XAML syntax with nothing to map |

XAML spells namespaces as URIs. `NamespaceMap` in `artic-xaml` says which crates a URI stands for: `artic::xaml::namespaces()` maps `https://artic.rs/ui` to `artic_element` and `artic_controls`, and an app adds its own URIs with `NamespaceMap::alias`. The map holds only URIs. The path generated code reaches each crate by is set on a `CratePaths` in `artic-view-compiler`, for every UI language: `artic::view::crate_paths()` maps the facade's crates to `::artic::controls` and the like. `using:my_crate::views` joins the module and the name into a Rust path, `my_crate::views::Gauge`, for `Registry::type_by_rust_path`. `https://artic.rs/markup` holds XAML's own `x:` directives.

## 12. What artic gives a language

**`Document`, `check` and `Language` are built (#325), and the compiler takes a `Language`.**

A front end turns its syntax into a `Document` from `artic-view`: each element's type looked up in the registry, each member as written, and each problem it found recorded where it occurs rather than reported. `artic_view::check` finds every member on its type and returns a `CheckedView`, the elements children first, which a back end builds. A language implements `Language`, its file extension and a `read` from a file to a document, and the compiler in `artic-view-compiler` takes it, as the runtime loader will with #58. The [view pipeline design](../design/view-pipeline.md) has the details.
