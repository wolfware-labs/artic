// What render reads of a node's layout result.

use artic_core::Object;
use artic_core::property::PropertyVisitor;
use artic_core::tree::{NodeId, UiTree};
use artic_element::RenderCx;
use artic_layout::geometry::{Rect, Size};
use artic_layout::{ArrangeCx, LayoutManager, LayoutNodes, Layoutable, MeasureCx, PixelGrid};
use artic_text::Engine;
use pretty_assertions::assert_eq;

/// A leaf that keeps "measured" from its measure and, when `arrange` is set, "arranged" from its
/// arrange.
#[derive(Debug)]
struct Noted {
  arrange: bool,
}

impl Object for Noted {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl Layoutable for Noted {
  fn measure(&self, cx: &mut MeasureCx<'_>, _available: Size) -> Size {
    cx.set_measured("measured");
    Size::new(10.0, 10.0)
  }

  fn arrange(&self, cx: &mut ArrangeCx<'_>, _rect: Rect) {
    if self.arrange {
      cx.set_arranged("arranged");
    }
  }
}

struct Notes;

impl LayoutNodes for Notes {
  fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
    tree.get_as::<Noted>(node).map(|node| node as &dyn Layoutable)
  }
}

/// A `Noted` leaf laid out in 10 by 10: the tree, the node and its layout.
fn laid_out(arrange: bool) -> (UiTree, NodeId, LayoutManager) {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(Noted { arrange }));
  let mut layout = LayoutManager::default();
  let grid = PixelGrid { scale: 1.0, snap: false };
  layout.run(
    &mut tree,
    &Notes,
    node,
    Size::new(10.0, 10.0),
    grid,
    &mut Engine::with_fonts([]),
  );
  (tree, node, layout)
}

// Render reads the arranged result when the node set one.
#[test]
fn render_reads_the_arranged_result_first() {
  let (tree, node, layout) = laid_out(true);

  let cx = RenderCx::new(&tree, node, Size::new(10.0, 10.0), layout.result(node));

  assert_eq!(cx.layout_result::<&str>(), Some(&"arranged"));
}

// Without an arranged result, render reads the measured one.
#[test]
fn render_reads_the_measured_result_without_one() {
  let (tree, node, layout) = laid_out(false);

  let cx = RenderCx::new(&tree, node, Size::new(10.0, 10.0), layout.result(node));

  assert_eq!(cx.layout_result::<&str>(), Some(&"measured"));
}

// A result asked for as another type reads as nothing.
#[test]
fn render_reads_nothing_of_another_type() {
  let (tree, node, layout) = laid_out(false);

  let cx = RenderCx::new(&tree, node, Size::new(10.0, 10.0), layout.result(node));

  assert_eq!(cx.layout_result::<u32>(), None);
}
