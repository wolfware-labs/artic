use super::*;
use artic_core::Object;
use artic_core::property::AnyProperty;
use pretty_assertions::assert_eq;
use std::cell::Cell;

/// A test node: a leaf of a fixed size, or a stack of its children from top to bottom. It counts
/// how often it's measured and arranged.
#[derive(Debug)]
struct Node {
  stack: bool,
  size: Cell<Size>,
  measured: Cell<u32>,
  arranged: Cell<u32>,
}

impl Object for Node {
  fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
}

impl Layoutable for Node {
  fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    self.measured.set(self.measured.get() + 1);
    if !self.stack {
      return self.size.get();
    }
    let mut total = Size::ZERO;
    for child in cx.children() {
      let desired = cx.measure(child, Size::new(available.width, f32::INFINITY));
      total = Size::new(total.width.max(desired.width), total.height + desired.height);
    }
    total
  }

  fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
    self.arranged.set(self.arranged.get() + 1);
    let mut top = 0.0;
    for child in cx.children() {
      let height = cx.desired(child).height;
      cx.arrange(child, Rect::new(0.0, top, rect.width, height));
      top += height;
    }
  }
}

struct Nodes;

impl LayoutNodes for Nodes {
  fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
    tree.get_as::<Node>(node).map(|node| node as &dyn Layoutable)
  }
}

fn leaf(tree: &mut UiTree, width: f32, height: f32) -> NodeId {
  tree.alloc(Box::new(Node {
    stack: false,
    size: Cell::new(Size::new(width, height)),
    measured: Cell::new(0),
    arranged: Cell::new(0),
  }))
}

fn stack(tree: &mut UiTree, children: &[NodeId]) -> NodeId {
  let node = tree.alloc(Box::new(Node {
    stack: true,
    size: Cell::new(Size::ZERO),
    measured: Cell::new(0),
    arranged: Cell::new(0),
  }));
  for child in children {
    tree.add_child(TreeKind::Visual, node, *child);
  }
  node
}

fn node(tree: &UiTree, id: NodeId) -> &Node {
  tree.get_as::<Node>(id).expect("a test node")
}

/// How often each of `ids` was measured and arranged.
fn counts(tree: &UiTree, ids: &[NodeId]) -> Vec<(u32, u32)> {
  ids
    .iter()
    .map(|id| (node(tree, *id).measured.get(), node(tree, *id).arranged.get()))
    .collect()
}

fn run(manager: &mut LayoutManager, tree: &mut UiTree, root: NodeId, available: Size) {
  manager.run(tree, &Nodes, root, available, GRID, &mut Recorder::default());
}

const WINDOW: Size = Size::new(400.0, 300.0);

// Measuring a node again in the same available size, with nothing changed, doesn't call it.
#[test]
fn measure_cached_for_same_available() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, 100.0, 20.0);
  let root = stack(&mut tree, &[child]);
  let mut manager = LayoutManager::default();

  run(&mut manager, &mut tree, root, WINDOW);
  run(&mut manager, &mut tree, root, WINDOW);

  assert_eq!(counts(&tree, &[root, child]), [(1, 1), (1, 1)]);
  assert_eq!(manager.desired(root), Some(Size::new(100.0, 20.0)));
}

// A different available size measures again, and the new results are kept.
#[test]
fn measure_reruns_on_available_change() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, 100.0, 20.0);
  let root = stack(&mut tree, &[child]);
  let mut manager = LayoutManager::default();

  run(&mut manager, &mut tree, root, WINDOW);
  run(&mut manager, &mut tree, root, Size::new(500.0, 300.0));

  assert_eq!(counts(&tree, &[root, child]), [(2, 2), (2, 2)]);
  assert_eq!(manager.rect(child), Some(Rect::new(0.0, 0.0, 500.0, 20.0)));
}

// A node marked for layout whose size doesn't change is measured and arranged again on its own:
// its parent and siblings keep their results.
#[test]
fn unchanged_desired_size_stops_propagation() {
  let mut tree = UiTree::new();
  let [first, second] = [leaf(&mut tree, 100.0, 20.0), leaf(&mut tree, 100.0, 30.0)];
  let root = stack(&mut tree, &[first, second]);
  let mut manager = LayoutManager::default();
  run(&mut manager, &mut tree, root, WINDOW);

  LayoutManager::invalidate_measure(&mut tree, first);
  run(&mut manager, &mut tree, root, WINDOW);

  assert_eq!(counts(&tree, &[root, first, second]), [(1, 1), (2, 2), (1, 1)]);
}

// A node whose size changes measures its parent again, which moves the siblings after it.
#[test]
fn changed_desired_size_propagates_to_the_parent() {
  let mut tree = UiTree::new();
  let [first, second] = [leaf(&mut tree, 100.0, 20.0), leaf(&mut tree, 100.0, 30.0)];
  let root = stack(&mut tree, &[first, second]);
  let mut manager = LayoutManager::default();
  run(&mut manager, &mut tree, root, WINDOW);

  node(&tree, first).size.set(Size::new(100.0, 50.0));
  LayoutManager::invalidate_measure(&mut tree, first);
  run(&mut manager, &mut tree, root, WINDOW);

  assert_eq!(counts(&tree, &[root, first, second]), [(2, 2), (2, 2), (1, 2)]);
  assert_eq!(manager.rect(second), Some(Rect::new(0.0, 50.0, 400.0, 30.0)));
}

// Removing a visual child lays its parent out again, and drops the child's results.
#[test]
fn removed_children_relayout_the_parent() {
  let mut tree = UiTree::new();
  let [first, second] = [leaf(&mut tree, 100.0, 20.0), leaf(&mut tree, 100.0, 30.0)];
  let root = stack(&mut tree, &[first, second]);
  let mut manager = LayoutManager::default();
  run(&mut manager, &mut tree, root, WINDOW);

  tree.detach(TreeKind::Visual, first);
  run(&mut manager, &mut tree, root, WINDOW);

  assert_eq!(manager.rect(second), Some(Rect::new(0.0, 0.0, 400.0, 30.0)));
  assert_eq!(manager.desired(first), None);
}

// Arrange-only invalidation arranges the node again in the same rectangle, without measuring.
#[test]
fn invalidate_arrange_skips_measure() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, 100.0, 20.0);
  let root = stack(&mut tree, &[child]);
  let mut manager = LayoutManager::default();
  run(&mut manager, &mut tree, root, WINDOW);

  manager.invalidate_arrange(child);
  run(&mut manager, &mut tree, root, WINDOW);

  assert_eq!(counts(&tree, &[root, child]), [(1, 1), (1, 2)]);
}

