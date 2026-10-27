use crate::ValueError;
use artic_core::RustExpr;
use artic_core::property::{Affects, PropertyVisitor};
use artic_core::tree::{NodeId, UiTree};

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
  /// Creates a node of this type with its default values and returns its id.
  pub new: fn(&mut UiTree) -> NodeId,
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
}

/// How the parser treats whitespace in text content.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub enum Whitespace {
  /// Runs of whitespace collapse to one space, and leading and trailing whitespace is dropped.
  #[default]
  Collapse,
  /// Text keeps its whitespace as written.
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

impl TypeInfo {
  /// The type's namespace: the crate it's defined in, the first segment of its
  /// [`rust_path`](TypeInfo::rust_path), such as `artic_controls`. A language reaches types by
  /// namespace and name, or by Rust path.
  #[must_use]
  pub fn namespace(&self) -> &'static str {
    self
      .rust_path
      .split_once("::")
      .map_or(self.rust_path, |(crate_name, _)| crate_name)
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

  /// Whether the type can be used as a value provider, such as a binding.
  #[must_use]
  pub const fn is_value_provider(&self) -> bool {
    self.value_provider.is_some()
  }
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

  /// Lists the group's properties, as [`Object::visit_properties`](artic_core::Object::visit_properties)
  /// does for a node.
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
    /// The builder method that takes the children, or `None` for a collection that isn't
    /// `Child` or `Children`, which has no builder method, so only `add_child` fills it.
    method: Option<ContentMethod>,
    /// Adds one child element as content, calling the field's `UiCollection::push_child` with
    /// an accessor to the field.
    add_child: fn(&mut UiTree, NodeId, NodeId) -> Result<(), ValueError>,
  },
}

/// The builder method that takes a collection content's children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentMethod {
  /// `child`, for a field of type `Child`: one child.
  Child,
  /// `children`, for a field of type `Children`: any number, in order.
  Children,
}

impl ContentMethod {
  /// The method's name: `child` or `children`.
  #[must_use]
  pub const fn name(self) -> &'static str {
    match self {
      ContentMethod::Child => "child",
      ContentMethod::Children => "children",
    }
  }
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

  // Each content method is named as the builder method the derive writes.
  #[test]
  fn content_method_names_its_builder_method() {
    assert_eq!(ContentMethod::Child.name(), "child");
    assert_eq!(ContentMethod::Children.name(), "children");
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

  static BORDER: TypeInfo = TypeInfo {
    name: "Border",
    rust_path: "artic_controls::Border",
    new: new_border,
    content: None,
    properties: &[PropertyInfo {
      name: PADDING.name(),
      field: "padding",
      value_type: "f32",
      affects: PADDING.affects(),
      inherits: PADDING.inherits(),
      literal: None,
    }],
    groups: &[],
    events: &[EventInfo {
      name: "PointerPressed",
      args_type: "artic_core::PointerEvent",
    }],
    attached_properties: &[],
    value_provider: None,
    whitespace: Whitespace::Collapse,
  };

  // A descriptor written as a `static` copies its property metadata from the property's key at
  // compile time, and its constructor creates a node.
  #[test]
  fn static_descriptor_reads_metadata_from_the_key() {
    let mut tree = UiTree::new();

    let node = (BORDER.new)(&mut tree);

    assert_eq!(BORDER.properties[0].name, "Padding");
    assert_eq!(BORDER.properties[0].affects, Affects::LAYOUT);
    assert!(!BORDER.is_value_provider());
    assert!(tree.get_as::<Border>(node).is_some());
  }
}
