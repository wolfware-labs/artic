use crate::Object;
use crate::property::{Affects, AnyProperty, Inheritance, Precedence, Property, PropertyKey, inherit};
use crate::tree::{NodeId, TreeKind, UiTree};
use pretty_assertions::assert_eq;

static FONT_SIZE: PropertyKey<i32> = PropertyKey::new("FontSize", Affects::LAYOUT, Inheritance::Replace);
static WIDTH: PropertyKey<i32> = PropertyKey::new("Width", Affects::LAYOUT, Inheritance::None);
static ENABLED: PropertyKey<bool> = PropertyKey::new("Enabled", Affects::RENDER, Inheritance::Combine(inherit::and));

/// A root node with an inheritable font size.
#[derive(Debug)]
struct Window {
  font_size: Property<i32>,
}

impl Object for Window {
  fn visit_properties(&self, visit: &mut dyn FnMut(&dyn AnyProperty)) {
    visit(&self.font_size);
  }
}

/// A container without a font size, which inheritance passes through.
#[derive(Debug)]
struct StackPanel;

impl Object for StackPanel {
  fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
}

/// A leaf with an inheritable font size and a width that doesn't inherit.
#[derive(Debug)]
struct TextBlock {
  font_size: Property<i32>,
  width: Property<i32>,
}

impl Object for TextBlock {
  fn visit_properties(&self, visit: &mut dyn FnMut(&dyn AnyProperty)) {
    visit(&self.font_size);
    visit(&self.width);
  }
}

/// A node whose enabled flag combines with its ancestors'.
#[derive(Debug)]
struct Toggle {
  enabled: Property<bool>,
}

impl Object for Toggle {
  fn visit_properties(&self, visit: &mut dyn FnMut(&dyn AnyProperty)) {
    visit(&self.enabled);
  }
}

/// Allocates a toggle with `enabled` set locally.
fn new_toggle(tree: &mut UiTree, enabled: bool) -> (NodeId, Property<bool>) {
  let property = Property::new(&ENABLED, true);
  property.set(enabled);
  (tree.alloc(Box::new(Toggle { enabled: property })), property)
}

/// Allocates a window with `font_size` set locally.
fn new_window(tree: &mut UiTree, font_size: i32) -> (NodeId, Property<i32>) {
  let property = Property::new(&FONT_SIZE, 0);
  property.set(font_size);
  (tree.alloc(Box::new(Window { font_size: property })), property)
}

/// Allocates a text block with nothing set, and returns its font size and width.
fn new_text_block(tree: &mut UiTree) -> (NodeId, Property<i32>, Property<i32>) {
  let font_size = Property::new(&FONT_SIZE, 10);
  let width = Property::new(&WIDTH, 100);
  (tree.alloc(Box::new(TextBlock { font_size, width })), font_size, width)
}

// An inheritable value set on a window reaches a text block two logical levels down, through a
// container type that doesn't have the property.
#[test]
fn inherited_value_flows_from_logical_ancestor() {
  let mut tree = UiTree::new();
  let (window, window_font_size) = new_window(&mut tree, 20);
  let panel = tree.alloc(Box::new(StackPanel));
  let (text, font_size, _) = new_text_block(&mut tree);

  tree.add_child(TreeKind::Logical, window, panel);
  tree.add_child(TreeKind::Logical, panel, text);

  assert_eq!(font_size.get(), 20);
  assert_eq!(font_size.effective_source(), Precedence::Inherited);
  window_font_size.set(24);
  assert_eq!(font_size.get(), 24);
}

// A node with its own value hides the inherited one, for itself and for everything below it.
#[test]
fn local_value_blocks_inheritance() {
  let mut tree = UiTree::new();
  let (window, window_font_size) = new_window(&mut tree, 20);
  let (border, border_font_size, _) = new_text_block(&mut tree);
  let (text, font_size, _) = new_text_block(&mut tree);
  border_font_size.set(12);
  tree.add_child(TreeKind::Logical, window, border);
  tree.add_child(TreeKind::Logical, border, text);
  tree.clear_dirty(text, Affects::LAYOUT | Affects::RENDER);

  window_font_size.set(30);

  assert_eq!(font_size.get(), 12);
  assert_eq!(tree.dirty(text), Affects::NONE);
}

// A change on an ancestor marks dirty every node that inherits it.
#[test]
fn inherited_change_marks_inheriting_nodes_dirty() {
  let mut tree = UiTree::new();
  let (window, window_font_size) = new_window(&mut tree, 20);
  let panel = tree.alloc(Box::new(StackPanel));
  let (text, _, _) = new_text_block(&mut tree);
  tree.add_child(TreeKind::Logical, window, panel);
  tree.add_child(TreeKind::Logical, panel, text);
  tree.clear_dirty(text, Affects::LAYOUT | Affects::RENDER);

  window_font_size.set(24);

  assert_eq!(tree.dirty(text), Affects::LAYOUT);
}

// Moving a node to another logical parent makes it inherit from its new ancestors, and marks it
// dirty.
#[test]
fn reparent_updates_inherited_value() {
  let mut tree = UiTree::new();
  let (large, _) = new_window(&mut tree, 20);
  let (small, _) = new_window(&mut tree, 12);
  let (text, font_size, _) = new_text_block(&mut tree);
  tree.add_child(TreeKind::Logical, large, text);
  tree.clear_dirty(text, Affects::LAYOUT | Affects::RENDER);

  tree.add_child(TreeKind::Logical, small, text);

  assert_eq!(font_size.get(), 12);
  assert_eq!(tree.dirty(text), Affects::LAYOUT);
}

