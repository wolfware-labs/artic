//! The UI tree: every node, the two trees that link them, and what each node needs redone.
//!
//! [`UiTree`] owns every node as a `Box<dyn Object>`, and [`NodeId`] is the handle used to reach one.
//! Handles are checked against a per-slot generation, so a handle to a freed node stops resolving instead of
//! pointing at whatever node reuses its slot.
//!
//! Every node belongs to two trees at once, each with its own parent and ordered children: the logical tree
//! the view declares, and the visual tree that is laid out and drawn. Methods that work on links take a
//! [`TreeKind`] to say which one, such as [`add_child`](UiTree::add_child) and
//! [`descendants`](UiTree::descendants).
//!
//! Every node also has dirty flags saying which frame stages it needs redone. A new node starts with
//! layout and rendering dirty, and a change to one of its properties adds the stages the property's key
//! affects; see [`dirty`](UiTree::dirty).
//!
//! # How this module is organized
//!
//! `UiTree`, defined here with all its methods, is a front over parts with one job each, one file
//! per part:
//!
//! - `objects`: the objects themselves, and the only part that knows whether an id is live.
//! - `hierarchy`: `Tree`, one tree's parent and child links; `UiTree` keeps one per kind.
//! - `dirty`: which frame stages each node needs redone.
//! - `attached`: attached property values, stored per node.
//! - `inheritance`: how inheritable properties follow the logical tree, as functions over the
//!   objects and the logical links.
//!
//! The other parts keep their data under the same `NodeId`, in a `SlotMap` each; only `objects`
//! knows whether an id is live, so `UiTree` checks an id there before it reaches them. `UiTree`
//! also keeps the rules that span parts: a new node gets a place in every part and its properties
//! share its dirty flags; a freed node leaves every part; a logical edit reconnects inheritance; a
//! visual edit marks the parent for layout; a new attached value shares its node's dirty flags and,
//! when its key inherits, joins inheritance. Every link edit goes through two private methods,
//! `attach` and `unlink`, so those rules hold for each public one.
//!
//! The tests in `tests` check `UiTree`'s behavior, one file per concern.

mod attached;
mod dirty;
mod handle;
mod hierarchy;
mod inheritance;
mod objects;

use crate::Object;
use crate::arena::SlotId;
use crate::property::{Affects, AnyProperty, AttachedKey, Inheritance, KeyId, Property};
use attached::AttachedValues;
use dirty::DirtyFlags;
pub use dirty::DirtyHandle;
pub use handle::Handle;
use hierarchy::Tree;
use objects::Objects;
use std::any::Any;
use std::collections::HashSet;
use std::fmt::Debug;

const TREES: [TreeKind; 2] = [TreeKind::Logical, TreeKind::Visual];

/// The UI tree: owns every node, links them into the logical and visual trees, and tracks what each
/// node needs redone.
///
/// Every node is stored as a `Box<dyn Object>` in a slot and referenced by a [`NodeId`] handle instead of a
/// pointer or an `Rc<RefCell<_>>`. A handle stays valid until its node is [freed](UiTree::free); after that,
/// every lookup through it returns `None`, even once the slot has been reused for another node.
///
/// Freed slots are kept in a free list and reused by later allocations, so the backing storage only grows
/// when there is no free slot left.
///
/// # Examples
///
/// ```
/// use artic_core::Object;
/// use artic_core::property::AnyProperty;
/// use artic_core::tree::UiTree;
///
/// #[derive(Debug)]
/// struct Label(&'static str);
/// impl Object for Label {
///   fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
/// }
///
/// let mut tree = UiTree::new();
/// let id = tree.alloc(Box::new(Label("hello")));
///
/// assert_eq!(tree.get_as::<Label>(id).map(|label| label.0), Some("hello"));
///
/// tree.free(id);
/// assert!(tree.get(id).is_none());
/// ```
#[derive(Debug, Default)]
pub struct UiTree {
  objects: Objects,
  logical: Tree,
  visual: Tree,
  dirty_flags: DirtyFlags,
  attached_values: AttachedValues,
}