// An infinite available size is legal: the stack offers its children infinite height.
#[test]
fn infinite_available_is_legal() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, 100.0, 20.0);
  let root = stack(&mut tree, &[child]);
  let mut manager = LayoutManager::default();

  run(&mut manager, &mut tree, root, Size::INFINITY);

  assert_eq!(manager.desired(child), Some(Size::new(100.0, 20.0)));
  assert_eq!(
    manager.rect(root),
    Some(Rect::new(0.0, 0.0, 100.0, 20.0)),
    "an infinite root takes its desired size"
  );
}

// An element that asks for an infinite size is a bug, reported with the node in debug builds.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "returned an invalid desired size")]
fn infinite_desired_size_panics_in_debug() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, f32::INFINITY, 20.0);
  let root = stack(&mut tree, &[child]);

  run(&mut LayoutManager::default(), &mut tree, root, WINDOW);
}

// The panic names the node that returned the infinite size.
#[test]
#[cfg(debug_assertions)]
fn infinite_desired_size_names_the_node() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, f32::INFINITY, 20.0);
  let root = stack(&mut tree, &[child]);

  let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    run(&mut LayoutManager::default(), &mut tree, root, WINDOW);
  }))
  .expect_err("panics");

  let message = panic.downcast_ref::<String>().expect("a formatted message");
  assert!(message.starts_with(&format!("node {child:?} returned")), "{message}");
}

// With snapping on, arranged rectangles have their edges on whole device pixels.
#[test]
fn layout_rounding_snaps_to_device_pixels() {
  for scale in [1.0_f32, 1.25, 2.0] {
    let mut tree = UiTree::new();
    let children = [10.3, 10.3, 10.3].map(|height| leaf(&mut tree, 33.3, height));
    let root = stack(&mut tree, &children);
    let mut manager = LayoutManager::default();

    manager.run(
      &mut tree,
      &Nodes,
      root,
      Size::new(100.3, 200.0),
      PixelGrid { scale, snap: true },
      &mut Recorder::default(),
    );

    for child in children {
      let rect = manager.rect(child).expect("arranged");
      for edge in [rect.x, rect.y, rect.x + rect.width, rect.y + rect.height] {
        let device = edge * scale;
        assert!((device - device.round()).abs() < 1e-3, "{rect} at scale {scale}");
      }
    }
  }
}

mod bounds {
  use super::{GRID, Recorder};
  use crate::geometry::{Rect, Size};
  use crate::{ArrangeCx, LayoutManager, LayoutNodes, Layoutable, MeasureCx};
  use artic_core::Object;
  use artic_core::property::AnyProperty;
  use artic_core::tree::{NodeId, TreeKind, UiTree};
  use pretty_assertions::assert_eq;
  use std::cell::Cell;

  /// Takes a 10-unit inset of its slot as its bounds, and fills them with its children.
  #[derive(Debug, Default)]
  struct Inset {
    arranged: Cell<u32>,
  }

  impl Object for Inset {
    fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  }

  impl Layoutable for Inset {
    fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
      let children: Vec<NodeId> = cx.children().collect();
      for child in children {
        cx.measure(child, available);
      }
      available
    }

    fn arrange(&self, cx: &mut ArrangeCx<'_>, slot: Rect) {
      self.arranged.set(self.arranged.get() + 1);
      let bounds = Rect::new(slot.x + 10.0, slot.y + 10.0, slot.width - 20.0, slot.height - 20.0);
      cx.set_bounds(bounds);
      let children: Vec<NodeId> = cx.children().collect();
      for child in children {
        cx.arrange(child, Rect::new(0.0, 0.0, bounds.width, bounds.height));
      }
    }
  }

  /// Lays out `Inset` nodes.
  struct Insets;

  impl LayoutNodes for Insets {
    fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
      tree.get_as::<Inset>(node).map(|inset| inset as &dyn Layoutable)
    }
  }

  // An element's bounds are what it set, its children are placed relative to them, and the same
  // slot doesn't arrange it again although its bounds differ from the slot.
  #[test]
  fn set_bounds_records_the_element_box() {
    let mut tree = UiTree::new();
    let outer = tree.alloc(Box::<Inset>::default());
    let inner = tree.alloc(Box::<Inset>::default());
    tree.add_child(TreeKind::Visual, outer, inner);
    let mut layout = LayoutManager::default();

    layout.run(
      &mut tree,
      &Insets,
      outer,
      Size::new(100.0, 80.0),
      GRID,
      &mut Recorder::default(),
    );
    assert_eq!(layout.rect(outer), Some(Rect::new(10.0, 10.0, 80.0, 60.0)));
    assert_eq!(layout.rect(inner), Some(Rect::new(10.0, 10.0, 60.0, 40.0)));

    LayoutManager::invalidate_measure(&mut tree, outer);
    layout.run(
      &mut tree,
      &Insets,
      outer,
      Size::new(100.0, 80.0),
      GRID,
      &mut Recorder::default(),
    );
    let arranged = |node| tree.get_as::<Inset>(node).unwrap().arranged.get();
    assert_eq!(arranged(inner), 1, "same slot, nothing changed below: not arranged again");
  }
}

mod skipped {
  use super::{GRID, Recorder};
  use crate::geometry::{Rect, Size};
  use crate::{ArrangeCx, LayoutManager, LayoutNodes, Layoutable, MeasureCx};
  use artic_core::Object;
  use artic_core::property::AnyProperty;
  use artic_core::tree::{NodeId, TreeKind, UiTree};
  use pretty_assertions::assert_eq;
  use std::cell::Cell;

  /// Measures its children unless `skip` is set, as a collapsed element does; counts its own
  /// measures.
  #[derive(Debug, Default)]
  struct Toggle {
    skip: Cell<bool>,
    measured: Cell<u32>,
  }

  impl Object for Toggle {
    fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  }

  impl Layoutable for Toggle {
    fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
      self.measured.set(self.measured.get() + 1);
      if self.skip.get() {
        return Size::ZERO;
      }
      let children: Vec<NodeId> = cx.children().collect();
      for child in children {
        cx.measure(child, available);
      }
      Size::new(10.0, 10.0)
    }

    fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
      if self.skip.get() {
        return;
      }
      let children: Vec<NodeId> = cx.children().collect();
      for child in children {
        cx.arrange(child, Rect::new(0.0, 0.0, rect.width, rect.height));
      }
    }
  }

  /// Lays out `Toggle` nodes.
  struct Toggles;

  impl LayoutNodes for Toggles {
    fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
      tree.get_as::<Toggle>(node).map(|toggle| toggle as &dyn Layoutable)
    }
  }

  // Once a parent stops measuring a child, the child and its subtree are forgotten, and a child
  // made dirty afterwards isn't laid out.
  #[test]
  fn children_a_measure_skips_are_forgotten() {
    let mut tree = UiTree::new();
    let [parent, child, grandchild] = [(); 3].map(|()| tree.alloc(Box::<Toggle>::default()));
    tree.add_child(TreeKind::Visual, parent, child);
    tree.add_child(TreeKind::Visual, child, grandchild);
    let mut layout = LayoutManager::default();
    let available = Size::new(100.0, 100.0);
    layout.run(&mut tree, &Toggles, parent, available, GRID, &mut Recorder::default());
    assert!(layout.rect(grandchild).is_some());

    tree.get_as::<Toggle>(parent).unwrap().skip.set(true);
    LayoutManager::invalidate_measure(&mut tree, parent);
    layout.run(&mut tree, &Toggles, parent, available, GRID, &mut Recorder::default());
    assert_eq!((layout.rect(child), layout.rect(grandchild)), (None, None));

    LayoutManager::invalidate_measure(&mut tree, child);
    layout.run(&mut tree, &Toggles, parent, available, GRID, &mut Recorder::default());
    assert_eq!(tree.get_as::<Toggle>(child).unwrap().measured.get(), 1);
  }

  // A child whose parent stopped measuring it, as a collapsed parent does, keeps its last size,
  // so when the parent shows it again at another size the child is listed as resized.
  #[test]
  fn child_shown_again_at_another_size_is_resized() {
    let mut tree = UiTree::new();
    let [parent, child] = [(); 2].map(|()| tree.alloc(Box::<Toggle>::default()));
    tree.add_child(TreeKind::Visual, parent, child);
    let mut layout = LayoutManager::default();
    layout.run(
      &mut tree,
      &Toggles,
      parent,
      Size::new(100.0, 100.0),
      GRID,
      &mut Recorder::default(),
    );
    tree.get_as::<Toggle>(parent).unwrap().skip.set(true);
    LayoutManager::invalidate_measure(&mut tree, parent);
    layout.run(
      &mut tree,
      &Toggles,
      parent,
      Size::new(200.0, 200.0),
      GRID,
      &mut Recorder::default(),
    );

    tree.get_as::<Toggle>(parent).unwrap().skip.set(false);
    LayoutManager::invalidate_measure(&mut tree, parent);
    let stats = layout.run(
      &mut tree,
      &Toggles,
      parent,
      Size::new(200.0, 200.0),
      GRID,
      &mut Recorder::default(),
    );

    assert_eq!(stats.resized, [child]);
  }

  /// A root, a middle node and a leaf, laid out once.
  fn three_levels() -> (UiTree, LayoutManager, [NodeId; 3]) {
    let mut tree = UiTree::new();
    let nodes = [(); 3].map(|()| tree.alloc(Box::<Toggle>::default()));
    tree.add_child(TreeKind::Visual, nodes[0], nodes[1]);
    tree.add_child(TreeKind::Visual, nodes[1], nodes[2]);
    let mut layout = LayoutManager::default();
    layout.run(
      &mut tree,
      &Toggles,
      nodes[0],
      Size::new(100.0, 100.0),
      GRID,
      &mut Recorder::default(),
    );
    (tree, layout, nodes)
  }

  /// How many times `node` has measured.
  fn measures(tree: &UiTree, node: NodeId) -> u32 {
    tree.get_as::<Toggle>(node).unwrap().measured.get()
  }

  // A dirty node under a dirty ancestor waits for the ancestor's measure, so when the ancestor
  // stops measuring its children in the same run, the node isn't measured at all.
  #[test]
  fn dirty_descendant_of_a_collapsing_ancestor_isnt_measured() {
    let (mut tree, mut layout, [root, middle, leaf]) = three_levels();

    tree.get_as::<Toggle>(middle).unwrap().skip.set(true);
    LayoutManager::invalidate_measure(&mut tree, middle);
    LayoutManager::invalidate_measure(&mut tree, leaf);
    layout.run(
      &mut tree,
      &Toggles,
      root,
      Size::new(100.0, 100.0),
      GRID,
      &mut Recorder::default(),
    );

    assert_eq!(measures(&tree, leaf), 1);
    assert_eq!(layout.rect(leaf), None);
  }

  // When the ancestor keeps measuring, its measure still reaches the dirty node through a clean
  // node in between, which would otherwise answer from its cache.
  #[test]
  fn dirty_descendant_of_a_dirty_ancestor_is_still_measured() {
    let (mut tree, mut layout, [root, _, leaf]) = three_levels();

    LayoutManager::invalidate_measure(&mut tree, root);
    LayoutManager::invalidate_measure(&mut tree, leaf);
    layout.run(
      &mut tree,
      &Toggles,
      root,
      Size::new(100.0, 100.0),
      GRID,
      &mut Recorder::default(),
    );

    assert_eq!(measures(&tree, leaf), 2);
  }
}

mod misuse {
  use super::{GRID, Recorder};
  use crate::geometry::{Rect, Size};
  use crate::{ArrangeCx, LayoutManager, LayoutNodes, Layoutable, MeasureCx};
  use artic_core::Object;
  use artic_core::property::AnyProperty;
  use artic_core::tree::{NodeId, TreeKind, UiTree};

  /// Measures its first grandchild directly, skipping the child in between.
  #[derive(Debug)]
  struct Skipper;

  impl Object for Skipper {
    fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  }

  impl Layoutable for Skipper {
    fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
      let tree = cx.tree();
      let grandchild = cx.children().find_map(|child| tree.children(TreeKind::Visual, child).next());
      if let Some(grandchild) = grandchild {
        cx.measure(grandchild, available);
      }
      Size::ZERO
    }

    fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}
  }

  /// Lays out `Skipper` nodes.
  struct Skippers;

  impl LayoutNodes for Skippers {
    fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
      tree.get_as::<Skipper>(node).map(|skipper| skipper as &dyn Layoutable)
    }
  }

  // Measuring a node that isn't one of the current node's visual children is a bug, caught in
  // debug builds: its results would be forgotten right after.
  #[test]
  #[cfg(debug_assertions)]
  #[should_panic(expected = "isn't a visual child of")]
  fn measuring_a_grandchild_panics() {
    let mut tree = UiTree::new();
    let [root, child, grandchild] = [(); 3].map(|()| tree.alloc(Box::new(Skipper)));
    tree.add_child(TreeKind::Visual, root, child);
    tree.add_child(TreeKind::Visual, child, grandchild);

    LayoutManager::default().run(
      &mut tree,
      &Skippers,
      root,
      Size::new(10.0, 10.0),
      GRID,
      &mut Recorder::default(),
    );
  }
}

