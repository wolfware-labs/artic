//! The accessibility pass: one surface's accesskit tree, built from the `UiTree` and sent again only
//! where it changed.

use accesskit::{Affine, Node, TreeId, TreeInfo, TreeUpdate};
use artic_core::property::Affects;
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_element::{AccessCx, ElementNodes};
use artic_layout::geometry::{Rect, Transform};
use artic_render::{Layers, Nodes};
use std::collections::{HashMap, HashSet};

/// What the pass last sent for a node, the layout rect it built it with, and the last update whose
/// walk reached it.
#[derive(Debug)]
struct Described {
  node: Node,
  rect: Option<Rect>,
  seen: u64,
}

/// One surface's accessibility tree: the accesskit nodes it last handed over, so each frame sends only
/// the nodes that changed.
///
/// [`update`](Self::update) walks the surface's visible subtree. It asks a node's element for a new
/// description only when the node may have changed: it is new or newly shown, render-dirty, in the
/// frame's stale set (layout resized it or gave it a new result), moved, or it is the root and the
/// scale changed. Every other node costs a comparison of its child list. A node that leaves the tree,
/// freed, detached or hidden, leaves its parent's child list, which is how accesskit learns of it.
///
/// A new `AccessTree`, one after [`request_full`](Self::request_full), and one given another root
/// send the whole tree next.
#[derive(Debug)]
pub struct AccessTree {
  described: HashMap<NodeId, Described>,
  walk: Vec<(NodeId, Layers)>,
  root: Option<NodeId>,
  scale: f32,
  updates: u64,
  full: bool,
}

impl Default for AccessTree {
  fn default() -> Self {
    Self::new()
  }
}

impl AccessTree {
  /// A tree whose first update is the whole tree.
  #[must_use]
  pub fn new() -> Self {
    Self {
      described: HashMap::new(),
      walk: Vec::new(),
      root: None,
      scale: 1.0,
      updates: 0,
      full: true,
    }
  }

  /// Makes the next update the whole tree, as a screen reader that starts reading needs.
  pub fn request_full(&mut self) {
    self.full = true;
  }

  /// The update for the surface whose root is `root`, laid out and drawn through `nodes`, at `scale`
  /// device pixels per logical pixel; `stale` holds the nodes layout resized or gave a new result in
  /// this frame. `None` when nothing a screen reader sees changed.
  ///
  /// The root is always described, with no children while it isn't visible, since an accesskit tree
  /// must hold its root; below it only visible nodes are, children in visual order. Each node is
  /// placed by its layers' transform, its `RenderTransform` and then its layout offset, and the root's
  /// transform ends with the scale. A full update sets the tree and clears the request; a root other
  /// than the last one is a new tree, so its update is full.
  pub fn update(
    &mut self,
    tree: &UiTree,
    root: NodeId,
    nodes: &ElementNodes<'_>,
    stale: &HashSet<NodeId>,
    scale: f32,
  ) -> Option<TreeUpdate> {
    if self.root != Some(root) {
      self.root = Some(root);
      self.full = true;
    }
    if self.full {
      self.described.clear();
    }
    let rescaled = scale.to_bits() != self.scale.to_bits();
    self.scale = scale;
    self.updates = self.updates.wrapping_add(1);
    let seen = self.updates;
    let mut changed = Vec::new();
    let mut walk = std::mem::take(&mut self.walk);
    walk.push((root, nodes.layers(tree, root)));
    while let Some((node, layers)) = walk.pop() {
      let start = walk.len();
      if node != root || layers.visible {
        walk.extend(
          tree
            .children(TreeKind::Visual, node)
            .map(|child| (child, nodes.layers(tree, child)))
            .filter(|(_, layers)| layers.visible),
        );
      }
      let children = &walk[start..];
      let rect = nodes.layout.rect(node);
      let cached = self.described.get(&node);
      let fresh = cached.is_none_or(|cached| {
        cached.rect != rect || tree.dirty(node).contains(Affects::RENDER) || stale.contains(&node) || (node == root && rescaled)
      });
      let next = if fresh {
        let mut built = describe(tree, node, nodes, rect);
        place(&mut built, layers.transform, (node == root).then_some(scale));
        built.set_children(ids(children));
        Some(built)
      } else {
        cached.filter(|cached| !lists(&cached.node, children)).map(|cached| {
          let mut relisted = cached.node.clone();
          relisted.set_children(ids(children));
          relisted
        })
      };
      match next {
        Some(next) if cached.is_none_or(|cached| cached.node != next) => {
          changed.push((id(node), next.clone()));
          self.described.insert(node, Described { node: next, rect, seen });
        }
        _ => {
          if let Some(described) = self.described.get_mut(&node) {
            described.rect = rect;
            described.seen = seen;
          }
        }
      }
      walk[start..].reverse();
    }
    self.walk = walk;
    self.described.retain(|_, described| described.seen == seen);
    let full = std::mem::take(&mut self.full);
    if !full && changed.is_empty() {
      return None;
    }
    Some(TreeUpdate {
      nodes: changed,
      tree: full.then(|| TreeInfo::new(id(root))),
      tree_id: TreeId::ROOT,
      focus: id(root),
    })
  }
}

/// The accesskit ids of `children`.
fn ids(children: &[(NodeId, Layers)]) -> Vec<accesskit::NodeId> {
  children.iter().map(|(child, _)| id(*child)).collect()
}

/// Whether `node` lists exactly `children`, in order.
fn lists(node: &Node, children: &[(NodeId, Layers)]) -> bool {
  node
    .children()
    .iter()
    .copied()
    .eq(children.iter().map(|(child, _)| id(*child)))
}

/// The accesskit id of `node`.
fn id(node: NodeId) -> accesskit::NodeId {
  accesskit::NodeId(node.to_bits())
}

/// `node`'s element's description at `rect`'s size, or a generic container for a node that isn't an
/// element. A node that layout didn't place is described at zero size.
fn describe(tree: &UiTree, node: NodeId, nodes: &ElementNodes<'_>, rect: Option<Rect>) -> Node {
  let cx = AccessCx::new(tree, node, rect.unwrap_or_default());
  match tree.get(node).and_then(|object| nodes.types.element(object)) {
    Some(element) => element.accessibility(&cx),
    None => cx.container(),
  }
}

/// Sets `node`'s transform to `own`, then `scale` when it is the root; none when that is the identity.
fn place(node: &mut Node, own: Option<Transform>, scale: Option<f32>) {
  let own = own.map_or(Affine::IDENTITY, |own| Affine::new(own.coefficients().map(f64::from)));
  let placed = scale.map_or(own, |scale| Affine::scale(f64::from(scale)) * own);
  if placed == Affine::IDENTITY {
    node.clear_transform();
  } else {
    node.set_transform(placed);
  }
}