/// A handle to a node stored in a [`UiTree`].
///
/// A `NodeId` pairs a slot index with the generation that slot had when the node was allocated. Lookups
/// compare both, so an id whose node was freed never resolves to whatever node later takes its slot.
///
/// Ids are cheap to copy and compare, and `Option<NodeId>` is the same size as `NodeId`. An id is only
/// meaningful for the tree that returned it.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct NodeId(SlotId);

/// Which of a node's two trees an operation works on.
///
/// Every node in a [`UiTree`] belongs to two trees at once, with independent parent and child
/// links. The *logical* tree is the structure the view declares; names, resources and inherited
/// values follow it. The *visual* tree is what gets measured and drawn, including the nodes a
/// control's template adds around its content.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum TreeKind {
  /// The tree the view declares.
  Logical,
  /// The tree that is laid out and drawn.
  Visual,
}

/// Why a link between two nodes can't be made, or a node isn't where it was expected.
#[derive(Debug, Clone, Copy, Eq, PartialEq, thiserror::Error)]
pub enum LinkError {
  /// A node was freed, or never belonged to this tree.
  #[error("node {0:?} is not in the arena")]
  NotInTree(NodeId),
  /// An insertion index past the end of the parent's children.
  #[error("child index {index} is out of range for {len} children")]
  IndexOutOfRange {
    /// The index asked for.
    index: usize,
    /// How many children the parent has, not counting the node being moved.
    len: usize,
  },
  /// The parent is the child or one of its descendants.
  #[error("adding node {child:?} under {parent:?} would create a cycle in the {tree:?} tree")]
  Cycle {
    /// The tree the link is in.
    tree: TreeKind,
    /// The would-be parent.
    parent: NodeId,
    /// The would-be child.
    child: NodeId,
  },
  /// A node's logical and visual children differ, where they should be the same, in order.
  #[error("the logical and visual children of node {parent:?} differ")]
  ChildrenDiffer {
    /// The node.
    parent: NodeId,
  },
  /// A node expected under `parent` isn't its child.
  #[error("node {child:?} is not a child of {parent:?} in the {tree:?} tree")]
  NotAChild {
    /// The tree that was checked.
    tree: TreeKind,
    /// The expected parent.
    parent: NodeId,
    /// The node.
    child: NodeId,
  },
}

impl UiTree {
  /// Creates an empty tree.
  ///
  /// It doesn't allocate until the first node is added.
  #[must_use]
  pub const fn new() -> Self {
    Self {
      objects: Objects::new(),
      logical: Tree::new(),
      visual: Tree::new(),
      dirty_flags: DirtyFlags::new(),
      attached_values: AttachedValues::new(),
    }
  }

  /// Moves `value` into the tree and returns the id that refers to it.
  ///
  /// The most recently freed slot is reused if there is one; otherwise a new slot is appended. The new
  /// node has no parent or children in either tree and starts with layout and rendering dirty. Each
  /// property it lists through [`Object::visit_properties`] is connected to its dirty flags, so a later
  /// change to that property marks the node dirty.
  ///
  /// # Panics
  ///
  /// Panics if the tree would need more than `u32::MAX` slots.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::{Object};
  /// use artic_core::property::AnyProperty;
  /// use artic_core::tree::UiTree;
  ///
  /// #[derive(Debug)]
  /// struct Label;
  /// impl Object for Label {
  ///   fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  /// }
  ///
  /// let mut tree = UiTree::new();
  /// let id = tree.alloc(Box::new(Label));
  ///
  /// assert!(tree.get(id).is_some());
  /// assert_eq!(tree.len(), 1);
  /// ```
  #[must_use = "node ID needed to get or free the node from the tree"]
  pub fn alloc(&mut self, value: Box<dyn Object>) -> NodeId {
    let id = self.objects.alloc(value);
    self.logical.insert(id);
    self.visual.insert(id);
    let dirty = self.dirty_flags.insert(id);
    self
      .objects
      .object(id)
      .visit_properties(&mut |property| property.attach(dirty.clone()));
    id
  }

