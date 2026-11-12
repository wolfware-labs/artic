// The accessibility pass over headless frames: layout, the pass, then composition, in the order
// the pipeline runs them. Every update goes through accesskit_consumer, which panics on one the OS
// adapters would reject.

mod common;

use accesskit::{Affine, Node, Role, TreeId, TreeInfo, TreeUpdate};
use accesskit_consumer::{NodeRef, TreeChangeHandler};
use artic_core::Object;
use artic_core::property::{Affects, Inheritance, Property, PropertyKey, PropertyVisitor};
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_element::{AccessCx, Element, ElementNodes, ElementTypes, RenderCx};
use artic_layout::geometry::{Rect, Size};
use artic_layout::{ArrangeCx, LayoutManager, Layoutable, MeasureCx, PixelGrid};
use artic_platform::AccessTree;
use artic_render::{Compositor, SceneBuilder};
use common::{column, label, leaf};
use pretty_assertions::assert_eq;
use std::collections::HashSet;

static NAME: PropertyKey<String> = PropertyKey::new("Name", Affects::RENDER, Inheritance::None);

/// A 10 by 10 button named by a property that affects render only, with bounds of its size at its
/// origin, as `Interaction` gives every control.
#[derive(Debug)]
struct Named {
  name: Property<String>,
  collapsed: bool,
}

impl Object for Named {
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
    visitor.visit(&self.name);
  }
}

impl Layoutable for Named {
  fn measure(&self, _cx: &mut MeasureCx<'_>, _available: Size) -> Size {
    Size::new(10.0, 10.0)
  }

  fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}

  fn collapsed(&self) -> bool {
    self.collapsed
  }
}

impl Element for Named {
  fn render(&self, _cx: &mut RenderCx<'_>, _scene: &mut SceneBuilder) {}

  fn accessibility(&self, cx: &AccessCx<'_>) -> Node {
    let mut node = Node::new(Role::Button);
    node.set_label(self.name.get());
    let size = cx.bounds().size();
    node.set_bounds(accesskit::Rect::new(0.0, 0.0, f64::from(size.width), f64::from(size.height)));
    node
  }
}

artic_element::element_type!(Named);

fn named(tree: &mut UiTree, name: &str) -> (NodeId, Property<String>) {
  let name = Property::new(&NAME, name.to_owned());
  (tree.alloc(Box::new(Named { name, collapsed: false })), name)
}

/// A node that isn't an element.
#[derive(Debug)]
struct Plain;

impl Object for Plain {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

struct Ignore;

impl TreeChangeHandler for Ignore {
  fn node_added(&mut self, _node: &NodeRef) {}
  fn node_updated(&mut self, _old: &NodeRef, _new: &NodeRef) {}
  fn focus_moved(&mut self, _old: Option<&NodeRef>, _new: Option<&NodeRef>) {}
  fn node_removed(&mut self, _node: &NodeRef) {}
}

/// One surface laid out in 100 by 100 at `scale`, with its pass and the consumer that checks what
/// the pass sends.
struct Window {
  tree: UiTree,
  root: NodeId,
  types: ElementTypes,
  layout: LayoutManager,
  compositor: Compositor,
  access: AccessTree,
  oracle: Option<accesskit_consumer::Tree>,
  scale: f32,
}

impl Window {
  fn new(tree: UiTree, root: NodeId) -> Self {
    Self {
      tree,
      root,
      types: ElementTypes::from_link_time(),
      layout: LayoutManager::default(),
      compositor: Compositor::default(),
      access: AccessTree::new(),
      oracle: None,
      scale: 1.0,
    }
  }

  /// One frame: layout, the pass, then composition, which clears the render flags.
  fn frame(&mut self) -> Option<TreeUpdate> {
    let grid = PixelGrid {
      scale: self.scale,
      snap: true,
    };
    let mut text = common::engine();
    let stats = self.layout.run(
      &mut self.tree,
      &self.types,
      self.root,
      Size::new(100.0, 100.0),
      grid,
      &mut text,
    );
    let stale: HashSet<NodeId> = stats.resized.into_iter().chain(stats.results).collect();
    let nodes = ElementNodes {
      types: &self.types,
      layout: &self.layout,
    };
    let update = self.access.update(&self.tree, self.root, &nodes, &stale, self.scale);
    let mut nodes = ElementNodes {
      types: &self.types,
      layout: &self.layout,
    };
    self.compositor.compose(&mut self.tree, self.root, &mut nodes);
    if let Some(update) = &update {
      match &mut self.oracle {
        None => self.oracle = Some(accesskit_consumer::Tree::new(update.clone(), true)),
        Some(oracle) => oracle.update_and_process_changes(update.clone(), &mut Ignore),
      }
    }
    update
  }
}

fn id(node: NodeId) -> accesskit::NodeId {
  accesskit::NodeId(node.to_bits())
}

fn sent(update: &TreeUpdate) -> Vec<accesskit::NodeId> {
  let mut ids: Vec<_> = update.nodes.iter().map(|(id, _)| *id).collect();
  ids.sort();
  ids
}

fn ids(nodes: &[NodeId]) -> Vec<accesskit::NodeId> {
  let mut ids: Vec<_> = nodes.iter().copied().map(id).collect();
  ids.sort();
  ids
}

fn node(update: &TreeUpdate, node: NodeId) -> &Node {
  &update
    .nodes
    .iter()
    .find(|(sent, _)| *sent == id(node))
    .expect("the node was sent")
    .1
}

// The first update is the whole tree: every visible node, children in visual order, the root
// focused.
#[test]
fn first_update_is_the_whole_tree() {
  let mut tree = UiTree::new();
  let (first, _) = named(&mut tree, "first");
  let (second, _) = named(&mut tree, "second");
  let (root, _) = column(&mut tree, &[first, second]);
  let mut window = Window::new(tree, root);

  let update = window.frame().expect("a full update");

  assert_eq!(update.tree, Some(TreeInfo::new(id(root))));
  assert_eq!(update.tree_id, TreeId::ROOT);
  assert_eq!(update.focus, id(root));
  assert_eq!(sent(&update), ids(&[root, first, second]));
  assert_eq!(node(&update, root).children(), [id(first), id(second)]);
}

// A frame where nothing a screen reader sees changed sends nothing.
#[test]
fn unchanged_frame_sends_nothing() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, 20.0);
  let (root, _) = column(&mut tree, &[child.id]);
  let mut window = Window::new(tree, root);
  window.frame();