// A run reports the nodes it measured and arranged, lists none as resized the first time, and a
// run with nothing changed reports nothing.
#[test]
fn run_reports_measured_and_arranged() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, 100.0, 20.0);
  let root = stack(&mut tree, &[child]);
  let mut manager = LayoutManager::default();

  let first = manager.run(&mut tree, &Nodes, root, WINDOW, GRID, &mut Recorder::default());
  let second = manager.run(&mut tree, &Nodes, root, WINDOW, GRID, &mut Recorder::default());

  assert_eq!((first.measured, first.arranged), (2, 2));
  assert!(first.resized.is_empty());
  assert_eq!(second, LayoutStats::default());
}

// A node whose arranged size changes is listed as resized; a sibling that only moves isn't.
#[test]
fn run_reports_resized() {
  let mut tree = UiTree::new();
  let grows = leaf(&mut tree, 100.0, 20.0);
  let moves = leaf(&mut tree, 100.0, 20.0);
  let root = stack(&mut tree, &[grows, moves]);
  let mut manager = LayoutManager::default();
  run(&mut manager, &mut tree, root, WINDOW);

  node(&tree, grows).size.set(Size::new(100.0, 40.0));
  LayoutManager::invalidate_measure(&mut tree, grows);
  let stats = manager.run(&mut tree, &Nodes, root, WINDOW, GRID, &mut Recorder::default());

  assert_eq!(stats.resized, [grows]);
}

// A new scale arranges every node again on the new device grid and measures nothing: a desired
// size doesn't depend on the display.
#[test]
fn scale_change_arranges_without_measuring() {
  let mut tree = UiTree::new();
  let children = [10.3, 10.3].map(|height| leaf(&mut tree, 33.3, height));
  let root = stack(&mut tree, &children);
  let mut manager = LayoutManager::default();
  manager.run(
    &mut tree,
    &Nodes,
    root,
    WINDOW,
    PixelGrid { scale: 1.0, snap: true },
    &mut Recorder::default(),
  );

  let stats = manager.run(
    &mut tree,
    &Nodes,
    root,
    WINDOW,
    PixelGrid { scale: 1.25, snap: true },
    &mut Recorder::default(),
  );

  assert_eq!((stats.measured, stats.arranged), (0, 3));
  let rect = manager.rect(children[1]).expect("arranged");
  let device = rect.y * 1.25;
  assert!((device - device.round()).abs() < 1e-3, "{rect}");
}

// A scale reported before the size that matches it, as Windows does, changes the logical size:
// the root is measured again, like on any resize.
#[test]
fn scale_reported_before_size_measures_again() {
  let mut tree = UiTree::new();
  let children = [10.0, 10.0].map(|height| leaf(&mut tree, 33.0, height));
  let root = stack(&mut tree, &children);
  let mut manager = LayoutManager::default();
  manager.run(
    &mut tree,
    &Nodes,
    root,
    WINDOW,
    PixelGrid { scale: 1.0, snap: true },
    &mut Recorder::default(),
  );

  let stats = manager.run(
    &mut tree,
    &Nodes,
    root,
    Size::new(200.0, 150.0),
    PixelGrid { scale: 2.0, snap: true },
    &mut Recorder::default(),
  );

  assert_eq!(stats.measured, 3);
  assert_eq!(manager.rect(root), Some(Rect::new(0.0, 0.0, 200.0, 150.0)));
}

// Turning snapping on at the same scale moves the rectangles onto the device grid.
#[test]
fn snap_change_arranges_again() {
  let mut tree = UiTree::new();
  let children = [10.3, 10.3].map(|height| leaf(&mut tree, 33.3, height));
  let root = stack(&mut tree, &children);
  let mut manager = LayoutManager::default();
  manager.run(&mut tree, &Nodes, root, WINDOW, GRID, &mut Recorder::default());

  let stats = manager.run(
    &mut tree,
    &Nodes,
    root,
    WINDOW,
    PixelGrid { scale: 1.0, snap: true },
    &mut Recorder::default(),
  );

  assert_eq!((stats.measured, stats.arranged), (0, 3));
  assert_eq!(manager.rect(children[1]).map(|rect| rect.y), Some(10.0));
}

// Measuring alone sizes the subtree without arranging it, and a run in the same available size
// then measures nothing.
#[test]
fn measure_alone_leaves_arranging_to_the_next_run() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, 100.0, 20.0);
  let root = stack(&mut tree, &[child]);
  let mut manager = LayoutManager::default();
  let unbounded = Size::new(f32::INFINITY, f32::INFINITY);

  let desired = manager.measure(&mut tree, &Nodes, root, unbounded, &mut Recorder::default());

  assert_eq!(desired, Size::new(100.0, 20.0));
  assert_eq!(manager.rect(root), None);
  let stats = manager.run(&mut tree, &Nodes, root, unbounded, GRID, &mut Recorder::default());
  assert_eq!((stats.measured, stats.arranged), (0, 2));
}

mod overlay {
  use super::{GRID, Recorder, leaf};
  use crate::geometry::{Rect, Size};
  use crate::{ArrangeCx, LayoutManager, LayoutNodes, Layoutable, MeasureCx};
  use artic_core::Object;
  use artic_core::property::AnyProperty;
  use artic_core::tree::{NodeId, TreeKind, UiTree};
  use pretty_assertions::assert_eq;

  /// Lays its children over each other, in its whole box.
  #[derive(Debug)]
  struct Overlay;

  impl Object for Overlay {
    fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  }

  impl Layoutable for Overlay {
    fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
      cx.measure_children(available)
    }

    fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
      cx.arrange_children(Rect::new(0.0, 0.0, rect.width, rect.height));
    }
  }

  /// Lays out `Overlay` nodes and the parent module's test nodes.
  struct Nodes;

  impl LayoutNodes for Nodes {
    fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
      tree
        .get_as::<Overlay>(node)
        .map(|overlay| overlay as &dyn Layoutable)
        .or_else(|| super::Nodes.layoutable(tree, node))
    }
  }

  // Children sharing one box want the largest of their sizes, and each fills the box.
  #[test]
  fn children_share_one_box() {
    let mut tree = UiTree::new();
    let wide = leaf(&mut tree, 40.0, 10.0);
    let tall = leaf(&mut tree, 20.0, 30.0);
    let root = tree.alloc(Box::new(Overlay));
    tree.add_child(TreeKind::Visual, root, wide);
    tree.add_child(TreeKind::Visual, root, tall);
    let mut manager = LayoutManager::default();

    manager.run(
      &mut tree,
      &Nodes,
      root,
      Size::new(f32::INFINITY, f32::INFINITY),
      GRID,
      &mut Recorder::default(),
    );

    assert_eq!(manager.desired(root), Some(Size::new(40.0, 30.0)));
    for child in [wide, tall] {
      assert_eq!(manager.rect(child), Some(Rect::new(0.0, 0.0, 40.0, 30.0)));
    }
  }
}