  /// Adds `node` to the tree, as [`alloc`](UiTree::alloc) does, and returns a handle that knows
  /// its type.
  #[must_use = "the handle is needed to get or free the node"]
  pub fn alloc_typed<T: Object>(&mut self, node: T) -> Handle<T> {
    Handle::new(self.alloc(Box::new(node)))
  }

  /// Removes one node from the arena and cuts its properties loose. Its links are left to
  /// [`free`](UiTree::free), which only calls this once no surviving node names it.
  fn free_node(&mut self, node_id: NodeId) {
    let Some(object) = self.objects.free(node_id) else {
      return;
    };
    self.dirty_flags.remove(node_id);
    object.visit_properties(&mut |property| {
      property.inherit_from(None);
      property.dispose();
    });
    self.logical.remove(node_id);
    self.visual.remove(node_id);
    self.attached_values.of(node_id).for_each(|property| {
      property.inherit_from(None);
      property.dispose();
    });
    self.attached_values.remove(node_id);
  }

  /// Returns a reference to the node referred to by `id`, or `None` if `id` is stale.
  ///
  /// Use [`get_as`](UiTree::get_as) to get the node as its concrete type.
  #[must_use]
  pub fn get(&self, id: NodeId) -> Option<&dyn Object> {
    self.objects.get(id)
  }

  /// Returns a reference to the node referred to by `id` as a `T`.
  ///
  /// Returns `None` if `id` is stale or if the node isn't a `T`.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::{Object};
  /// use artic_core::property::AnyProperty;
  /// use artic_core::tree::UiTree;
  ///
  /// #[derive(Debug)]
  /// struct Label(&'static str);
  /// impl Object for Label {
  ///   fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  /// }
  ///
  /// #[derive(Debug)]
  /// struct Button;
  /// impl Object for Button {
  ///   fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  /// }
  ///
  /// let mut tree = UiTree::new();
  /// let id = tree.alloc(Box::new(Label("hello")));
  ///
  /// assert_eq!(tree.get_as::<Label>(id).map(|label| label.0), Some("hello"));
  /// assert!(tree.get_as::<Button>(id).is_none()); // wrong type
  /// ```
  #[must_use]
  pub fn get_as<T>(&self, id: NodeId) -> Option<&T>
  where
    T: Object,
  {
    let node: &dyn Any = self.get(id)?;
    node.downcast_ref()
  }

  /// Returns a mutable reference to the node referred to by `id`, or `None` if `id` is stale.
  ///
  /// Use [`get_as_mut`](UiTree::get_as_mut) to get the node as its concrete type.
  #[must_use]
  pub fn get_mut(&mut self, id: NodeId) -> Option<&mut dyn Object> {
    self.objects.get_mut(id)
  }

  /// Returns a mutable reference to the node referred to by `id` as a `T`.
  ///
  /// Returns `None` if `id` is stale or if the node isn't a `T`.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::{Object};
  /// use artic_core::property::AnyProperty;
  /// use artic_core::tree::UiTree;
  ///
  /// #[derive(Debug)]
  /// struct Counter(u32);
  /// impl Object for Counter {
  ///   fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  /// }
  ///
  /// let mut tree = UiTree::new();
  /// let id = tree.alloc(Box::new(Counter(0)));
  ///
  /// if let Some(counter) = tree.get_as_mut::<Counter>(id) {
  ///   counter.0 += 1;
  /// }
  /// assert_eq!(tree.get_as::<Counter>(id).map(|counter| counter.0), Some(1));
  /// ```
  #[must_use]
  pub fn get_as_mut<T>(&mut self, id: NodeId) -> Option<&mut T>
  where
    T: Object,
  {
    let node: &mut dyn Any = self.get_mut(id)?;
    node.downcast_mut()
  }

  /// Returns the number of live nodes in the tree.
  ///
  /// Freed slots waiting to be reused aren't counted.
  #[must_use]
  pub fn len(&self) -> usize {
    self.objects.len()
  }

