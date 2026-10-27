//! Derive and attribute macros for artic types, view-models and views.

mod crate_root;
mod element;
mod members;
mod ui_enum;
mod ui_properties;
mod ui_type;

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

/// Describes a struct to the registry, so a UI language can name it.
///
/// # Struct attributes
///
/// - `override_defaults`: `Default` calls `OverrideDefaults::override_defaults` once after building
///   the fields, to change defaults the type doesn't declare, such as a flattened group's.
/// - `whitespace = "significant"`: keep whitespace in text content (the default collapses it).
/// - `crate = "path"`: where generated code finds artic, if neither `artic` nor `artic-registry`
///   is a direct dependency under its usual name.
/// - `attached(name = "Row", ty = usize, default = 0, affects = layout, target = children)`, once
///   per attached property the type owns: a property set on other elements as `Grid.Row`. `name`,
///   `ty` and `target` are required; `target = children` allows it only on the logical children
///   of an element of this type, `target = any` anywhere. The value type is checked like a
///   property's. `inherits` or `inherits = f` works as on a property: a node that holds it follows
///   its nearest logical ancestor that does, and one that doesn't reads that ancestor's value.
/// - `value_provider`, or `value_provider(positional(key, ...))`: the type is a value provider:
///   it computes a property's value when the UI is built, such as a localized string. Every
///   property is a named argument; `positional` lists, in order, the properties also set by
///   position.
///
/// # Fields
///
/// Every `Property<T>` and `Event<A>` field must be marked `#[ui]`:
///
/// - `Property<T>` is a property. `T`, or `X` in `Option<X>`, must implement `FromStr` (with a
///   `Display` error) and `ToRust`, or `UiType`, or be a `NodeId`, a property that names a node, such as a data context. Its
///   settings: `default = expr` for the `Default` level (otherwise `T::default()`),
///   `affects = layout | render | surface` and `inherits` for its generated key, `key = KEY` to
///   share a key with other types instead (inheritance across types needs it): a
///   `static PropertyKey`, or an attached property's key such as `TextElement::FONT_SIZE`, which
///   also gives the field its default, so `default` can't go beside it, and `name = "..."` to
///   rename it (the field name in PascalCase otherwise).
///   A bare `inherits` replaces what the property inherits with its own value; `inherits = f`
///   combines the two through `f(own, inherited)`, where a single name such as `and` is one of
///   `artic_core::property::inherit`'s functions and a longer path names any
///   `fn(&T, &T) -> T`: write your own function as `self::my_fn` or `crate::my_fn`.
/// - `Event<A>` is an event named like a property.
/// - `#[ui(flatten)]` on a field whose type derives `UiProperties`: its properties are
///   listed and visited as the type's own, and the type gets the group's builder methods through
///   the group's `Methods` trait. It takes no other settings. An own property named like a field
///   of a flattened group, or two groups sharing a field name, is an error.
/// - Any other `#[ui]` field is a collection of child elements: it must implement
///   `UiCollection` and be the content, since children reach a type only through its content.
/// - `#[ui(content)]` on one `Property` or collection field: it receives content written inside
///   the element. A flattened group may mark one of its properties instead; a type has at most one
///   content member, counting its groups.
///
/// Unmarked fields are created with their own `Default`.
///
/// # Generated code
///
/// The derive implements `Default`, `Object` (with `visit_properties`) and `UiType`, writes the
/// type's `TypeInfo` with its property, event, attached property and value provider descriptors,
/// and adds it to `UI_TYPES`. So the struct derives `Debug` but not `Default`, and a crate's types
/// only reach the registry if the crate is linked (see `UI_TYPES`).
///
/// It also writes the builder from the builder design: `new()`, the `Default`; one method per
/// `Property` field, named after the field, which sets the `Local` level and takes `impl Into<T>`
/// (the `X` of `Option<X>`), or the number itself for a number type written as `f32` or
/// `core::primitive::f32`, so a literal such as `8.0` needs no suffix; and `build(tree)`, which
/// adds the node and returns a `Handle`. A type whose content is a field written `Child` gets
/// `.child(node)`, and one written `Children` gets `.children(nodes)`; both return a
/// `WithChildren`, whose `build` adds the node and links them in order, moving a child that has a
/// parent. Any other content collection holds values, not child elements, and gets neither. A
/// type alias of a number isn't seen through, so a literal passed to its property needs a suffix
/// (`8.0_f32`). Each property with its own key gets a constant for it, such as `Border::PADDING`;
/// a shared key is named by its owner. A method and a constant have their field's visibility.
///
/// ```
/// # use artic_core::property::Property;
/// # use artic_core::tree::UiTree;
/// # use artic_macros::UiType;
/// #[derive(Debug, UiType)]
/// struct Card {
///   #[ui(default = 1.0)]
///   width: Property<f32>,
/// }
///
/// let mut tree = UiTree::new();
/// let card = Card::new().width(2.0).build(&mut tree);
/// assert_eq!(card.get(&tree).width.get(), 2.0);
/// assert_eq!(Card::WIDTH.name(), "Width");
/// ```
///
/// For each attached property it also writes, on the owner, the key constant and the helper pair
/// from the builder design, which read and write the value on any node:
///
/// ```ignore
/// pub const ROW: &'static AttachedKey<usize>;
/// pub fn set_row(tree: &mut UiTree, node: impl Into<NodeId>, row: usize);
/// pub fn row(tree: &UiTree, node: impl Into<NodeId>) -> usize;
/// ```
///
/// Two of these items with one name are an error, as is a property named `new` or `build`, or a
/// property with its own key named `type_info`, whose constant would hide `UiType::TYPE_INFO`:
/// rename the field, or the attached property with `name = ...`.
#[proc_macro_derive(UiType, attributes(ui))]
pub fn derive_ui_type(input: TokenStream) -> TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  ui_type::expand(&input).unwrap_or_else(syn::Error::into_compile_error).into()
}

