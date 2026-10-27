//! The [`Object`] trait that every node type in the tree implements.

use crate::property::{AnyProperty, PropertyVisitor};
use std::any::Any;
use std::fmt::Debug;

/// A value that can be stored as a node in a [`UiTree`](crate::tree::UiTree).
///
/// The tree stores nodes as `Box<dyn Object>`, so every node type has to implement this trait. It
/// bundles the supertraits the tree relies on:
///
/// - [`Any`] lets the tree give a node back as its concrete type, through
///   [`UiTree::get_as`](crate::tree::UiTree::get_as) and
///   [`UiTree::get_as_mut`](crate::tree::UiTree::get_as_mut). Because `Any` requires `'static`, node
///   types can't borrow non-`'static` data.
/// - [`Debug`] lets the tree itself implement `Debug`.
///
/// Its one method, [`visit_properties`](Object::visit_properties), lists the node's
/// [`Property`](crate::property::Property) fields, so the tree can mark the node dirty when one of them
/// changes and make inheritable ones follow the same property on a logical ancestor. A node type without
/// properties implements it with an empty body.
///
/// # Examples
///
/// A node without properties needs only the empty `impl`:
///
/// ```
/// use artic_core::Object;
/// use artic_core::property::PropertyVisitor;
/// use artic_core::tree::UiTree;
///
/// #[derive(Debug)]
/// struct Label {
///   text: String,
/// }
///
/// impl Object for Label {
///   fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
/// }
///
/// let mut tree = UiTree::new();
/// let id = tree.alloc(Box::new(Label { text: "hello".into() }));
///
/// assert_eq!(tree.get_as::<Label>(id).map(|label| label.text.as_str()), Some("hello"));
/// ```
///
/// A node with properties visits each one, and changes to them then mark it dirty:
///
/// ```
/// use artic_core::Object;
/// use artic_core::property::{Affects, PropertyVisitor, Inheritance, Property, PropertyKey};
/// use artic_core::tree::UiTree;
///
/// static FONT_SIZE: PropertyKey<f32> = PropertyKey::new("FontSize", Affects::LAYOUT, Inheritance::Replace);
///
/// #[derive(Debug)]
/// struct TextBlock {
///   font_size: Property<f32>,
/// }
///
/// impl Object for TextBlock {
///   fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
///     visitor.visit(&self.font_size);
///   }
/// }
///
/// let font_size = Property::new(&FONT_SIZE, 12.0);
/// let mut tree = UiTree::new();
/// let id = tree.alloc(Box::new(TextBlock { font_size }));
/// tree.clear_dirty(id, Affects::LAYOUT | Affects::RENDER);
///
/// font_size.set(16.0);
/// assert_eq!(tree.dirty(id), Affects::LAYOUT);
/// ```
pub trait Object: Debug + Any {
  /// Calls `visit` with each of the node's [`Property`](crate::property::Property) fields.
  ///
  /// The tree uses it to connect the properties to their node when the node is allocated, so that
  /// changes mark the node dirty, and to find a property by its key. A node type without properties
  /// implements it with an empty body. It has no default on purpose: a type that has properties
  /// but forgot to visit them would compile and silently never update.
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>);
}

/// Calls `action` with each property `object` lists.
pub(crate) fn for_each_property(object: &dyn Object, mut action: impl FnMut(AnyProperty<'_>)) {
  object.visit_properties(&mut PropertyVisitor::new(&mut action));
}
