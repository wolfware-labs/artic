use crate::{PropertyAccess, ValueError};
use artic_core::property::{Affects, PropertyVisitor};
use artic_core::tree::{NodeId, UiTree};
use artic_core::{Object, RustExpr};

/// What the registry knows about one Rust type that UI languages can name.
///
/// Every field is plain data that can be built at compile time, so a descriptor is a `static` item:
/// the `UiType` derive emits one per type, and a type can also be described by hand.
#[derive(Debug)]
pub struct TypeInfo {
  /// The name UI languages use for the type, such as `Border`. Lookups are exact and case-sensitive.
  pub name: &'static str,
  /// The full Rust path where the type is defined, such as `artic_controls::border::Border`; a
  /// re-export doesn't change it.
  pub rust_path: &'static str,
  /// Whether a node's object is of this type.
  pub is: fn(&dyn Object) -> bool,
  /// Creates a node of this type with its default values and returns its id, or `None` for a type
  /// that can't be created from markup alone, such as one with a `#[ui(required)]` field.
  pub new: Option<fn(&mut UiTree) -> NodeId>,
  /// Whether the type's objects stay in the tree, or are values moved into a property.
  pub kind: TypeKind,
  /// What receives content written directly inside the element, if anything.
  pub content: Option<ContentInfo>,
  /// The type's settable properties.
  pub properties: &'static [PropertyInfo],
  /// The property groups the type embeds, whose properties are listed as the type's.
  pub groups: &'static [PropertyGroupInfo],
  /// The type's events.
  pub events: &'static [EventInfo],
  /// The attached properties this type owns, set on other elements as `Owner.Name`.
  pub attached_properties: &'static [AttachedPropertyInfo],
  /// The arguments the type takes when used as a value provider, or `None` if it isn't one.
  pub value_provider: Option<&'static ValueProviderInfo>,
  /// How the parser treats whitespace in text written inside the element.
  pub whitespace: Whitespace,
  /// Whether a view's code type can take this type's place at the root of the view, as a struct
  /// with this type's members.
  pub code_type_root: bool,
}

/// How a UI language treats whitespace-only text between two child elements, such as the space
/// between two runs of a text. Other text follows the language's own rules either way.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub enum Whitespace {
  /// It adds nothing, as indentation between elements shouldn't.
  #[default]
  Collapse,
  /// It counts as one space, as between words in a text's inlines.
  Significant,
}

/// A Rust type that UI languages can name, described by a [`TypeInfo`].
///
/// `#[derive(UiType)]` implements it. A type that implements it can also be the value of a
/// property, written as an object rather than as text.
pub trait UiType {
  /// The type's registry descriptor.
  const TYPE_INFO: &'static TypeInfo;
}

/// The crate a Rust path starts in: its first segment, such as `artic_controls` in
/// `artic_controls::border::Border`.
#[must_use]
pub fn crate_of(path: &str) -> &str {
  path.split_once("::").map_or(path, |(krate, _)| krate)
}

/// The crate and the item of a Rust path, its first and last segments: `artic_controls` and
/// `Border` in `artic_controls::border::Border`. A UI crate re-exports each item at its root, so the
/// two reach it.
#[must_use]
pub fn crate_and_item(path: &str) -> (&str, &str) {
  (crate_of(path), path.rsplit_once("::").map_or(path, |(_, item)| item))
}

impl TypeInfo {
  /// A descriptor of a [`TypeKind::Node`] type named `name`, defined at `rust_path`, whose nodes
  /// `is` recognizes, usable in a `static`. Every other field starts empty: no
  /// [`new`](TypeInfo::new), so markup alone can't create it, no content, properties, groups,
  /// events or attached properties, not a value provider, [`Whitespace::Collapse`], and no
  /// [`code_type_root`](TypeInfo::code_type_root). The `with_*` methods set each of these but
  /// [`value_provider`](TypeInfo::value_provider), which only the derive fills.
  #[must_use]
  pub const fn node(name: &'static str, rust_path: &'static str, is: fn(&dyn Object) -> bool) -> Self {
    Self {
      name,
      rust_path,
      is,
      new: None,
      kind: TypeKind::Node,
      content: None,
      properties: &[],
      groups: &[],
      events: &[],
      attached_properties: &[],
      value_provider: None,
      whitespace: Whitespace::Collapse,
      code_type_root: false,
    }
  }