mod collapsed {
  use super::{GRID, Recorder};
  use crate::geometry::{Rect, Size};
  use crate::{ArrangeCx, LayoutManager, LayoutNodes, Layoutable, MeasureCx};
  use artic_core::Object;
  use artic_core::property::AnyProperty;
  use artic_core::tree::{NodeId, TreeKind, UiTree};
  use pretty_assertions::assert_eq;
  use std::cell::Cell;

  /// A leaf of a fixed size that can be collapsed; counts its measures.
  #[derive(Debug)]
  struct Item {
    size: Size,
    collapsed: Cell<bool>,
    measured: Cell<u32>,
  }

  /// Stacks its children top to bottom with 5 between each pair, as a panel with spacing does.
  #[derive(Debug, Default)]
  struct Column {
    collapsed: Cell<bool>,
    measured: Cell<u32>,
  }

  impl Object for Item {
    fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  }

  impl Object for Column {
    fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  }

  impl Layoutable for Item {
    fn measure(&self, _cx: &mut MeasureCx<'_>, _available: Size) -> Size {
      self.measured.set(self.measured.get() + 1);
      self.size
    }

    fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}

    fn collapsed(&self) -> bool {
      self.collapsed.get()
    }
  }

  impl Layoutable for Column {
    fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
      self.measured.set(self.measured.get() + 1);
      let children: Vec<NodeId> = cx.children().collect();
      let gaps = children.len().saturating_sub(1) as f32 * 5.0;
      let mut total = Size::new(0.0, gaps);
      for child in children {
        let desired = cx.measure(child, Size::new(available.width, f32::INFINITY));
        total = Size::new(total.width.max(desired.width), total.height + desired.height);
      }
      total
    }

    fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
      let mut top = 0.0;
      let children: Vec<NodeId> = cx.children().collect();
      for child in children {
        let height = cx.desired(child).height;
        cx.arrange(child, Rect::new(0.0, top, rect.width, height));
        top += height + 5.0;
      }
    }

    fn collapsed(&self) -> bool {
      self.collapsed.get()
    }
  }

  struct Nodes;

  impl LayoutNodes for Nodes {
    fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
      tree
        .get_as::<Item>(node)
        .map(|item| item as &dyn Layoutable)
        .or_else(|| tree.get_as::<Column>(node).map(|column| column as &dyn Layoutable))
    }
  }

  const WINDOW: Size = Size::new(100.0, 300.0);

  fn item(tree: &mut UiTree, height: f32) -> NodeId {
    tree.alloc(Box::new(Item {
      size: Size::new(10.0, height),
      collapsed: Cell::new(false),
      measured: Cell::new(0),
    }))
  }

  fn column(tree: &mut UiTree, children: &[NodeId]) -> NodeId {
    let node = tree.alloc(Box::new(Column::default()));
    for child in children {
      tree.add_child(TreeKind::Visual, node, *child);
    }
    node
  }

  fn run(layout: &mut LayoutManager, tree: &mut UiTree, root: NodeId) {
    layout.run(tree, &Nodes, root, WINDOW, GRID, &mut Recorder::default());
  }

  fn collapse(tree: &mut UiTree, node: NodeId, collapsed: bool) {
    if let Some(item) = tree.get_as::<Item>(node) {
      item.collapsed.set(collapsed);
    }
    if let Some(column) = tree.get_as::<Column>(node) {
      column.collapsed.set(collapsed);
    }
    LayoutManager::invalidate_measure(tree, node);
  }

  // A collapsed child isn't among its parent's children: it gets no spacing, no size and no rect.
  #[test]
  fn collapsed_children_are_left_out() {
    let mut tree = UiTree::new();
    let (a, b, c) = (item(&mut tree, 20.0), item(&mut tree, 30.0), item(&mut tree, 40.0));
    tree.get_as::<Item>(b).unwrap().collapsed.set(true);
    let root = column(&mut tree, &[a, b, c]);
    let mut layout = LayoutManager::default();

    run(&mut layout, &mut tree, root);

    assert_eq!(layout.desired(root), Some(Size::new(10.0, 65.0)));
    assert_eq!((layout.desired(b), layout.rect(b)), (None, None));
    assert_eq!(tree.get_as::<Item>(b).unwrap().measured.get(), 0);
    assert_eq!(layout.rect(c), Some(Rect::new(0.0, 25.0, 100.0, 40.0)));
  }

  // A visible child that is empty is still a child, and still gets its spacing.
  #[test]
  fn empty_visible_child_is_still_a_child() {
    let mut tree = UiTree::new();
    let (a, b) = (item(&mut tree, 20.0), item(&mut tree, 0.0));
    let root = column(&mut tree, &[a, b]);
    let mut layout = LayoutManager::default();

    run(&mut layout, &mut tree, root);

    assert_eq!(layout.desired(root), Some(Size::new(10.0, 25.0)));
    assert!(layout.rect(b).is_some());
  }

  // Collapsing a child measures its parent again, without it.
  #[test]
  fn collapsing_a_child_remeasures_the_parent() {
    let mut tree = UiTree::new();
    let (a, b) = (item(&mut tree, 20.0), item(&mut tree, 30.0));
    let root = column(&mut tree, &[a, b]);
    let mut layout = LayoutManager::default();
    run(&mut layout, &mut tree, root);

    collapse(&mut tree, b, true);
    run(&mut layout, &mut tree, root);

    assert_eq!(layout.desired(root), Some(Size::new(10.0, 20.0)));
    assert_eq!(layout.rect(b), None);
  }

  // Collapsing a child whose size was already zero still measures the parent again: the change
  // is in who the children are, not in a size.
  #[test]
  fn collapsing_an_empty_child_remeasures_the_parent() {
    let mut tree = UiTree::new();
    let (a, b) = (item(&mut tree, 20.0), item(&mut tree, 0.0));
    let root = column(&mut tree, &[a, b]);
    let mut layout = LayoutManager::default();
    run(&mut layout, &mut tree, root);

    collapse(&mut tree, b, true);
    run(&mut layout, &mut tree, root);

    assert_eq!(layout.desired(root), Some(Size::new(10.0, 20.0)));
  }

  // Showing a collapsed child again measures it and gives it its place back.
  #[test]
  fn showing_a_child_again_measures_it() {
    let mut tree = UiTree::new();
    let (a, b) = (item(&mut tree, 20.0), item(&mut tree, 30.0));
    tree.get_as::<Item>(b).unwrap().collapsed.set(true);
    let root = column(&mut tree, &[a, b]);
    let mut layout = LayoutManager::default();
    run(&mut layout, &mut tree, root);

    collapse(&mut tree, b, false);
    run(&mut layout, &mut tree, root);

    assert_eq!(tree.get_as::<Item>(b).unwrap().measured.get(), 1);
    assert_eq!(layout.rect(b), Some(Rect::new(0.0, 25.0, 100.0, 30.0)));
  }

  // A child under a collapsed child isn't measured even when it is invalidated on its own.
  #[test]
  fn dirty_descendant_of_a_collapsed_child_isnt_measured() {
    let mut tree = UiTree::new();
    let leaf = item(&mut tree, 20.0);
    let inner = column(&mut tree, &[leaf]);
    let root = column(&mut tree, &[inner]);
    let mut layout = LayoutManager::default();
    run(&mut layout, &mut tree, root);
    collapse(&mut tree, inner, true);
    run(&mut layout, &mut tree, root);

    LayoutManager::invalidate_measure(&mut tree, leaf);
    run(&mut layout, &mut tree, root);

    assert_eq!(tree.get_as::<Item>(leaf).unwrap().measured.get(), 1);
    assert_eq!(layout.desired(leaf), None);
  }

  // Changes under a collapsed node stop at it once it's left out: invalidating a descendant
  // again and again doesn't measure the collapsed node's parent again.
  #[test]
  fn changes_under_a_collapsed_node_stop_there() {
    let mut tree = UiTree::new();
    let leaf = item(&mut tree, 20.0);
    let inner = column(&mut tree, &[leaf]);
    let middle = column(&mut tree, &[inner]);
    let root = column(&mut tree, &[middle]);
    let mut layout = LayoutManager::default();
    run(&mut layout, &mut tree, root);
    collapse(&mut tree, inner, true);
    run(&mut layout, &mut tree, root);
    let before = tree.get_as::<Column>(middle).unwrap().measured.get();

    for _ in 0..3 {
      LayoutManager::invalidate_measure(&mut tree, leaf);
      LayoutManager::invalidate_measure(&mut tree, inner);
      run(&mut layout, &mut tree, root);
    }

    assert_eq!(tree.get_as::<Column>(middle).unwrap().measured.get(), before);
  }

  /// Measures and arranges its first child by id, whatever `children()` says.
  #[derive(Debug, Default)]
  struct Direct;

  impl Object for Direct {
    fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  }

  impl Layoutable for Direct {
    fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
      let child = cx.tree().children(TreeKind::Visual, cx.node()).next().expect("a child");
      cx.measure(child, available)
    }

    fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
      let child = cx.tree().children(TreeKind::Visual, cx.node()).next().expect("a child");
      cx.arrange(child, rect);
    }
  }

  struct WithDirect;

  impl LayoutNodes for WithDirect {
    fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
      Nodes
        .layoutable(tree, node)
        .or_else(|| tree.get_as::<Direct>(node).map(|direct| direct as &dyn Layoutable))
    }
  }

  // A parent that names a collapsed child directly gets nothing back: no size, no rect.
  #[test]
  fn measuring_a_collapsed_child_directly_does_nothing() {
    let mut tree = UiTree::new();
    let child = item(&mut tree, 20.0);
    tree.get_as::<Item>(child).unwrap().collapsed.set(true);
    let root = tree.alloc(Box::new(Direct));
    tree.add_child(TreeKind::Visual, root, child);
    let mut layout = LayoutManager::default();

    layout.run(&mut tree, &WithDirect, root, WINDOW, GRID, &mut Recorder::default());

    assert_eq!(layout.desired(root), Some(Size::ZERO));
    assert_eq!((layout.desired(child), layout.rect(child)), (None, None));
    assert_eq!(tree.get_as::<Item>(child).unwrap().measured.get(), 0);
  }

  // A collapsed root lays out nothing, in `run` and in `measure`.
  #[test]
  fn collapsed_root_lays_out_nothing() {
    let mut tree = UiTree::new();
    let a = item(&mut tree, 20.0);
    let root = column(&mut tree, &[a]);
    tree.get_as::<Column>(root).unwrap().collapsed.set(true);
    let mut layout = LayoutManager::default();

    run(&mut layout, &mut tree, root);

    assert_eq!((layout.desired(root), layout.rect(root), layout.rect(a)), (None, None, None));
  }

  // `measure` on a collapsed root reports nothing either.
  #[test]
  fn measure_of_a_collapsed_root_is_none() {
    let mut tree = UiTree::new();
    let root = column(&mut tree, &[]);
    tree.get_as::<Column>(root).unwrap().collapsed.set(true);
    let mut layout = LayoutManager::default();

    let size = layout.measure(&mut tree, &Nodes, root, WINDOW, &mut Recorder::default());

    assert_eq!((size, layout.desired(root)), (Size::ZERO, None));
  }

  // A root shown again after being collapsed lays out as usual.
  #[test]
  fn collapsed_root_comes_back() {
    let mut tree = UiTree::new();
    let a = item(&mut tree, 20.0);
    let root = column(&mut tree, &[a]);
    let mut layout = LayoutManager::default();
    collapse(&mut tree, root, true);
    run(&mut layout, &mut tree, root);

    collapse(&mut tree, root, false);
    run(&mut layout, &mut tree, root);

    assert_eq!(layout.rect(a), Some(Rect::new(0.0, 0.0, 100.0, 20.0)));
  }
}