  common::set_color(child.color, 0x00ff_0000);

  assert_eq!(window.frame(), None);
}

// A render-dirty node is described again, and goes alone, as an incremental update.
#[test]
fn render_change_sends_that_node() {
  let mut tree = UiTree::new();
  let (button, name) = named(&mut tree, "old");
  let (root, _) = column(&mut tree, &[button]);
  let mut window = Window::new(tree, root);
  window.frame();

  name.set("new".to_owned());
  let update = window.frame().expect("an update");

  assert_eq!(update.tree, None);
  assert_eq!(sent(&update), ids(&[button]));
  assert_eq!(node(&update, button).label(), Some("new"));
}

// A node that layout gave a new result, in a rect that stayed the same, is described again.
#[test]
fn layout_result_change_sends_that_node() {
  let mut tree = UiTree::new();
  let (text, value) = label(&mut tree, "abc");
  let (root, _) = column(&mut tree, &[text]);
  let mut window = Window::new(tree, root);
  window.frame();

  value.set("xyz".to_owned());
  let update = window.frame().expect("an update");

  assert_eq!(sent(&update), ids(&[text]));
  assert_eq!(node(&update, text).value(), Some("xyz"));
}

// A node layout moved is sent alone, with its new offset; its descendants stay as they were, and so
// does a node layout resized whose description doesn't depend on its size.
#[test]
fn moved_node_sends_only_itself() {
  let mut tree = UiTree::new();
  let above = leaf(&mut tree, 20.0);
  let (button, _) = named(&mut tree, "b");
  let (inner, _) = column(&mut tree, &[button]);
  let (root, _) = column(&mut tree, &[above.id, inner]);
  let mut window = Window::new(tree, root);
  window.frame();

  above.height.set(30.0);
  let update = window.frame().expect("an update");

  assert_eq!(sent(&update), ids(&[inner]));
  assert_eq!(node(&update, inner).transform(), Some(&Affine::translate((0.0, 30.0))));
}

// Each node's bounds are its size at its own origin; its transform places it in its parent.
#[test]
fn nodes_are_placed_by_their_transform() {
  let mut tree = UiTree::new();
  let above = leaf(&mut tree, 20.0);
  let (button, _) = named(&mut tree, "b");
  let (root, _) = column(&mut tree, &[above.id, button]);
  let mut window = Window::new(tree, root);

  let update = window.frame().expect("a full update");

  assert_eq!(node(&update, above.id).transform(), None);
  assert_eq!(node(&update, button).transform(), Some(&Affine::translate((0.0, 20.0))));
  assert_eq!(
    node(&update, button).bounds(),
    Some(accesskit::Rect::new(0.0, 0.0, 100.0, 10.0))
  );
}

// A node's own transform comes before its offset, as the compositor draws it.
#[test]
fn render_transform_is_part_of_the_node_transform() {
  let mut tree = UiTree::new();
  let above = leaf(&mut tree, 20.0);
  let turned = leaf(&mut tree, 10.0);
  let (root, _) = column(&mut tree, &[above.id, turned.id]);
  let mut window = Window::new(tree, root);
  window.frame();

  turned.turn.set(true);
  let update = window.frame().expect("an update");

  assert_eq!(
    node(&update, turned.id).transform(),
    Some(&(Affine::translate((0.0, 20.0)) * Affine::scale(2.0)))
  );
}

// A hidden subtree is left out, and its parent lists it no more; shown again, it comes back whole.
#[test]
fn hidden_subtree_leaves_and_comes_back_whole() {
  let mut tree = UiTree::new();
  let (button, _) = named(&mut tree, "b");
  let (inner, visible) = column(&mut tree, &[button]);
  let (root, _) = column(&mut tree, &[inner]);
  let mut window = Window::new(tree, root);
  window.frame();

  visible.set(false);
  let hidden = window.frame().expect("an update");
  visible.set(true);
  let shown = window.frame().expect("an update");

  assert_eq!(sent(&hidden), ids(&[root]));
  assert_eq!(node(&hidden, root).children(), []);
  assert_eq!(sent(&shown), ids(&[root, inner, button]));
}