  /// Returns `true` if the tree holds no live nodes.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.objects.is_empty()
  }

  /// Returns `true` if `id` refers to a live node.
  ///
  /// This is `false` once the node has been freed, even if its slot has since been reused by another node.
  /// The answer is only meaningful for ids returned by this tree: an id from another tree can match a live
  /// node here by chance.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::{Object};
  /// use artic_core::property::AnyProperty;
  /// use artic_core::tree::UiTree;
  ///
  /// #[derive(Debug)]
  /// struct Label;
  /// impl Object for Label {
  ///   fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  /// }
  ///
  /// let mut tree = UiTree::new();
  /// let id = tree.alloc(Box::new(Label));
  /// assert!(tree.contains(id));
  ///
  /// tree.free(id);
  /// let reused = tree.alloc(Box::new(Label)); // takes the freed slot
  /// assert!(!tree.contains(id));
  /// assert!(tree.contains(reused));
  /// ```
  #[must_use]
  pub fn contains(&self, id: NodeId) -> bool {
    self.objects.contains(id)
  }

  /// Returns the parent of `id` in `tree`, or `None` if it has none or `id` is stale.
  #[must_use]
  pub fn parent(&self, tree: TreeKind, id: NodeId) -> Option<NodeId> {
    self.tree(tree).parent(id)
  }

  /// Makes `child` the last child of `parent` in `tree`.
  ///
  /// If `child` already has a parent in `tree`, it leaves that parent first, so a node is never in
  /// two child lists of the same tree. Its links in the other tree don't change.
  ///
  /// # Panics
  ///
  /// Panics if `parent` or `child` is stale, or if `child` is `parent` or one of its ancestors in
  /// `tree`, because the tree would then contain a cycle.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::Object;
  /// use artic_core::property::AnyProperty;
  /// use artic_core::tree::{TreeKind, UiTree};
  ///
  /// #[derive(Debug)]
  /// struct Stack;
  /// impl Object for Stack {
  ///   fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  /// }
  ///
  /// let mut tree = UiTree::new();
  /// let panel = tree.alloc(Box::new(Stack));
  /// let first = tree.alloc(Box::new(Stack));
  /// let second = tree.alloc(Box::new(Stack));
  ///
  /// tree.add_child(TreeKind::Logical, panel, first);
  /// tree.add_child(TreeKind::Logical, panel, second);
  /// assert_eq!(tree.children(TreeKind::Logical, panel).collect::<Vec<_>>(), [first, second]);
  /// assert_eq!(tree.parent(TreeKind::Logical, first), Some(panel));
  /// assert_eq!(tree.parent(TreeKind::Visual, first), None);
  /// ```
  #[track_caller]
  pub fn add_child(&mut self, tree: TreeKind, parent: NodeId, child: NodeId) {
    self.check_attach(tree, parent, child);
    self.unlink(tree, child);
    let index = self.tree(tree).children(parent).len();
    self.attach(tree, parent, index, child);
  }

  /// Makes `child` the child of `parent` at `index` in `tree`, shifting later children right.
  ///
  /// Like [`add_child`](UiTree::add_child), `child` first leaves its current parent in `tree`.
  /// `index` counts the children after that, so moving a node within one parent's list works the
  /// same as inserting a new one.
  ///
  /// # Panics
  ///
  /// Panics in the same cases as [`add_child`](UiTree::add_child), and if `index` is greater than
  /// the number of children.
  #[track_caller]
  pub fn insert_child(&mut self, tree: TreeKind, parent: NodeId, index: usize, child: NodeId) {
    if let Err(error) = self.check_link(tree, parent, Some(index), child) {
      panic!("{error}");
    }
    self.unlink(tree, child);
    self.attach(tree, parent, index, child);
  }

  /// Whether linking `child` under `parent` in `tree` would succeed, at `index` or at the end: both
  /// nodes in the tree, `index` in range once `child` has left its current parent, and no cycle.
  /// Changes nothing.
  pub fn check_link(&self, tree: TreeKind, parent: NodeId, index: Option<usize>, child: NodeId) -> Result<(), LinkError> {
    for node in [parent, child] {
      if !self.contains(node) {
        return Err(LinkError::NotInTree(node));
      }
    }
    if self.tree(tree).would_cycle(parent, child) {
      return Err(LinkError::Cycle { tree, parent, child });
    }
    if let Some(index) = index {
      let mut len = self.tree(tree).children(parent).len();
      if self.tree(tree).parent(child) == Some(parent) {
        len -= 1;
      }
      if index > len {
        return Err(LinkError::IndexOutOfRange { index, len });
      }
    }
    Ok(())
  }

  /// Removes `node` from its parent's children in `tree` and returns that parent.
  ///
  /// Returns `None`, and changes nothing, if `node` has no parent in `tree`. The node stays in the
  /// tree with its own children; only the link to its parent is cut. In the logical tree, the
  /// node and its descendants then inherit only from what is left above them.
  ///
  /// # Panics
  ///
  /// Panics if `node` is stale.
  #[track_caller]
  pub fn detach(&mut self, tree: TreeKind, node: NodeId) -> Option<NodeId> {
    let parent = self.unlink(tree, node)?;
    if tree == TreeKind::Logical {
      inheritance::resolve(&self.objects, &self.attached_values, &self.logical, node);
    }
    Some(parent)
  }

  /// Returns the children of `id` in `tree`, in order.
  ///
  /// A stale `id` has no children.
  pub fn children(&self, tree: TreeKind, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    self.tree(tree).children(id).iter().copied()
  }

  /// Returns the ancestors of `id` in `tree`: its parent, then that node's parent, up to the root.
  ///
  /// `id` itself isn't included, and a stale `id` has no ancestors.
  pub fn ancestors(&self, tree: TreeKind, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    self.tree(tree).ancestors(id)
  }

  /// Returns every descendant of `id` in `tree`, depth-first in pre-order: a node, then its first
  /// child's whole subtree, then its next child's.
  ///
  /// `id` itself isn't included, and a stale `id` has no descendants.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::Object;
  /// use artic_core::property::AnyProperty;
  /// use artic_core::tree::{TreeKind, UiTree};
  ///
  /// #[derive(Debug)]
  /// struct Stack;
  /// impl Object for Stack {
  ///   fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  /// }
  ///
  /// let mut tree = UiTree::new();
  /// let [root, a, a1, b] = [(); 4].map(|_| tree.alloc(Box::new(Stack)));
  /// tree.add_child(TreeKind::Visual, root, a);
  /// tree.add_child(TreeKind::Visual, a, a1);
  /// tree.add_child(TreeKind::Visual, root, b);
  ///
  /// assert_eq!(tree.descendants(TreeKind::Visual, root).collect::<Vec<_>>(), [a, a1, b]);
  /// ```
  pub fn descendants(&self, tree: TreeKind, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    self.tree(tree).descendants(id)
  }

  /// Frees `id` and everything below it in either tree.
  ///
  /// The subtree is every node reachable from `id` through child links of either tree, so template
  /// parts that only exist in the visual tree go too. Any freed node whose parent survives is
  /// detached from that parent first, so no live node keeps the id of a freed child, and every child
  /// of a freed node is freed with it.
  ///
  /// Each freed node's properties and attached values are disposed with it: a copy of one kept
  /// outside the tree panics when used.
  ///
  /// Freeing bumps each slot's generation, which invalidates the freed ids and every copy of them.
  /// The generation wraps around after `u32::MAX - 1` reuses of the same slot, so an id kept across
  /// that many reuses could in principle match a newer node.
  ///
  /// # Panics
  ///
  /// Panics if `id` is stale: it was already freed, or never returned by this tree.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::Object;
  /// use artic_core::property::AnyProperty;
  /// use artic_core::tree::{TreeKind, UiTree};
  ///
  /// #[derive(Debug)]
  /// struct Panel;
  /// impl Object for Panel {
  ///   fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  /// }
  ///
  /// let mut tree = UiTree::new();
  /// let [root, panel, label] = [(); 3].map(|_| tree.alloc(Box::new(Panel)));
  /// tree.add_child(TreeKind::Logical, root, panel);
  /// tree.add_child(TreeKind::Logical, panel, label);
  ///
  /// tree.free(panel);
  /// assert!(!tree.contains(label));
  /// assert_eq!(tree.children(TreeKind::Logical, root).count(), 0);
  /// ```
  #[track_caller]
  pub fn free(&mut self, id: NodeId) {
    self.check(id);
    let mut subtree = HashSet::from([id]);
    let mut pending = vec![id];
    while let Some(node) = pending.pop() {
      for tree in TREES {
        for child in self.tree(tree).children(node) {
          if subtree.insert(*child) {
            pending.push(*child);
          }
        }
      }
    }
    for node in &subtree {
      for tree in TREES {
        if self.parent(tree, *node).is_some_and(|parent| !subtree.contains(&parent)) {
          self.unlink(tree, *node);
        }
      }
    }
    for node in subtree {
      self.free_node(node);
    }
  }

  /// Returns the frame stages `id` needs redone, or none if `id` is stale.
  ///
  /// A new node starts with layout and rendering dirty. A change to the effective value of one of
  /// its properties adds the stages its key [`affects`](crate::property::PropertyKey::affects).
  #[must_use]
  pub fn dirty(&self, id: NodeId) -> Affects {
    self.dirty_flags.get(id)
  }

  /// Adds `stages` to the frame stages `id` needs redone.
  ///
  /// # Panics
  ///
  /// Panics if `id` is stale.
  #[track_caller]
  pub fn mark_dirty(&mut self, id: NodeId, stages: Affects) {
    self.check(id);
    self.dirty_flags.mark(id, stages);
  }

  /// Removes `stages` from the frame stages `id` needs redone, once they have been redone.
  ///
  /// # Panics
  ///
  /// Panics if `id` is stale.
  #[track_caller]
  pub fn clear_dirty(&mut self, id: NodeId, stages: Affects) {
    self.check(id);
    self.dirty_flags.clear(id, stages);
  }

  /// Returns whether any node was marked dirty since the last call, and clears that.
  ///
  /// Every path that marks a node dirty sets it: a property change, [`mark_dirty`](Self::mark_dirty),
  /// a new node and a change to a node's visual children. Clearing flags doesn't. The frame
  /// pipeline asks after each change it handles, so it needn't walk the tree to know a frame is
  /// needed.
  #[must_use = "the flag is cleared, so a caller that ignores it loses the request for a frame"]
  pub fn take_pending(&self) -> bool {
    self.dirty_flags.take_pending()
  }

  /// Returns `node`'s value of the attached property `key`: the value it holds, or for an
  /// inheriting key the value of its nearest logical ancestor that holds one, as an attached value
  /// or a field sharing the key, or the key's default. Reading never stores anything on the node.
  ///
  /// # Panics
  ///
  /// Panics if `node` is stale.
  #[track_caller]
  #[must_use]
  pub fn attached<T>(&self, node: NodeId, key: &'static AttachedKey<T>) -> T
  where
    T: PartialEq + Clone + 'static,
  {
    if let Some(property) = self.held(node, key) {
      return property.get();
    }
    let inherited = match key.key().inheritance() {
      Inheritance::None => None,
      inheritance => self
        .logical
        .ancestors(node)
        .find_map(|ancestor| self.held(ancestor, key))
        .map(|source| (inheritance, source.get())),
    };
    match inherited {
      Some((Inheritance::Combine(combine), value)) => combine(&key.default_value(), &value),
      Some((_, value)) => value,
      None => key.default_value(),
    }
  }

  /// Returns `node`'s property for the attached property `key`: a field that shares the key, or
  /// its attached value, created at the key's default the first time. A new property is connected
  /// to the node's dirty flags like any of its fields, and its levels let a style value and a local
  /// value compete as usual. An inheriting property follows its nearest logical ancestor that
  /// holds the key, and the node's descendants that hold it follow the new one.
  ///
  /// # Panics
  ///
  /// Panics if `node` is stale.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::Object;
  /// use artic_core::property::{Affects, AnyProperty, AttachedKey, Inheritance, Precedence};
  /// use artic_core::tree::UiTree;
  ///
  /// static ROW: AttachedKey<usize> = AttachedKey::new("Row", Affects::LAYOUT, Inheritance::None, || 0);
  ///
  /// #[derive(Debug)]
  /// struct Button;
  /// impl Object for Button {
  ///   fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
  /// }
  ///
  /// let mut tree = UiTree::new();
  /// let button = tree.alloc(Box::new(Button));
  /// assert_eq!(tree.attached(button, &ROW), 0);
  ///
  /// tree.attached_property(button, &ROW).set_at(Precedence::StyleSetter, 2);
  /// tree.attached_property(button, &ROW).set(1);
  /// assert_eq!(tree.attached(button, &ROW), 1, "a local value overrides a style value");
  /// ```
  #[track_caller]
  pub fn attached_property<T>(&mut self, node: NodeId, key: &'static AttachedKey<T>) -> Property<T>
  where
    T: PartialEq + 'static,
  {
    if let Some(property) = self.held(node, key) {
      return property;
    }
    let property = Property::new(key.key(), key.default_value());
    property.attach(self.dirty_flags.shared(node));
    self.attached_values.insert(node, Box::new(property));
    if key.key().inherits() {
      inheritance::resolve(&self.objects, &self.attached_values, &self.logical, node);
    }
    property
  }

  /// Panics if `id` is stale: every edit checks its nodes before it reaches their parts.
  #[track_caller]
  fn check(&self, id: NodeId) {
    self.objects.object(id);
  }

  fn tree(&self, tree: TreeKind) -> &Tree {
    match tree {
      TreeKind::Logical => &self.logical,
      TreeKind::Visual => &self.visual,
    }
  }

  fn tree_mut(&mut self, tree: TreeKind) -> &mut Tree {
    match tree {
      TreeKind::Logical => &mut self.logical,
      TreeKind::Visual => &mut self.visual,
    }
  }

  #[track_caller]
  fn check_attach(&self, tree: TreeKind, parent: NodeId, child: NodeId) {
    if let Err(error) = self.check_link(tree, parent, None, child) {
      panic!("{error}");
    }
  }

  fn attach(&mut self, tree: TreeKind, parent: NodeId, index: usize, child: NodeId) {
    self.tree_mut(tree).link(parent, index, child);
    self.children_changed(tree, parent);
    if tree == TreeKind::Logical {
      inheritance::resolve(&self.objects, &self.attached_values, &self.logical, child);
    }
  }

  #[track_caller]
  fn unlink(&mut self, tree: TreeKind, node: NodeId) -> Option<NodeId> {
    let parent = self.tree_mut(tree).unlink(node)?;
    self.children_changed(tree, parent);
    Some(parent)
  }

  /// A node's visual children size and place it, so changing them means laying it out again.
  fn children_changed(&mut self, tree: TreeKind, parent: NodeId) {
    if tree == TreeKind::Visual {
      self.mark_dirty(parent, Affects::LAYOUT);
    }
  }

  fn held<T>(&self, node: NodeId, key: &'static AttachedKey<T>) -> Option<Property<T>>
  where
    T: PartialEq + 'static,
  {
    let id = KeyId::of(key.key());
    let mut field = None;
    self.objects.object(node).visit_properties(&mut |property| {
      if field.is_none() && property.key_id() == id {
        field = property.as_any().downcast_ref::<Property<T>>().copied();
      }
    });
    field.or_else(|| self.stored(node, key))
  }

  /// The property `node` stores for `key`, if one was created.
  #[track_caller]
  fn stored<T>(&self, node: NodeId, key: &'static AttachedKey<T>) -> Option<Property<T>>
  where
    T: PartialEq + 'static,
  {
    self.check(node);
    let property = self.attached_values.find(node, KeyId::of(key.key()))?;
    let property = property.as_any().downcast_ref::<Property<T>>();
    Some(*property.expect("a key's property has the key's value type"))
  }
}

#[cfg(test)]
mod tests;