// Detaching a node from the logical tree leaves it with nothing to inherit from.
#[test]
fn detach_stops_inheriting() {
  let mut tree = UiTree::new();
  let (window, _) = new_window(&mut tree, 20);
  let (text, font_size, _) = new_text_block(&mut tree);
  tree.add_child(TreeKind::Logical, window, text);

  tree.detach(TreeKind::Logical, text);

  assert_eq!(font_size.get(), 10);
  assert_eq!(font_size.effective_source(), Precedence::Default);
}

// Inheritance follows the logical parent, not a template part that is the visual parent.
#[test]
fn inheritance_follows_logical_not_visual_parent() {
  let mut tree = UiTree::new();
  let (window, _) = new_window(&mut tree, 20);
  let (template_part, _) = new_window(&mut tree, 12);
  let (text, font_size, _) = new_text_block(&mut tree);

  tree.add_child(TreeKind::Logical, window, text);
  tree.add_child(TreeKind::Visual, window, template_part);
  tree.add_child(TreeKind::Visual, template_part, text);

  assert_eq!(font_size.get(), 20);
}

// A property whose key doesn't inherit keeps its own value under an ancestor that has one.
#[test]
fn non_inheriting_property_is_not_linked() {
  let mut tree = UiTree::new();
  let (parent, _, parent_width) = new_text_block(&mut tree);
  let (child, _, width) = new_text_block(&mut tree);
  parent_width.set(300);

  tree.add_child(TreeKind::Logical, parent, child);

  assert_eq!(width.get(), 100);
}

// Freeing an ancestor's subtree unlinks the freed followers, so a later change on the surviving
// root walks nothing that was freed.
#[test]
fn freeing_a_subtree_unlinks_its_followers() {
  let mut tree = UiTree::new();
  let (window, window_font_size) = new_window(&mut tree, 20);
  let panel = tree.alloc(Box::new(StackPanel));
  let (text, font_size, _) = new_text_block(&mut tree);
  tree.add_child(TreeKind::Logical, window, panel);
  tree.add_child(TreeKind::Logical, panel, text);

  tree.free(panel);
  window_font_size.set(24);

  assert_eq!(font_size.effective_source(), Precedence::Default);
  assert!(!tree.contains(text));
}

// Moving a container re-resolves everything below it: a text block that inherited through the
// container now inherits from the container's new ancestors.
#[test]
fn reparenting_a_container_updates_its_descendants() {
  let mut tree = UiTree::new();
  let (large, _) = new_window(&mut tree, 20);
  let (small, _) = new_window(&mut tree, 12);
  let panel = tree.alloc(Box::new(StackPanel));
  let (text, font_size, _) = new_text_block(&mut tree);
  tree.add_child(TreeKind::Logical, large, panel);
  tree.add_child(TreeKind::Logical, panel, text);

  tree.add_child(TreeKind::Logical, small, panel);

  assert_eq!(font_size.get(), 12);
}

// A combining value ANDs down a chain: a disabled root disables a grandchild that enables itself,
// through a parent that enables itself too.
#[test]
fn combine_chains_through_ancestors() {
  let mut tree = UiTree::new();
  let (root, _) = new_toggle(&mut tree, false);
  let (middle, _) = new_toggle(&mut tree, true);
  let (leaf, enabled) = new_toggle(&mut tree, true);
  tree.add_child(TreeKind::Logical, root, middle);
  tree.add_child(TreeKind::Logical, middle, leaf);

  assert!(!enabled.get());
}

// A combining node is marked dirty when its source changes, even with a value of its own.
#[test]
fn combining_follower_is_marked_dirty_despite_own_value() {
  let mut tree = UiTree::new();
  let (parent, parent_enabled) = new_toggle(&mut tree, true);
  let (child, _) = new_toggle(&mut tree, true);
  tree.add_child(TreeKind::Logical, parent, child);
  tree.clear_dirty(child, Affects::LAYOUT | Affects::RENDER);

  parent_enabled.set(false);

  assert_eq!(tree.dirty(child), Affects::RENDER);
}

// A change reaches a combining grandchild through a parent with its own value, marking both dirty.
#[test]
fn combine_marks_dirty_through_a_node_with_its_own_value() {
  let mut tree = UiTree::new();
  let (root, root_enabled) = new_toggle(&mut tree, true);
  let (middle, _) = new_toggle(&mut tree, true);
  let (leaf, enabled) = new_toggle(&mut tree, true);
  tree.add_child(TreeKind::Logical, root, middle);
  tree.add_child(TreeKind::Logical, middle, leaf);
  tree.clear_dirty(middle, Affects::LAYOUT | Affects::RENDER);
  tree.clear_dirty(leaf, Affects::LAYOUT | Affects::RENDER);

  root_enabled.set(false);

  assert_eq!((tree.dirty(middle), tree.dirty(leaf)), (Affects::RENDER, Affects::RENDER));
  assert!(!enabled.get());
  root_enabled.set(true);
  assert!(enabled.get());
}

// Freeing a node whose value was inherited stops it following its parent without setting the
// pending flag: the node is gone, so nothing needs a frame.
#[test]
fn freeing_an_inheriting_node_sets_no_pending() {
  let mut tree = UiTree::new();
  let font_size = Property::new(&FONT_SIZE, 12);
  let window = tree.alloc(Box::new(Window { font_size }));
  font_size.set(20);
  let text = tree.alloc(Box::new(TextBlock {
    font_size: Property::new(&FONT_SIZE, 12),
    width: Property::new(&WIDTH, 0),
  }));
  tree.add_child(TreeKind::Logical, window, text);
  let _ = tree.take_pending();

  tree.free(text);

  assert!(!tree.take_pending());
}