// A reordered child list and a node moved to another parent send the parents whose lists changed.
#[test]
fn reorder_and_reparent_send_the_parents() {
  let mut tree = UiTree::new();
  let (a, _) = named(&mut tree, "a");
  let (b, _) = named(&mut tree, "b");
  let (c, _) = named(&mut tree, "c");
  let (left, _) = column(&mut tree, &[a, b]);
  let (right, _) = column(&mut tree, &[c]);
  let (root, _) = column(&mut tree, &[left, right]);
  let mut window = Window::new(tree, root);
  window.frame();

  window.tree.detach(TreeKind::Visual, a);
  window.tree.add_child(TreeKind::Visual, left, a);
  window.tree.detach(TreeKind::Visual, c);
  window.tree.add_child(TreeKind::Visual, left, c);
  let update = window.frame().expect("an update");

  assert_eq!(node(&update, left).children(), [id(b), id(a), id(c)]);
  assert_eq!(node(&update, right).children(), []);
  assert!(!sent(&update).contains(&id(root)));
}

// A freed node leaves its parent's list, and is described anew if its slot is reused.
#[test]
fn freed_node_leaves_its_parent() {
  let mut tree = UiTree::new();
  let (a, _) = named(&mut tree, "a");
  let (b, _) = named(&mut tree, "b");
  let (root, _) = column(&mut tree, &[a, b]);
  let mut window = Window::new(tree, root);
  window.frame();

  window.tree.free(b);
  let update = window.frame().expect("an update");

  assert_eq!(node(&update, root).children(), [id(a)]);
}

// A hidden root is still described, with no children, since a tree must hold its root.
#[test]
fn hidden_root_is_described_without_children() {
  let mut tree = UiTree::new();
  let (button, _) = named(&mut tree, "b");
  let (root, visible) = column(&mut tree, &[button]);
  visible.set(false);
  let mut window = Window::new(tree, root);

  let update = window.frame().expect("a full update");

  assert_eq!(sent(&update), ids(&[root]));
  assert_eq!(node(&update, root).children(), []);
}

// A collapsed root gets no rect from layout, so it is described at zero size.
#[test]
fn collapsed_root_has_zero_size() {
  let mut tree = UiTree::new();
  let name = Property::new(&NAME, "root".to_owned());
  let root = tree.alloc(Box::new(Named { name, collapsed: true }));
  let mut window = Window::new(tree, root);

  let update = window.frame().expect("a full update");

  assert_eq!(node(&update, root).bounds(), Some(accesskit::Rect::new(0.0, 0.0, 0.0, 0.0)));
}

// The root is scaled to device pixels, and a new scale sends it alone.
#[test]
fn scale_change_sends_the_root() {
  let mut tree = UiTree::new();
  let (button, _) = named(&mut tree, "b");
  let (root, _) = column(&mut tree, &[button]);
  let mut window = Window::new(tree, root);
  window.frame();

  window.scale = 2.0;
  let update = window.frame().expect("an update");

  assert_eq!(sent(&update), ids(&[root]));
  assert_eq!(node(&update, root).transform(), Some(&Affine::scale(2.0)));
}

// A visual node that isn't an element is a generic container of its size, so the nodes below it stay
// where they are drawn.
#[test]
fn node_that_isnt_an_element_is_a_container() {
  let mut tree = UiTree::new();
  let plain = tree.alloc(Box::new(Plain));
  let (root, _) = column(&mut tree, &[plain]);
  let mut window = Window::new(tree, root);

  let update = window.frame().expect("a full update");

  assert_eq!(node(&update, plain).role(), Role::GenericContainer);
  assert_eq!(
    node(&update, plain).bounds(),
    Some(accesskit::Rect::new(0.0, 0.0, 100.0, 0.0))
  );
}

// After a full request the next update is the whole tree again, even with nothing changed.
#[test]
fn full_request_sends_the_whole_tree() {
  let mut tree = UiTree::new();
  let (button, _) = named(&mut tree, "b");
  let (root, _) = column(&mut tree, &[button]);
  let mut window = Window::new(tree, root);
  window.frame();

  window.access.request_full();
  window.oracle = None;
  let update = window.frame().expect("a full update");

  assert_eq!(update.tree, Some(TreeInfo::new(id(root))));
  assert_eq!(sent(&update), ids(&[root, button]));
}

// A new pass over a tree another pass described starts with the whole tree.
#[test]
fn new_pass_starts_full() {
  let mut tree = UiTree::new();
  let (button, _) = named(&mut tree, "b");
  let (root, _) = column(&mut tree, &[button]);
  let mut window = Window::new(tree, root);
  window.frame();

  window.access = AccessTree::new();
  window.oracle = None;
  let update = window.frame().expect("a full update");

  assert_eq!(update.tree, Some(TreeInfo::new(id(root))));
}