/// Makes a struct a property group: a set of properties several UI types share by embedding it
/// in a field marked `#[ui(flatten)]`, as controls embed `Layout`.
///
/// Fields follow `UiType`'s rules for `Property` fields; a group holds no events or
/// collections, and takes no attributes on the struct. One property may be marked
/// `#[ui(content)]`, which makes it the content of every type that flattens the group. The
/// derive implements `Default` and `PropertyGroup`, gives each property with its own key a
/// constant such as `Layout::MARGIN`, and writes beside the group a `{Group}Methods` trait
/// (`LayoutMethods`) with a builder method per field at least as visible as the group,
/// implemented for every type that flattens it. A group field named `new`, `build`, `child` or
/// `children` is an error. A group's keys are its own, so every type that flattens it shares
/// them, and an inheriting property inherits across those types.
#[proc_macro_derive(UiProperties, attributes(ui))]
pub fn derive_ui_properties(input: TokenStream) -> TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  ui_properties::expand(&input)
    .unwrap_or_else(syn::Error::into_compile_error)
    .into()
}

/// Parses a fieldless enum from its variant names, ignoring ASCII case, so a UI language can set it from
/// text: `Orientation="horizontal"` becomes `Orientation::Horizontal`.
///
/// It implements `FromStr` with `Err = ParseEnumError`, whose message lists every valid name, and
/// `ToRust`, which writes the variant's path at the crate root. Every variant must be a unit
/// variant, and no two names may differ only in case. The generated code reaches `ParseEnumError`
/// and `ToRust` through `artic`, `artic-registry` or `artic-core`, whichever the
/// calling crate depends on first, so a crate that has only `artic-core` can derive it.
#[proc_macro_derive(UiEnum)]
pub fn derive_ui_enum(input: TokenStream) -> TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  ui_enum::expand(&input).unwrap_or_else(syn::Error::into_compile_error).into()
}

/// Makes a struct an element: it adds the property groups every control has, writes what
/// `#[derive(UiType)]` writes, and writes `Layoutable` and `Element` around the struct's
/// `ElementImpl`. The struct reaches the cast table, so the frame pipeline finds it with no
/// registration call.
///
/// ```ignore
/// #[element]
/// #[derive(Debug)]
/// pub struct Border {
///   #[ui(content)] pub child: Child,
/// }
///
/// impl ElementImpl for Border { /* measure_override, render, … */ }
/// ```
///
/// `#[element]` goes first, above the struct's `#[derive(...)]` and `#[ui(...)]` attributes,
/// and takes no settings. It adds four fields before the struct's own, each flattened into the
/// struct's properties: `layout: Layout`, `appearance: Appearance`,
/// `interaction: Interaction` and `identity: Identity`. A field of the
/// struct with one of those names is an error, as are `UiType` and `Default` in its derives,
/// since `#[element]` adds a derive that writes both. A unit struct gets the four fields; a tuple struct or a generic
/// one is an error.
///
/// `#[element]` adds `#[derive(UiType)]`, so the struct's `#[ui(...)]` settings are the
/// derive's: a control changes a group's default with `#[ui(override_defaults)]` and an
/// `OverrideDefaults` impl that calls `Property::set_default`.
///
/// Each `Element` method does the groups' shared work, then calls the matching `ElementImpl`
/// hook:
///
/// - `measure` and `arrange`: a collapsed element takes no space and its hooks aren't called;
///   otherwise the margin, size limits and alignment wrap `measure_override` and
///   `arrange_override`.
/// - `render`: calls `ElementImpl::render`.
/// - `hit_test`: a miss unless the element is `Visible` and `IsHitTestVisible`, otherwise its
///   bounds. There is no hook.
/// - `on_event`: `Handled::No` without calling `ElementImpl::on_event` while `IsEnabled` is false,
///   locally or through a logical ancestor.
/// - `accessibility`: the default node, which `ElementImpl::accessibility` adds to.
/// - `layers`: the appearance group's layers, with `ElementImpl::clip` as the clip.
///
/// # State fields
///
/// For each field marked `#[element(state)]`, of type `S`, `#[element]` implements
/// `HasState<S>`, and the traits built on that state reach the struct through their blanket
/// impls. A struct holds at most one field per state type: two fields whose types are written the
/// same way are an error naming both, and two names for one type, such as an alias, are rustc's
/// conflicting-implementations error (E0119).
///
/// Generated code reaches artic through `artic`, or `artic-element` if the crate doesn't depend
/// on `artic`; `#[ui(crate = path)]` names another path.
#[proc_macro_attribute]
pub fn element(attribute: TokenStream, item: TokenStream) -> TokenStream {
  element::expand(attribute.into(), item.into())
    .unwrap_or_else(syn::Error::into_compile_error)
    .into()
}