use crate::text::{FontFamily, Paragraph, TextEngine, TextLimits, TextStyle};

/// A text engine that records the text it's asked to shape and answers with an empty paragraph.
#[derive(Debug, Default)]
struct Recorder {
  asked: Vec<String>,
}

impl TextEngine for Recorder {
  fn paragraph(&mut self, text: &str, _style: &TextStyle, _limits: TextLimits) -> Paragraph {
    self.asked.push(text.to_owned());
    Paragraph::default()
  }
}

/// A test leaf, 50 by 10, that shapes "measure" in its measure and "arrange" in its arrange, keeps
/// the results it's told to, and notes what its arrange read back.
#[derive(Debug, Default)]
struct Shaper {
  measured: Cell<Option<u32>>,
  arranged: Cell<Option<u32>>,
  read: Cell<Option<u32>>,
  twice: Cell<bool>,
  collapsed: Cell<bool>,
}

impl Object for Shaper {
  fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
}

impl Layoutable for Shaper {
  fn measure(&self, cx: &mut MeasureCx<'_>, _available: Size) -> Size {
    cx.text()
      .paragraph("measure", &TextStyle::new(FontFamily::default(), 16.0), TextLimits::NONE);
    if let Some(result) = self.measured.get() {
      if self.twice.get() {
        cx.set_measured(9_u32);
      }
      cx.set_measured(result);
    }
    Size::new(50.0, 10.0)
  }