  /// The descriptor, with `new` creating its nodes.
  #[must_use]
  pub const fn with_new(mut self, new: fn(&mut UiTree) -> NodeId) -> Self {
    self.new = Some(new);
    self
  }

  /// The descriptor, with its objects of kind `kind`.
  #[must_use]
  pub const fn with_kind(mut self, kind: TypeKind) -> Self {
    self.kind = kind;
    self
  }

  /// The descriptor, with `content` receiving what is written inside the element.
  #[must_use]
  pub const fn with_content(mut self, content: ContentInfo) -> Self {
    self.content = Some(content);
    self
  }

  /// The descriptor, with `properties` as its own properties.
  #[must_use]
  pub const fn with_properties(mut self, properties: &'static [PropertyInfo]) -> Self {
    self.properties = properties;
    self
  }

  /// The descriptor, embedding `groups`.
  #[must_use]
  pub const fn with_groups(mut self, groups: &'static [PropertyGroupInfo]) -> Self {
    self.groups = groups;
    self
  }

  /// The descriptor, with `events` as its events.
  #[must_use]
  pub const fn with_events(mut self, events: &'static [EventInfo]) -> Self {
    self.events = events;
    self
  }

  /// The descriptor, owning `attached_properties`.
  #[must_use]
  pub const fn with_attached_properties(mut self, attached_properties: &'static [AttachedPropertyInfo]) -> Self {
    self.attached_properties = attached_properties;
    self
  }

  /// The descriptor, treating whitespace inside the element as `whitespace` says.
  #[must_use]
  pub const fn with_whitespace(mut self, whitespace: Whitespace) -> Self {
    self.whitespace = whitespace;
    self
  }

  /// The descriptor, with a view's code type able to take its place at the root when
  /// `code_type_root` is `true`.
  #[must_use]
  pub const fn with_code_type_root(mut self, code_type_root: bool) -> Self {
    self.code_type_root = code_type_root;
    self
  }

  /// The type's namespace: the crate it's defined in, the first segment of its
  /// [`rust_path`](TypeInfo::rust_path), such as `artic_controls`. A language reaches types by
  /// namespace and name, or by Rust path.
  #[must_use]
  pub fn namespace(&self) -> &'static str {
    crate_of(self.rust_path)
  }

  /// The type's property named `name`: one of its own, or one from a group it embeds.
  #[must_use]
  pub fn property(&self, name: &str) -> Option<PropertyRef> {
    let own = self
      .properties
      .iter()
      .find(|info| info.name == name)
      .map(|info| PropertyRef { group: None, info });
    own.or_else(|| {
      self.groups.iter().find_map(|group| {
        let info = group.properties.iter().find(|info| info.name == name)?;
        Some(PropertyRef {
          group: Some(group.field),
          info,
        })
      })
    })
  }

  /// The attached property named `name` the type owns, such as `Row` on `Grid`.
  #[must_use]
  pub fn attached_property(&self, name: &str) -> Option<&'static AttachedPropertyInfo> {
    self.attached_properties.iter().find(|info| info.name == name)
  }

  /// Whether the type can be used as a value provider, such as a binding.
  #[must_use]
  pub const fn is_value_provider(&self) -> bool {
    self.value_provider.is_some()
  }
}

/// Whether a type's objects live in the tree or are values a property holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
  /// The object stays in the tree: it has a parent, and takes part in layout, input or rendering.
  Node,
  /// The object is staged in the tree while its members are set, then moved into the property
  /// that holds it, as a brush is: a [`UiValue`](crate::UiValue).
  Value,
}

