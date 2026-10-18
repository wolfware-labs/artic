use super::TOLERANCE;
use super::compare::{rects_match, sizes_match};
use artic_core::Object;
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_element::{ElementTypes, RenderCx};
use artic_layout::geometry::{Rect, Size};
use artic_layout::{LayoutManager, LayoutNodes, LayoutStats, PixelGrid};
use artic_render::{Command, Compositor, Layers, Nodes, SceneBuilder, SceneItem};
use std::collections::HashMap;

/// A tree laid out with no window and no GPU. Nodes get a label when they're added, and failures
/// name them by it.
#[derive(Debug)]
pub struct Headless<N: LayoutNodes> {
  pub(super) tree: UiTree,
  pub(super) manager: LayoutManager,
  pub(super) nodes: N,
  pub(super) text: artic_text::Engine,
  pub(super) snap: bool,
  labels: HashMap<NodeId, String>,
}

impl<N: LayoutNodes> Headless<N> {
  /// An empty tree whose nodes `nodes` lays out, with snapping on.
  #[must_use]
  pub fn new(nodes: N) -> Headless<N> {
    Headless {
      tree: UiTree::new(),
      manager: LayoutManager::default(),
      nodes,
      text: crate::text::engine(),
      snap: true,
      labels: HashMap::new(),
    }
  }

  /// Adds `object` as a node with no parent, labeled `label`.
  pub fn add(&mut self, label: &str, object: impl Object) -> NodeId {
    let node = self.tree.alloc(Box::new(object));
    self.labels.insert(node, label.to_owned());
    node
  }

  /// Adds `object` labeled `label`, with `children` as its visual children in order.
  pub fn parent(&mut self, label: &str, object: impl Object, children: &[NodeId]) -> NodeId {
    let node = self.add(label, object);
    for child in children {
      self.tree.add_child(TreeKind::Visual, node, *child);
    }
    node
  }

  /// The tree, for what `Headless` doesn't wrap.
  pub fn tree(&mut self) -> &mut UiTree {
    &mut self.tree
  }

  /// The layout manager, for what `Headless` doesn't wrap.
  pub fn manager(&mut self) -> &mut LayoutManager {
    &mut self.manager
  }

  /// Turns snapping to the device-pixel grid on or off for the next runs. It's on by default, as
  /// in a window.
  pub fn set_snap(&mut self, snap: bool) {
    self.snap = snap;
  }

  /// Lays out `root`'s subtree in `available`, at `scale` device pixels per logical pixel, shaping
  /// text with the test engine.
  pub fn run(&mut self, root: NodeId, available: Size, scale: f32) -> LayoutStats {
    let grid = PixelGrid { scale, snap: self.snap };
    self
      .manager
      .run(&mut self.tree, &self.nodes, root, available, grid, &mut self.text)
  }

  /// Records `node`'s own drawing as the frame pipeline would: at the size layout gave it, with
  /// the result its layout kept. Its children aren't drawn.
  pub fn record(&mut self, node: NodeId, types: &ElementTypes) -> Vec<Command> {
    let size = self.manager.rect(node).map_or(Size::ZERO, |rect| rect.size());
    let mut one = OneNode {
      types,
      manager: &self.manager,
      node,
      size,
    };
    Compositor::default()
      .compose(&mut self.tree, node, &mut one)
      .items()
      .iter()
      .find_map(|item| match item {
        SceneItem::Fragment(fragment) => Some(fragment.commands().to_vec()),
        _ => None,
      })
      .unwrap_or_default()
  }

  /// The label `node` was added with, or its id for a node made through [`tree`](Headless::tree).
  #[must_use]
  pub fn label(&self, node: NodeId) -> String {
    self.labels.get(&node).cloned().unwrap_or_else(|| format!("{node:?}"))
  }

  /// Checks that `node` asked for `expected`, within [`TOLERANCE`].
  ///
  /// # Panics
  ///
  /// When it asked for another size or wasn't measured, naming the node.
  #[track_caller]
  pub fn assert_desired(&self, node: NodeId, expected: Size) {
    self.assert_desired_within(node, expected, TOLERANCE);
  }

  /// Checks that `node` asked for `expected`, within `tolerance` on each axis.
  ///
  /// # Panics
  ///
  /// When it asked for another size or wasn't measured, naming the node.
  #[track_caller]
  pub fn assert_desired_within(&self, node: NodeId, expected: Size, tolerance: f32) {
    let Some(actual) = self.manager.desired(node) else {
      panic!("{}: not measured", self.name(node));
    };
    assert!(
      sizes_match(actual, expected, tolerance),
      "{}: desired expected {expected}, got {actual}",
      self.name(node)
    );
  }

  /// Checks that `node` was arranged in `expected`, in its parent's coordinates, within
  /// [`TOLERANCE`].
  ///
  /// # Panics
  ///
  /// When it was arranged elsewhere or not at all, naming the node.
  #[track_caller]
  pub fn assert_rect(&self, node: NodeId, expected: Rect) {
    self.assert_rect_within(node, expected, TOLERANCE);
  }

  /// Checks that `node` was arranged in `expected`, within `tolerance` on each edge and side.
  ///
  /// # Panics
  ///
  /// When it was arranged elsewhere or not at all, naming the node.
  #[track_caller]
  pub fn assert_rect_within(&self, node: NodeId, expected: Rect, tolerance: f32) {
    let Some(actual) = self.manager.rect(node) else {
      panic!("{}: not arranged", self.name(node));
    };
    assert!(
      rects_match(actual, expected, tolerance),
      "{}: rect expected {expected}, got {actual}",
      self.name(node)
    );
  }

  pub(super) fn name(&self, node: NodeId) -> String {
    match self.labels.get(&node) {
      Some(label) => format!("{label:?}"),
      None => format!("{node:?}"),
    }
  }
}

/// Draws one node of a laid-out tree with what its layout kept.
struct OneNode<'a> {
  types: &'a ElementTypes,
  manager: &'a LayoutManager,
  node: NodeId,
  size: Size,
}

impl Nodes for OneNode<'_> {
  fn layers(&self, _tree: &UiTree, _node: NodeId) -> Layers {
    Layers::default()
  }

  fn record(&mut self, tree: &UiTree, node: NodeId, builder: &mut SceneBuilder) {
    if node != self.node {
      return;
    }
    if let Some(element) = tree.get(node).and_then(|object| self.types.element(object)) {
      element.render(&mut RenderCx::new(tree, node, self.size, self.manager.result(node)), builder);
    }
  }
}
