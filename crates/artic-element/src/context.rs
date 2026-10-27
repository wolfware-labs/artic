//! What an element receives while it's drawn, hit-tested, sent input or described to assistive
//! technology.
//!
//! These are placeholders until rendering, input routing and accessibility fill them in (M3); each
//! names its node and carries the little the trait defaults need.

use artic_core::tree::{NodeId, UiTree};
use artic_layout::geometry::{Point, Rect, Size};
use std::any::Any;
use std::marker::PhantomData;

/// Whether an element handled an input event, which stops it from going further.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Handled {
  /// The event goes on to the next element on its route.
  No,
  /// The event stops here.
  Yes,
}

/// Whether a point hits an element.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum HitTestResult {
  /// The point is outside the element, or the element doesn't take input there.
  Miss,
  /// The point is on the element.
  Hit,
}

/// What an element draws with.
#[derive(Debug, Clone, Copy)]
pub struct RenderCx<'a> {
  tree: &'a UiTree,
  node: NodeId,
  size: Size,
  result: Option<&'a dyn Any>,
}

impl<'a> RenderCx<'a> {
  /// A context for drawing `node`, which layout gave `size` and, if its measure or arrange kept
  /// one, `result`.
  #[must_use]
  pub fn new(tree: &'a UiTree, node: NodeId, size: Size, result: Option<&'a dyn Any>) -> Self {
    Self {
      tree,
      node,
      size,
      result,
    }
  }

  /// The node being drawn.
  #[must_use]
  pub fn node(&self) -> NodeId {
    self.node
  }

  /// The tree being drawn.
  #[must_use]
  pub fn tree(&self) -> &'a UiTree {
    self.tree
  }

  /// The size layout gave the node, in its own coordinates: it draws from the origin to here.
  #[must_use]
  pub fn size(&self) -> Size {
    self.size
  }

  /// What the node's layout kept for it to draw, if it's a `T`: its arranged result if it set one,
  /// and its measured result otherwise. A control reads back the type it set, such as the
  /// paragraph a text control shaped.
  #[must_use]
  pub fn layout_result<T: Any>(&self) -> Option<&'a T> {
    self.result?.downcast_ref()
  }
}

/// What an element is hit-tested with.
#[derive(Debug, Clone, Copy)]
pub struct HitTestCx<'a> {
  tree: &'a UiTree,
  node: NodeId,
  size: Size,
}

impl<'a> HitTestCx<'a> {
  /// A context for hit-testing `node`, arranged at `size`.
  #[must_use]
  pub fn new(tree: &'a UiTree, node: NodeId, size: Size) -> Self {
    Self { tree, node, size }
  }

  /// The node being hit-tested.
  #[must_use]
  pub fn node(&self) -> NodeId {
    self.node
  }

  /// The tree being hit-tested.
  #[must_use]
  pub fn tree(&self) -> &'a UiTree {
    self.tree
  }

  /// The node's arranged size.
  #[must_use]
  pub fn size(&self) -> Size {
    self.size
  }

  /// Hits when `point`, in the element's own coordinates, is inside its arranged bounds: the left
  /// and top edges are inside, the right and bottom edges outside.
  #[must_use]
  pub fn bounds_test(&self, point: Point) -> HitTestResult {
    let inside = (0.0..self.size.width).contains(&point.x) && (0.0..self.size.height).contains(&point.y);
    if inside { HitTestResult::Hit } else { HitTestResult::Miss }
  }
}

/// What an element handles input with.
#[derive(Debug)]
pub struct EventCx<'a> {
  node: NodeId,
  marker: PhantomData<&'a mut ()>,
}

impl EventCx<'_> {
  /// A context for sending input to `node`.
  #[must_use]
  pub fn new(node: NodeId) -> Self {
    Self {
      node,
      marker: PhantomData,
    }
  }

  /// The node receiving input.
  #[must_use]
  pub fn node(&self) -> NodeId {
    self.node
  }
}

/// What an element describes itself to assistive technology with.
#[derive(Debug, Clone, Copy)]
pub struct AccessCx<'a> {
  tree: &'a UiTree,
  node: NodeId,
  bounds: Rect,
}

impl<'a> AccessCx<'a> {
  /// A context for describing `node`, arranged in `bounds`, in its parent's coordinates.
  #[must_use]
  pub fn new(tree: &'a UiTree, node: NodeId, bounds: Rect) -> Self {
    Self { tree, node, bounds }
  }

  /// The node being described.
  #[must_use]
  pub fn node(&self) -> NodeId {
    self.node
  }

  /// The tree being described.
  #[must_use]
  pub fn tree(&self) -> &'a UiTree {
    self.tree
  }

  /// The node's arranged bounds, in its parent's coordinates.
  #[must_use]
  pub fn bounds(&self) -> Rect {
    self.bounds
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  /// A tree with one node, for contexts that need one.
  fn one_node() -> (UiTree, NodeId) {
    #[derive(Debug)]
    struct Leaf;
    impl artic_core::Object for Leaf {
      fn visit_properties(&self, _visit: &mut dyn FnMut(artic_core::property::AnyProperty<'_>)) {}
    }
    let mut tree = UiTree::new();
    let node = tree.alloc(Box::new(Leaf));
    (tree, node)
  }

  // Points inside the arranged size hit; the right and bottom edges, and anything beyond, miss.
  #[test]
  fn bounds_test_hits_inside_the_arranged_size() {
    let (tree, node) = one_node();
    let cx = HitTestCx::new(&tree, node, Size::new(100.0, 40.0));

    assert_eq!(cx.bounds_test(Point::new(0.0, 0.0)), HitTestResult::Hit);
    assert_eq!(cx.bounds_test(Point::new(99.5, 39.5)), HitTestResult::Hit);
    assert_eq!(cx.bounds_test(Point::new(100.0, 10.0)), HitTestResult::Miss);
    assert_eq!(cx.bounds_test(Point::new(10.0, 40.0)), HitTestResult::Miss);
    assert_eq!(cx.bounds_test(Point::new(-0.5, 10.0)), HitTestResult::Miss);
  }

  // A zero-sized element is never hit, not even at its origin.
  #[test]
  fn zero_size_is_never_hit() {
    let (tree, node) = one_node();
    let cx = HitTestCx::new(&tree, node, Size::ZERO);

    assert_eq!(cx.bounds_test(Point::new(0.0, 0.0)), HitTestResult::Miss);
  }
}