/// A settable property of a type.
///
/// `name`, `affects` and `inherits` repeat the property's
/// [`PropertyKey`](artic_core::property::PropertyKey), copied at compile time through its `const`
/// accessors, so the key stays the single source of that metadata.
#[derive(Debug)]
pub struct PropertyInfo {
  /// The property's name in UI languages, such as `Padding`.
  pub name: &'static str,
  /// The Rust field that holds the property, such as `padding`.
  pub field: &'static str,
  /// The Rust path of the property's value type, such as `artic_layout::Thickness`.
  pub value_type: &'static str,
  /// The frame stages a change to the property has to redo.
  pub affects: Affects,
  /// Whether the property takes its value from a logical ancestor when it has none of its own.
  pub inherits: bool,
  /// Parses text for the property and writes the Rust expression that builds the value, or
  /// returns why the text doesn't parse. For a `Property<Option<V>>` it writes a `V`, which is
  /// what the builder method takes. `Some` only when this crate's `literals` feature is on, which
  /// a build-time code generator turns on; an app's registry holds `None`.
  pub literal: Option<WriteLiteral>,
  /// How a UI language's runtime loader writes the property. `Some` only when this crate's
  /// `access` feature is on, which a runtime loader turns on; an app without one keeps no thunks.
  pub access: Option<&'static PropertyAccess>,
}

impl PropertyInfo {
  /// A property named `name`, held in `field`, whose value is of the type at `value_type`, usable
  /// in a `static`. It starts with [`Affects::NONE`], doesn't inherit, and has no
  /// [`literal`](PropertyInfo::literal) or [`access`](PropertyInfo::access). The `with_*` methods
  /// set the others.
  #[must_use]
  pub const fn new(name: &'static str, field: &'static str, value_type: &'static str) -> Self {
    Self {
      name,
      field,
      value_type,
      affects: Affects::NONE,
      inherits: false,
      literal: None,
      access: None,
    }
  }

  /// The property, with a change to it redoing the stages `affects` names.
  #[must_use]
  pub const fn with_affects(mut self, affects: Affects) -> Self {
    self.affects = affects;
    self
  }

  /// The property, taking its value from a logical ancestor when `inherits` is `true`.
  #[must_use]
  pub const fn with_inherits(mut self, inherits: bool) -> Self {
    self.inherits = inherits;
    self
  }

  /// The property, with `literal` writing the Rust expression for its text.
  #[must_use]
  pub const fn with_literal(mut self, literal: WriteLiteral) -> Self {
    self.literal = Some(literal);
    self
  }

  /// The property, written by a runtime loader through `access`.
  #[must_use]
  pub const fn with_access(mut self, access: &'static PropertyAccess) -> Self {
    self.access = Some(access);
    self
  }
}

/// Parses a property's text and writes the Rust expression that builds the value, or returns why
/// the text doesn't parse. [`PropertyInfo::literal`] holds one.
pub type WriteLiteral = fn(&str, &mut RustExpr) -> Result<(), String>;

/// Properties several types share by embedding one struct, such as `Layout`.
///
/// `#[derive(UiProperties)]` implements it. A type lists a group's properties as its own by
/// marking the field `#[ui(flatten)]`.
pub trait PropertyGroup {
  /// The group's properties.
  const PROPERTIES: &'static [PropertyInfo];

  /// The group's property marked `#[ui(content)]`, if any.
  const CONTENT: Option<&'static PropertyInfo> = None;

  /// The path of the group's methods trait, such as `artic_element::properties::LayoutMethods`,
  /// built like [`TypeInfo::rust_path`]: the module the trait is defined in, which a re-export
  /// doesn't change, so it may name a private module. Generated code turns it into a path it can
  /// import the same way it does a type's `rust_path`.
  const METHODS: &'static str;

  /// Lists the group's properties, as [`Object::visit_properties`] does for a node.
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>);
}