  fn arrange(&self, cx: &mut ArrangeCx<'_>, _rect: Rect) {
    cx.text()
      .paragraph("arrange", &TextStyle::new(FontFamily::default(), 16.0), TextLimits::NONE);
    self.read.set(cx.arranged::<u32>().copied());
    if let Some(result) = self.arranged.get() {
      cx.set_arranged(result);
    }
  }

  fn collapsed(&self) -> bool {
    self.collapsed.get()
  }
}

/// Measures its children with all the room they want and arranges each in its whole rectangle, so
/// a change of its size reaches their arrange and not their measure.
#[derive(Debug, Default)]
struct Holder;

impl Object for Holder {
  fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
}

impl Layoutable for Holder {
  fn measure(&self, cx: &mut MeasureCx<'_>, _available: Size) -> Size {
    cx.measure_children(Size::new(f32::INFINITY, f32::INFINITY))
  }

  fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
    cx.arrange_children(Rect::new(0.0, 0.0, rect.width, rect.height));
  }
}

/// Lays out `Node`, `Holder` and `Shaper` nodes.
struct WithShapers;

impl LayoutNodes for WithShapers {
  fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
    tree
      .get_as::<Node>(node)
      .map(|node| node as &dyn Layoutable)
      .or_else(|| tree.get_as::<Holder>(node).map(|node| node as &dyn Layoutable))
      .or_else(|| tree.get_as::<Shaper>(node).map(|node| node as &dyn Layoutable))
  }
}

/// A `Holder` holding one `Shaper`: the tree, the holder and the shaper.
fn holding_a_shaper() -> (UiTree, NodeId, NodeId) {
  let mut tree = UiTree::new();
  let shaper = tree.alloc(Box::<Shaper>::default());
  let holder = tree.alloc(Box::new(Holder));
  tree.add_child(TreeKind::Visual, holder, shaper);
  (tree, holder, shaper)
}

const GRID: PixelGrid = PixelGrid { scale: 1.0, snap: false };

// Measuring hands the node the engine layout was given.
#[test]
fn measure_reaches_the_text_engine() {
  let (mut tree, holder, _) = holding_a_shaper();
  let mut text = Recorder::default();

  LayoutManager::default().run(&mut tree, &WithShapers, holder, WINDOW, GRID, &mut text);

  assert_eq!(text.asked, ["measure", "arrange"]);
}

// LayoutManager::measure lends the engine to measure too, and arranges nothing.
#[test]
fn measure_only_reaches_the_text_engine() {
  let (mut tree, holder, _) = holding_a_shaper();
  let mut text = Recorder::default();

  LayoutManager::default().measure(&mut tree, &WithShapers, holder, WINDOW, &mut text);

  assert_eq!(text.asked, ["measure"]);
}

// Arranging hands the node the same engine, after measuring.
#[test]
fn arrange_reaches_the_text_engine() {
  let (mut tree, holder, _) = holding_a_shaper();
  let mut layout = LayoutManager::default();
  let mut text = Recorder::default();
  layout.run(&mut tree, &WithShapers, holder, WINDOW, GRID, &mut text);
  text.asked.clear();

  layout.run(&mut tree, &WithShapers, holder, Size::new(300.0, 200.0), GRID, &mut text);

  assert_eq!(text.asked, ["arrange"]);
}

fn shaper(tree: &UiTree, id: NodeId) -> &Shaper {
  tree.get_as::<Shaper>(id).expect("a shaper")
}

fn result(layout: &LayoutManager, node: NodeId) -> Option<u32> {
  layout.result(node).and_then(|result| result.downcast_ref::<u32>()).copied()
}

fn lay_out(layout: &mut LayoutManager, tree: &mut UiTree, root: NodeId, available: Size) -> LayoutStats {
  layout.run(tree, &WithShapers, root, available, GRID, &mut Recorder::default())
}

// What a measure set is kept, beside the size it returned.
#[test]
fn measured_result_is_kept_with_the_size() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).measured.set(Some(1));
  let mut layout = LayoutManager::default();

  lay_out(&mut layout, &mut tree, holder, WINDOW);

  assert_eq!(result(&layout, node), Some(1));
  assert_eq!(layout.desired(node), Some(Size::new(50.0, 10.0)));
}

// A measure answered from the cache doesn't run, so the result it set before stays.
#[test]
fn cached_measure_keeps_its_result() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).measured.set(Some(1));
  let mut layout = LayoutManager::default();
  lay_out(&mut layout, &mut tree, holder, WINDOW);
  shaper(&tree, node).measured.set(Some(2));

  lay_out(&mut layout, &mut tree, holder, WINDOW);

  assert_eq!(result(&layout, node), Some(1));
}

// A measure that runs without setting a result drops the one before.
#[test]
fn measure_without_a_result_drops_the_old_one() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).measured.set(Some(1));
  let mut layout = LayoutManager::default();
  lay_out(&mut layout, &mut tree, holder, WINDOW);
  shaper(&tree, node).measured.set(None);
  LayoutManager::invalidate_measure(&mut tree, node);

  lay_out(&mut layout, &mut tree, holder, WINDOW);

  assert_eq!(result(&layout, node), None);
}

// When a measure sets its result twice, the last value is kept, and the node is reported once.
#[test]
fn second_set_measured_wins() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).measured.set(Some(1));
  shaper(&tree, node).twice.set(true);
  let mut layout = LayoutManager::default();

  let stats = lay_out(&mut layout, &mut tree, holder, WINDOW);

  assert_eq!(result(&layout, node), Some(1));
  assert_eq!(stats.results, [node]);
}

// The arranged result is read first: render draws what arrange made for the final size.
#[test]
fn arranged_result_replaces_the_measured_one() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).measured.set(Some(1));
  shaper(&tree, node).arranged.set(Some(2));
  let mut layout = LayoutManager::default();

  lay_out(&mut layout, &mut tree, holder, WINDOW);

  assert_eq!(result(&layout, node), Some(2));
}

