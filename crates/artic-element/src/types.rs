//! The table that finds a node's [`Element`] from its `Box<dyn Object>`.
//!
//! Rust can't downcast a `&dyn Object` to a trait, only to a concrete type, so each element type
//! adds an [`ElementType`] with its casts to [`ELEMENT_TYPES`], and [`ElementTypes`] looks them up
//! by `TypeId`.

use crate::Element;
use artic_core::Object;
use artic_core::tree::{NodeId, UiTree};
use artic_layout::{LayoutNodes, Layoutable};
use linkme::distributed_slice;
use std::any::{Any, TypeId};
use std::collections::HashMap;

/// How to view one element type's nodes as [`Element`]s.
#[derive(Debug)]
pub struct ElementType {
  /// The type's `TypeId`.
  pub type_id: fn() -> TypeId,
  /// The node as an `Element`, or `None` if it isn't of this type.
  pub cast: fn(&dyn Object) -> Option<&dyn Element>,
  /// The node as a mutable `Element`, or `None` if it isn't of this type.
  pub cast_mut: fn(&mut dyn Object) -> Option<&mut dyn Element>,
}

/// Every element type linked into the program. [`element_type!`](crate::element_type) adds one;
/// `#[element]` does it for each control.
#[distributed_slice]
pub static ELEMENT_TYPES: [ElementType];

/// Adds an element type to [`ELEMENT_TYPES`], so [`ElementTypes`] finds its nodes.
///
/// `#[element]` writes this for each control; a hand-written [`Element`] calls it once.
/// The calling crate needs no `linkme` dependency, and needn't import `Any` or `Element`.
#[macro_export]
macro_rules! element_type {
  ($ty:ty) => {
    const _: () = {
      #[$crate::__private::linkme::distributed_slice($crate::__private::ELEMENT_TYPES)]
      #[linkme(crate = $crate::__private::linkme)]
      static ENTRY: $crate::__private::ElementType = $crate::__private::ElementType {
        type_id: ::core::any::TypeId::of::<$ty>,
        cast: |object| {
          (object as &dyn ::core::any::Any)
            .downcast_ref::<$ty>()
            .map(|element| element as &dyn $crate::__private::Element)
        },
        cast_mut: |object| {
          (object as &mut dyn ::core::any::Any)
            .downcast_mut::<$ty>()
            .map(|element| element as &mut dyn $crate::__private::Element)
        },
      };
    };
  };
}

/// Finds a node's [`Element`], for every type in [`ELEMENT_TYPES`].
///
/// It answers [`LayoutNodes`], so [`LayoutManager`](artic_layout::LayoutManager) lays out any
/// registered element. A node whose type isn't registered has no `Element`, and layout gives it
/// no space.
#[derive(Debug)]
pub struct ElementTypes {
  by_type: HashMap<TypeId, &'static ElementType>,
}

impl ElementTypes {
  /// Indexes every element type linked into the program. A type added twice is indexed once.
  #[must_use]
  pub fn from_link_time() -> Self {
    let by_type = ELEMENT_TYPES.iter().map(|entry| ((entry.type_id)(), entry)).collect();
    Self { by_type }
  }

  /// `object` as an [`Element`], or `None` if its type isn't registered.
  #[must_use]
  pub fn element<'a>(&self, object: &'a dyn Object) -> Option<&'a dyn Element> {
    let type_id = (object as &dyn Any).type_id();
    (self.by_type.get(&type_id)?.cast)(object)
  }

  /// `object` as a mutable [`Element`], or `None` if its type isn't registered.
  #[must_use]
  pub fn element_mut<'a>(&self, object: &'a mut dyn Object) -> Option<&'a mut dyn Element> {
    let type_id = (&*object as &dyn Any).type_id();
    (self.by_type.get(&type_id)?.cast_mut)(object)
  }
}

impl LayoutNodes for ElementTypes {
  fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
    let element = self.element(tree.get(node)?)?;
    Some(element as &dyn Layoutable)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{AccessCx, RenderCx};
  use artic_core::property::PropertyVisitor;
  use artic_layout::geometry::{Rect, Size};
  use artic_layout::{ArrangeCx, MeasureCx};
  use artic_render::SceneBuilder;
  use pretty_assertions::assert_eq;

  /// A registered element.
  #[derive(Debug)]
  struct Dot;

  impl Object for Dot {
    fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
  }

  impl Layoutable for Dot {
    fn measure(&self, _cx: &mut MeasureCx<'_>, _available: Size) -> Size {
      Size::new(1.0, 1.0)
    }

    fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}
  }

  impl Element for Dot {
    fn render(&self, _cx: &mut RenderCx<'_>, _scene: &mut SceneBuilder) {}

    fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
      accesskit::Node::new(accesskit::Role::GenericContainer)
    }
  }

  crate::element_type!(Dot);
  crate::element_type!(Dot);

  /// A node type that isn't an element.
  #[derive(Debug)]
  struct Plain;

  impl Object for Plain {
    fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
  }

  // A registered type is found, by shared and mutable reference; another type isn't.
  #[test]
  fn finds_registered_types_only() {
    let types = ElementTypes::from_link_time();
    let mut dot = Dot;
    let plain = Plain;

    assert!(types.element(&dot).is_some());
    assert!(types.element_mut(&mut dot).is_some());
    assert!(types.element(&plain).is_none());
  }

  // A cast answers only for its own type, and a type added twice is indexed once.
  #[test]
  fn casts_are_typed_and_duplicates_collapse() {
    let types = ElementTypes::from_link_time();
    let entry = types.by_type[&TypeId::of::<Dot>()];

    assert!((entry.cast)(&Plain).is_none());
    assert_eq!(
      ELEMENT_TYPES
        .iter()
        .filter(|entry| (entry.type_id)() == TypeId::of::<Dot>())
        .count(),
      2
    );
    assert_eq!(types.by_type.len(), ELEMENT_TYPES.len() - 1);
  }
}