/// A node that holds the property group `G` in a `#[ui(flatten)]` field. `#[derive(UiType)]`
/// implements it, and the group's methods trait reaches the group through it.
pub trait HasGroup<G> {
  /// The group.
  fn group(&self) -> &G;
}

/// A property group a type embeds, whose properties are listed as the type's.
#[derive(Debug)]
pub struct PropertyGroupInfo {
  /// The Rust field that holds the group, such as `layout`.
  pub field: &'static str,
  /// The group's properties.
  pub properties: &'static [PropertyInfo],
  /// The defining path of the group's methods trait, from [`PropertyGroup::METHODS`], such as
  /// `artic_element::properties::LayoutMethods`. It may name a private module; the trait is
  /// reachable at its crate's root, so generated code takes the crate (the first segment) and the
  /// trait (the last).
  pub methods: &'static str,
}

/// What a type takes as content: a property, or a collection field of child elements.
#[derive(Debug, Clone, Copy)]
pub enum ContentInfo {
  /// A property that takes the content as its value, set through its property access.
  Property(PropertyRef),
  /// A collection field that takes child elements.
  Collection {
    /// The collection field.
    field: &'static str,
    /// What the builder method named after the field takes, or `None` for a collection of
    /// values, which has no builder method, so only `add_child` fills it.
    method: Option<ContentMethod>,
    /// The types of child the collection takes, or `None` for any: `UiCollection::takes`.
    takes: fn() -> Option<&'static [&'static TypeInfo]>,
    /// The type text written inside the collection becomes, or `None` when it takes no text:
    /// `UiCollection::text_wrapper`.
    text_wrapper: fn() -> Option<&'static TypeInfo>,
    /// Adds one child element as content, calling the field's `UiCollection::add_child` with
    /// an accessor to the field.
    add_child: fn(&mut UiTree, NodeId, NodeId) -> Result<(), ValueError>,
  },
}

/// What the builder method of a collection content takes. The method is named after the field,
/// such as `children` or `inlines`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentMethod {
  /// One child, for a field of type `Child`.
  Child,
  /// Any number of children, in order.
  Children,
}

/// A property found on a type, with the field of the group it lives in, if any.
#[derive(Debug, Clone, Copy)]
pub struct PropertyRef {
  /// The group's field, such as `layout`, or `None` for the type's own property.
  pub group: Option<&'static str>,
  /// The property.
  pub info: &'static PropertyInfo,
}

/// An event of a type.
#[derive(Debug)]
pub struct EventInfo {
  /// The event's name in UI languages, such as `PointerPressed`.
  pub name: &'static str,
  /// The Rust path of the arguments handlers receive.
  pub args_type: &'static str,
}

/// A property owned by one type and set on elements of other types, such as `Grid.Row`.
#[derive(Debug)]
pub struct AttachedPropertyInfo {
  /// The property's name after the owner, such as `Row` in `Grid.Row`.
  pub name: &'static str,
  /// The Rust path of the property's value type.
  pub value_type: &'static str,
  /// Which elements it may be set on.
  pub target: AttachedTarget,
  /// Parses text for the property and writes the Rust expression that builds the value the
  /// owner's setter takes, as [`PropertyInfo::literal`] does; for an `Option<V>` property, `Some`
  /// of the `V`. `Some` only with this crate's `literals` feature.
  pub literal: Option<WriteLiteral>,
}

/// Which elements an attached property may be set on.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AttachedTarget {
  /// Any element, such as `Accessibility.Name`.
  Any,
  /// The logical children of an element of the owner type, such as `Grid.Row` on a grid's
  /// children.
  ChildrenOfOwner,
}

/// The arguments a value provider accepts.
#[derive(Debug)]
pub struct ValueProviderInfo {
  /// Arguments given by position, in order, such as a binding's path.
  pub positional: &'static [ArgInfo],
  /// Arguments given by name, such as a binding's `Path`.
  pub named: &'static [ArgInfo],
}