// An arrange that only moves the node keeps its arranged result and can read it back.
#[test]
fn moving_keeps_the_arranged_result() {
  let mut tree = UiTree::new();
  let above = leaf(&mut tree, 50.0, 10.0);
  let shaped = tree.alloc(Box::<Shaper>::default());
  let root = stack(&mut tree, &[above, shaped]);
  shaper(&tree, shaped).arranged.set(Some(2));
  let mut layout = LayoutManager::default();
  lay_out(&mut layout, &mut tree, root, WINDOW);
  shaper(&tree, shaped).arranged.set(None);
  node(&tree, above).size.set(Size::new(50.0, 30.0));
  LayoutManager::invalidate_measure(&mut tree, above);

  lay_out(&mut layout, &mut tree, root, WINDOW);

  assert_eq!(layout.rect(shaped).map(|rect| rect.y), Some(30.0));
  assert_eq!(shaper(&tree, shaped).read.get(), Some(2));
  assert_eq!(result(&layout, shaped), Some(2));
}

// Arranged at another size, a node loses the arranged result made for the old one.
#[test]
fn resizing_drops_the_arranged_result() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).arranged.set(Some(2));
  let mut layout = LayoutManager::default();
  lay_out(&mut layout, &mut tree, holder, WINDOW);
  shaper(&tree, node).arranged.set(None);

  lay_out(&mut layout, &mut tree, holder, Size::new(120.0, 80.0));

  assert_eq!(shaper(&tree, node).read.get(), None);
  assert_eq!(result(&layout, node), None);
}

// A new measure drops the arranged result, even when the node is then arranged at the same size.
#[test]
fn measuring_drops_the_arranged_result() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).arranged.set(Some(2));
  let mut layout = LayoutManager::default();
  lay_out(&mut layout, &mut tree, holder, WINDOW);
  shaper(&tree, node).arranged.set(None);
  LayoutManager::invalidate_measure(&mut tree, node);

  lay_out(&mut layout, &mut tree, holder, WINDOW);

  assert_eq!(result(&layout, node), None);
}

// A node taken out of the tree takes its results with it.
#[test]
fn dropped_node_drops_its_results() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).measured.set(Some(1));
  let mut layout = LayoutManager::default();
  lay_out(&mut layout, &mut tree, holder, WINDOW);
  tree.detach(TreeKind::Visual, node);
  LayoutManager::invalidate_measure(&mut tree, holder);

  lay_out(&mut layout, &mut tree, holder, WINDOW);

  assert!(layout.result(node).is_none());
}

// A node that collapses isn't measured by its parent any more, and loses its results.
#[test]
fn collapsed_node_drops_its_results() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).measured.set(Some(1));
  let mut layout = LayoutManager::default();
  lay_out(&mut layout, &mut tree, holder, WINDOW);
  shaper(&tree, node).collapsed.set(true);
  LayoutManager::invalidate_measure(&mut tree, holder);

  lay_out(&mut layout, &mut tree, holder, WINDOW);

  assert!(layout.result(node).is_none());
}

// A run reports each node whose result was set or dropped, once.
#[test]
fn set_and_dropped_results_are_reported() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).measured.set(Some(1));
  let mut layout = LayoutManager::default();

  assert_eq!(lay_out(&mut layout, &mut tree, holder, WINDOW).results, [node]);

  shaper(&tree, node).measured.set(None);
  LayoutManager::invalidate_measure(&mut tree, node);
  assert_eq!(lay_out(&mut layout, &mut tree, holder, WINDOW).results, [node]);
}

// A result set by LayoutManager::measure is reported by the next run, which measures nothing.
#[test]
fn results_from_measure_reach_the_next_run() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).measured.set(Some(1));
  let mut layout = LayoutManager::default();
  layout.measure(&mut tree, &WithShapers, holder, WINDOW, &mut Recorder::default());

  let stats = lay_out(&mut layout, &mut tree, holder, WINDOW);

  assert_eq!(stats.measured, 0);
  assert_eq!(stats.results, [node]);
}

// A run that changes nothing reports no results.
#[test]
fn unchanged_layout_reports_no_results() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).measured.set(Some(1));
  let mut layout = LayoutManager::default();
  lay_out(&mut layout, &mut tree, holder, WINDOW);

  assert_eq!(lay_out(&mut layout, &mut tree, holder, WINDOW).results, Vec::<NodeId>::new());
}

// An arranged result leaves the measured size alone, so the parent sees what the node asked for.
#[test]
fn parent_still_sees_the_measured_size() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).arranged.set(Some(2));
  let mut layout = LayoutManager::default();

  lay_out(&mut layout, &mut tree, holder, WINDOW);

  assert_eq!(layout.desired(node), Some(Size::new(50.0, 10.0)));
  assert_eq!(layout.desired(holder), Some(Size::new(50.0, 10.0)));
}

// A result read back as another type is nothing, not a panic.
#[test]
fn wrong_type_reads_nothing() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).measured.set(Some(1));
  let mut layout = LayoutManager::default();

  lay_out(&mut layout, &mut tree, holder, WINDOW);

  assert!(
    layout
      .result(node)
      .and_then(|result| result.downcast_ref::<String>())
      .is_none()
  );
}

// A new pixel grid arranges every node again, so an arranged result made on the old grid is
// dropped even at the same size.
#[test]
fn new_grid_drops_the_arranged_result() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).arranged.set(Some(2));
  let mut layout = LayoutManager::default();
  lay_out(&mut layout, &mut tree, holder, WINDOW);
  shaper(&tree, node).arranged.set(None);

  let grid = PixelGrid { scale: 2.0, snap: false };
  layout.run(&mut tree, &WithShapers, holder, WINDOW, grid, &mut Recorder::default());

  assert_eq!(shaper(&tree, node).read.get(), None);
  assert_eq!(result(&layout, node), None);
}

// A node marked to arrange again loses its arranged result, even at the same size.
#[test]
fn invalidated_arrange_drops_the_arranged_result() {
  let (mut tree, holder, node) = holding_a_shaper();
  shaper(&tree, node).arranged.set(Some(2));
  let mut layout = LayoutManager::default();
  lay_out(&mut layout, &mut tree, holder, WINDOW);
  shaper(&tree, node).arranged.set(None);
  layout.invalidate_arrange(node);

  let stats = lay_out(&mut layout, &mut tree, holder, WINDOW);

  assert_eq!(result(&layout, node), None);
  assert_eq!(stats.results, [node]);
}