/// One argument of a value provider.
#[derive(Debug)]
pub struct ArgInfo {
  /// The argument's name.
  pub name: &'static str,
  /// The Rust path of the argument's value type.
  pub value_type: &'static str,
}

#[cfg(test)]
mod tests {
  use super::*;
  use artic_core::Object;
  use artic_core::property::PropertyVisitor;
  use artic_core::property::{Inheritance, Property, PropertyKey};
  use pretty_assertions::assert_eq;
  use static_assertions::assert_impl_all;

  assert_impl_all!(TypeInfo: Sync);

  // A Rust path's crate is its first segment.
  #[test]
  fn crate_of_is_the_first_segment() {
    assert_eq!(crate_of("artic_controls::border::Border"), "artic_controls");
    assert_eq!(crate_of("acme"), "acme");
  }

  // A Rust path's crate and item are its first and last segments.
  #[test]
  fn crate_and_item_reads_both_ends() {
    assert_eq!(crate_and_item("artic_controls::border::Border"), ("artic_controls", "Border"));
    assert_eq!(crate_and_item("acme"), ("acme", "acme"));
  }

  static PADDING: PropertyKey<f32> = PropertyKey::new("Padding", Affects::LAYOUT, Inheritance::None);

  #[derive(Debug)]
  struct Border {
    _padding: Property<f32>,
  }

  impl Object for Border {
    fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
  }

  fn new_border(tree: &mut UiTree) -> NodeId {
    tree.alloc(Box::new(Border {
      _padding: Property::new(&PADDING, 0.0),
    }))
  }

  static BORDER: TypeInfo = TypeInfo::node("Border", "artic_controls::Border", crate::is_a::<Border>)
    .with_new(new_border)
    .with_properties(&[PropertyInfo::new(PADDING.name(), "padding", "f32")
      .with_affects(PADDING.affects())
      .with_inherits(PADDING.inherits())])
    .with_events(&[EventInfo {
      name: "PointerPressed",
      args_type: "artic_core::PointerEvent",
    }]);

  // A descriptor written as a `static` copies its property metadata from the property's key at
  // compile time, and its constructor creates a node.
  #[test]
  fn static_descriptor_reads_metadata_from_the_key() {
    let mut tree = UiTree::new();

    let node = (BORDER.new.expect("created from defaults"))(&mut tree);

    assert_eq!(BORDER.properties[0].name, "Padding");
    assert_eq!(BORDER.properties[0].affects, Affects::LAYOUT);
    assert!(!BORDER.is_value_provider());
    assert!(tree.get_as::<Border>(node).is_some());
  }

  static DEFAULT_TYPE: TypeInfo = TypeInfo::node("Gauge", "app::Gauge", crate::is_a::<Border>);
  static DEFAULT_PROPERTY: PropertyInfo = PropertyInfo::new("Text", "text", "String");

  // The constructors work in a `static` and leave every other field at its documented default.
  #[test]
  fn constructors_fill_the_defaults() {
    assert_eq!((DEFAULT_TYPE.name, DEFAULT_TYPE.rust_path), ("Gauge", "app::Gauge"));
    assert!(DEFAULT_TYPE.new.is_none());
    assert_eq!(DEFAULT_TYPE.kind, TypeKind::Node);
    assert!(DEFAULT_TYPE.content.is_none());
    assert!(DEFAULT_TYPE.properties.is_empty());
    assert!(DEFAULT_TYPE.groups.is_empty());
    assert!(DEFAULT_TYPE.events.is_empty());
    assert!(DEFAULT_TYPE.attached_properties.is_empty());
    assert!(DEFAULT_TYPE.value_provider.is_none());
    assert_eq!(DEFAULT_TYPE.whitespace, Whitespace::Collapse);
    assert!(!DEFAULT_TYPE.code_type_root);

    assert_eq!(
      (DEFAULT_PROPERTY.name, DEFAULT_PROPERTY.field, DEFAULT_PROPERTY.value_type),
      ("Text", "text", "String")
    );
    assert_eq!(DEFAULT_PROPERTY.affects, Affects::NONE);
    assert!(!DEFAULT_PROPERTY.inherits);
    assert!(DEFAULT_PROPERTY.literal.is_none());
    assert!(DEFAULT_PROPERTY.access.is_none());
  }

  /// Writes text as a string.
  fn quoted(text: &str, out: &mut RustExpr) -> Result<(), String> {
    out.str(text);
    Ok(())
  }

  static GROUPS: [PropertyGroupInfo; 1] = [PropertyGroupInfo {
    field: "layout",
    properties: &[],
    methods: "artic_element::LayoutMethods",
  }];

  static SET_TYPE: TypeInfo = TypeInfo::node("Shelf", "app::Shelf", crate::is_a::<Border>)
    .with_new(new_border)
    .with_kind(TypeKind::Value)
    .with_content(ContentInfo::Property(PropertyRef {
      group: None,
      info: &SET_PROPERTY,
    }))
    .with_properties(std::slice::from_ref(&SET_PROPERTY))
    .with_groups(&GROUPS)
    .with_events(&[EventInfo {
      name: "PointerPressed",
      args_type: "artic_core::PointerEvent",
    }])
    .with_attached_properties(&[AttachedPropertyInfo {
      name: "Slot",
      value_type: "usize",
      target: AttachedTarget::Any,
      literal: None,
    }])
    .with_whitespace(Whitespace::Significant)
    .with_code_type_root(true);
  static SET_PROPERTY: PropertyInfo = PropertyInfo::new("Text", "text", "String")
    .with_affects(Affects::LAYOUT)
    .with_inherits(true)
    .with_literal(quoted)
    .with_access(&crate::access::tests::UNWRITTEN);

  // Each setter sets its own field, and a chain of them keeps the fields set before it.
  #[test]
  fn setters_set_their_field() {
    let mut tree = UiTree::new();
    let mut out = RustExpr::new();

    let node = (SET_TYPE.new.expect("a constructor"))(&mut tree);
    (SET_PROPERTY.literal.expect("a literal"))("Hi", &mut out).expect("any text");

    assert!(tree.get_as::<Border>(node).is_some());
    assert_eq!(SET_TYPE.kind, TypeKind::Value);
    assert!(matches!(SET_TYPE.content, Some(ContentInfo::Property(found)) if found.info.name == "Text"));
    assert_eq!(SET_TYPE.properties.len(), 1);
    assert_eq!(SET_TYPE.groups[0].field, "layout");
    assert_eq!(SET_TYPE.events[0].name, "PointerPressed");
    assert_eq!(SET_TYPE.attached_properties[0].name, "Slot");
    assert_eq!(SET_TYPE.whitespace, Whitespace::Significant);
    assert!(SET_TYPE.code_type_root);
    assert_eq!(SET_PROPERTY.affects, Affects::LAYOUT);
    assert!(SET_PROPERTY.inherits);
    assert!(
      SET_PROPERTY
        .access
        .is_some_and(|access| std::ptr::eq(access, &crate::access::tests::UNWRITTEN))
    );
    assert_eq!(out.as_str(), r#""Hi""#);
  }

  // An attached property is found by its name among the ones its owner declares, and only there.
  #[test]
  fn attached_property_is_found_by_name() {
    static DOCK: TypeInfo = TypeInfo::node("Dock", "artic_controls::Dock", crate::is_a::<Border>).with_attached_properties(&[
      AttachedPropertyInfo {
        name: "Side",
        value_type: "artic_controls::Side",
        target: AttachedTarget::ChildrenOfOwner,
        literal: None,
      },
      AttachedPropertyInfo {
        name: "Order",
        value_type: "usize",
        target: AttachedTarget::Any,
        literal: None,
      },
    ]);

    assert_eq!(DOCK.attached_property("Order").map(|info| info.value_type), Some("usize"));
    assert!(DOCK.attached_property("Padding").is_none());
    assert!(BORDER.attached_property("Side").is_none());
  }
}
