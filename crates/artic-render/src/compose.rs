use crate::{Layer, Scene, SceneBuilder, SceneFragment, SceneItem, Shape};
use artic_core::property::Affects;
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_layout::geometry::Transform;
use std::collections::HashMap;
use std::sync::Arc;

/// How composition treats a node: its transform and opacity, which wrap it and its visual
/// descendants, its clip, which wraps only the descendants, whether they're drawn at all, and its
/// drawing order among its siblings.
#[derive(Debug, Clone, PartialEq)]
pub struct Layers {
  /// Maps the node's coordinates to its parent's, if they differ. An element's own layers hold
  /// only its render transform; the frame pipeline adds its layout offset.
  pub transform: Option<Transform>,
  /// Keeps the node's descendants inside this shape, in the node's coordinates, as CSS's
  /// `overflow: hidden` does; the node's own drawing isn't clipped.
  pub clip: Option<Shape>,
  /// Blends the node and its descendants as one group: 0 is clear, 1 opaque. Any value is kept,
  /// and made [drawable](crate::drawable_opacity) when drawn. At `1` or more, or NaN, no layer is pushed.
  pub opacity: f32,
  /// Whether the node and its descendants are drawn at all.
  pub visible: bool,
  /// The node's drawing order among its siblings: lower first, ties in tree order.
  pub z_index: i32,
}

impl Default for Layers {
  fn default() -> Layers {
    Layers {
      transform: None,
      clip: None,
      opacity: 1.0,
      visible: true,
      z_index: 0,
    }
  }
}

/// What composition asks about each node. The frame pipeline answers it from the elements; tests
/// answer it directly.
pub trait Nodes {
  /// The node's layers.
  fn layers(&self, tree: &UiTree, node: NodeId) -> Layers;

  /// Records the node's own drawing, in its own coordinates, without its descendants.
  fn record(&mut self, tree: &UiTree, node: NodeId, builder: &mut SceneBuilder);

  /// Whether the node's fragment is out of date although the node isn't render-dirty, such as after
  /// layout gave it a new size. A stale node is recorded again.
  fn stale(&self, _node: NodeId) -> bool {
    false
  }
}

/// Builds frames from the visual tree, keeping each node's fragment until the node is render-dirty
/// again.
#[derive(Debug, Default)]
pub struct Compositor {
  fragments: HashMap<NodeId, Arc<SceneFragment>>,
  generation: u64,
  recorded: u32,
}

impl Compositor {
  /// Builds the frame for `root`'s visual subtree.
  ///
  /// Each node is wrapped in its transform and opacity layers, each only if set, which also wrap
  /// its visual descendants. It draws before them, and they draw inside its clip layer, when it
  /// has one and at least one of them is drawn, in `z_index` order, ties in tree order. A node
  /// that isn't visible is skipped with its whole subtree, which keeps its fragments and its
  /// render flags, so it comes back without recording what didn't change. A node is recorded again only if it's render-dirty, [stale](Nodes::stale) or has no
  /// fragment yet. Its render flag is cleared before it's recorded, so a write made while recording
  /// keeps it dirty for the next frame. Fragments of nodes no longer in the subtree are dropped.
  pub fn compose(&mut self, tree: &mut UiTree, root: NodeId, nodes: &mut impl Nodes) -> Scene {
    self.recorded = 0;
    let mut previous = std::mem::take(&mut self.fragments);
    let mut walk = Walk {
      compositor: self,
      previous: &mut previous,
      nodes,
      builder: SceneBuilder::default(),
      items: Vec::new(),
    };
    let layers = walk.nodes.layers(tree, root);
    walk.visit(tree, root, layers);
    Scene { items: walk.items }
  }

  /// How many nodes the last [`compose`](Compositor::compose) recorded.
  #[must_use]
  pub fn recorded(&self) -> u32 {
    self.recorded
  }
}

/// One composition in progress.
struct Walk<'a, N> {
  compositor: &'a mut Compositor,
  previous: &'a mut HashMap<NodeId, Arc<SceneFragment>>,
  nodes: &'a mut N,
  builder: SceneBuilder,
  items: Vec<SceneItem>,
}

impl<N: Nodes> Walk<'_, N> {
  fn visit(&mut self, tree: &mut UiTree, node: NodeId, layers: Layers) {
    if !layers.visible {
      self.keep_hidden(tree, node);
      return;
    }
    let set = [
      layers.transform.map(Layer::Transform),
      (layers.opacity < 1.0).then_some(Layer::Opacity(layers.opacity)),
    ];
    let mut pushed = 0;
    for layer in set.into_iter().flatten() {
      self.items.push(SceneItem::Push(layer));
      pushed += 1;
    }

    let fragment = match self.previous.remove(&node) {
      Some(fragment) if !tree.dirty(node).contains(Affects::RENDER) && !self.nodes.stale(node) => fragment,
      _ => {
        tree.clear_dirty(node, Affects::RENDER);
        self.record(tree, node)
      }
    };
    self.compositor.fragments.insert(node, Arc::clone(&fragment));
    self.items.push(SceneItem::Fragment(fragment));

    let mut children: Vec<(NodeId, Layers)> = tree
      .children(TreeKind::Visual, node)
      .map(|child| (child, self.nodes.layers(tree, child)))
      .collect();
    if let Some(clip) = layers.clip
      && children.iter().any(|(_, child)| child.visible)
    {
      self.items.push(SceneItem::Push(Layer::Clip(clip)));
      pushed += 1;
    }
    children.sort_by_key(|(_, layers)| layers.z_index);
    for (child, layers) in children {
      self.visit(tree, child, layers);
    }
    self.items.extend((0..pushed).map(|_| SceneItem::Pop));
  }

  fn keep_hidden(&mut self, tree: &UiTree, node: NodeId) {
    for hidden in std::iter::once(node).chain(tree.descendants(TreeKind::Visual, node)) {
      if let Some(fragment) = self.previous.remove(&hidden) {
        self.compositor.fragments.insert(hidden, fragment);
      }
    }
  }

  fn record(&mut self, tree: &UiTree, node: NodeId) -> Arc<SceneFragment> {
    self.nodes.record(tree, node, &mut self.builder);
    self.compositor.generation += 1;
    self.compositor.recorded += 1;
    Arc::new(SceneFragment::new(node, self.compositor.generation, self.builder.take()))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{Color, Command};
  use artic_core::Object;
  use artic_core::property::{Inheritance, Property, PropertyKey, PropertyVisitor};
  use artic_layout::geometry::Rect;
  use pretty_assertions::assert_eq;
  use std::collections::HashSet;

  #[derive(Debug)]
  struct Node;

  impl Object for Node {
    fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
  }

  /// Answers composition from a table, and counts how often each node is recorded.
  #[derive(Default)]
  struct Table {
    layers: HashMap<NodeId, Layers>,
    recorded: HashMap<NodeId, usize>,
    stale: HashSet<NodeId>,
    writes: HashMap<NodeId, Property<i32>>,
  }

  impl Nodes for Table {
    fn layers(&self, _arena: &UiTree, node: NodeId) -> Layers {
      self.layers.get(&node).cloned().unwrap_or_default()
    }

    fn record(&mut self, _arena: &UiTree, node: NodeId, builder: &mut SceneBuilder) {
      *self.recorded.entry(node).or_default() += 1;
      if let Some(property) = self.writes.get(&node) {
        property.set(property.get() + 1);
      }
      builder.fill(Rect::new(0.0, 0.0, 1.0, 1.0), Color::rgb(0, 0, 0));
    }

    fn stale(&self, node: NodeId) -> bool {
      self.stale.contains(&node)
    }
  }

  /// A visual tree `root -> [a -> [c], b]`, returned as `[root, a, b, c]`.
  fn sample(tree: &mut UiTree) -> [NodeId; 4] {
    let [root, a, b, c] = [(); 4].map(|()| tree.alloc(Box::new(Node)));
    tree.add_child(TreeKind::Visual, root, a);
    tree.add_child(TreeKind::Visual, root, b);
    tree.add_child(TreeKind::Visual, a, c);
    [root, a, b, c]
  }

  /// What `scene` pushes, pops and draws, in order, with fragments named by `names`.
  fn shape(scene: &Scene, names: &HashMap<NodeId, &str>) -> Vec<String> {
    scene
      .items()
      .iter()
      .map(|item| match item {
        SceneItem::Push(layer) => format!("push {layer:?}"),
        SceneItem::Pop => "pop".to_owned(),
        SceneItem::Fragment(fragment) => format!("fragment {}", names[&fragment.node()]),
      })
      .collect()
  }

  /// The nodes whose fragments `scene` draws, in order.
  fn fragment_nodes(scene: &Scene) -> Vec<NodeId> {
    scene
      .items()
      .iter()
      .filter_map(|item| match item {
        SceneItem::Fragment(fragment) => Some(fragment.node()),
        _ => None,
      })
      .collect()
  }

  // A node that isn't visible is skipped together with its whole subtree, visible descendants
  // included.
  #[test]
  fn hidden_node_skips_its_subtree() {
    let mut tree = UiTree::new();
    let [root, a, b, _c] = sample(&mut tree);
    let mut nodes = Table::default();
    nodes.layers.insert(
      a,
      Layers {
        visible: false,
        ..Layers::default()
      },
    );

    let scene = Compositor::default().compose(&mut tree, root, &mut nodes);

    assert_eq!(fragment_nodes(&scene), [root, b]);
  }

  // A subtree hidden for a frame comes back whole once it's visible again, from the fragments it
  // kept rather than recorded again.
  #[test]
  fn subtree_returns_when_visible_again() {
    let mut tree = UiTree::new();
    let [root, a, b, c] = sample(&mut tree);
    let mut compositor = Compositor::default();
    let mut nodes = Table::default();
    nodes.layers.insert(
      a,
      Layers {
        visible: false,
        ..Layers::default()
      },
    );
    compositor.compose(&mut tree, root, &mut nodes);

    nodes.layers.remove(&a);
    let scene = compositor.compose(&mut tree, root, &mut nodes);

    assert_eq!(fragment_nodes(&scene), [root, a, c, b]);
    assert_eq!((nodes.recorded[&a], nodes.recorded[&c]), (1, 1));
  }

  // A node changed while hidden is recorded again when it shows, since its flag stayed set.
  #[test]
  fn node_changed_while_hidden_is_recorded_on_show() {
    let mut tree = UiTree::new();
    let [root, a, _b, c] = sample(&mut tree);
    let mut compositor = Compositor::default();
    let mut nodes = Table::default();
    compositor.compose(&mut tree, root, &mut nodes);
    nodes.layers.insert(
      a,
      Layers {
        visible: false,
        ..Layers::default()
      },
    );
    compositor.compose(&mut tree, root, &mut nodes);

    tree.mark_dirty(c, Affects::RENDER);
    nodes.layers.remove(&a);
    compositor.compose(&mut tree, root, &mut nodes);

    assert_eq!((nodes.recorded[&a], nodes.recorded[&c]), (1, 2));
  }

  // A hidden node's own layers aren't pushed: the scene has nothing of it.
  #[test]
  fn hidden_node_pushes_no_layers() {
    let mut tree = UiTree::new();
    let root = tree.alloc(Box::new(Node));
    let mut nodes = Table::default();
    nodes.layers.insert(
      root,
      Layers {
        visible: false,
        opacity: 0.5,
        transform: Some(Transform::translate(1.0, 0.0)),
        ..Layers::default()
      },
    );

    let scene = Compositor::default().compose(&mut tree, root, &mut nodes);

    assert!(scene.items().is_empty());
  }

  // A node's opacity is pushed as given when it's below 1, for the renderer to make drawable; at 1
  // or more, or NaN, which all draw opaque, no layer is pushed.
  #[test]
  fn opacity_is_pushed_as_given() {
    let pushed = |opacity: f32| {
      let mut tree = UiTree::new();
      let root = tree.alloc(Box::new(Node));
      let mut nodes = Table::default();
      nodes.layers.insert(
        root,
        Layers {
          opacity,
          ..Layers::default()
        },
      );
      let scene = Compositor::default().compose(&mut tree, root, &mut nodes);
      scene.items().iter().find_map(|item| match item {
        SceneItem::Push(Layer::Opacity(opacity)) => Some(*opacity),
        _ => None,
      })
    };

    assert_eq!(pushed(-0.5), Some(-0.5));
    assert_eq!(pushed(0.25), Some(0.25));
    assert_eq!([2.0, 1.0, f32::NAN].map(pushed), [None, None, None]);
  }

  // Siblings draw by ZIndex, lowest first; equal values keep tree order.
  #[test]
  fn children_draw_by_z_index() {
    let mut tree = UiTree::new();
    let root = tree.alloc(Box::new(Node));
    let [first, second, third, fourth] = [(); 4].map(|()| tree.alloc(Box::new(Node)));
    for child in [first, second, third, fourth] {
      tree.add_child(TreeKind::Visual, root, child);
    }
    let mut nodes = Table::default();
    nodes.layers.insert(
      first,
      Layers {
        z_index: 1,
        ..Layers::default()
      },
    );
    nodes.layers.insert(
      third,
      Layers {
        z_index: -1,
        ..Layers::default()
      },
    );
    nodes.layers.insert(
      fourth,
      Layers {
        z_index: 1,
        ..Layers::default()
      },
    );

    let scene = Compositor::default().compose(&mut tree, root, &mut nodes);

    assert_eq!(fragment_nodes(&scene), [root, third, second, first, fourth]);
  }

  // Fragments follow the visual tree depth first, each node before its descendants.
  #[test]
  fn scene_composes_fragments_in_visual_order() {
    let mut tree = UiTree::new();
    let [root, a, b, c] = sample(&mut tree);

    let scene = Compositor::default().compose(&mut tree, root, &mut Table::default());

    assert_eq!(fragment_nodes(&scene), [root, a, c, b]);
  }

  // A node that isn't render-dirty keeps its fragment, shared rather than recorded again; a
  // render-dirty one is recorded again with a higher generation, and its flag is cleared.
  #[test]
  fn clean_fragment_is_reused() {
    let mut tree = UiTree::new();
    let [root, a, b, c] = sample(&mut tree);
    let mut compositor = Compositor::default();
    let mut nodes = Table::default();
    let first = compositor.compose(&mut tree, root, &mut nodes);

    tree.mark_dirty(b, Affects::RENDER);
    let second = compositor.compose(&mut tree, root, &mut nodes);

    let recorded: Vec<usize> = [root, a, b, c].iter().map(|node| nodes.recorded[node]).collect();
    assert_eq!(recorded, [1, 1, 2, 1]);
    let fragment = |scene: &Scene, index: usize| match &scene.items()[index] {
      SceneItem::Fragment(fragment) => Arc::clone(fragment),
      other => panic!("expected a fragment, found {other:?}"),
    };
    assert!(Arc::ptr_eq(&fragment(&first, 0), &fragment(&second, 0)), "root is shared");
    assert!(fragment(&second, 3).generation() > fragment(&first, 3).generation());
    assert!(!tree.dirty(b).contains(Affects::RENDER));
  }

  // A node's clip fences its children, not its own fragment; its transform and opacity cover both.
  #[test]
  fn clip_applies_to_children_not_the_node() {
    let mut tree = UiTree::new();
    let [root, a, b, c] = sample(&mut tree);
    let names = HashMap::from([(root, "root"), (a, "a"), (b, "b"), (c, "c")]);
    let transform = Transform::translate(5.0, 0.0);
    let clip = Shape::Rect(Rect::new(0.0, 0.0, 4.0, 4.0));
    let mut nodes = Table::default();
    let layers = Layers {
      transform: Some(transform),
      clip: Some(clip.clone()),
      opacity: 0.5,
      ..Layers::default()
    };
    nodes.layers.insert(a, layers);

    let scene = Compositor::default().compose(&mut tree, root, &mut nodes);

    let push = |layer: Layer| format!("push {layer:?}");
    assert_eq!(
      shape(&scene, &names),
      [
        "fragment root".to_owned(),
        push(Layer::Transform(transform)),
        push(Layer::Opacity(0.5)),
        "fragment a".to_owned(),
        push(Layer::Clip(clip)),
        "fragment c".to_owned(),
        "pop".to_owned(),
        "pop".to_owned(),
        "pop".to_owned(),
        "fragment b".to_owned(),
      ]
    );
  }

  // A clip on a node without children pushes nothing.
  #[test]
  fn no_clip_without_children() {
    let mut tree = UiTree::new();
    let [root, a, b, c] = sample(&mut tree);
    let names = HashMap::from([(root, "root"), (a, "a"), (b, "b"), (c, "c")]);
    let mut nodes = Table::default();
    let clip = Some(Shape::Rect(Rect::new(0.0, 0.0, 4.0, 4.0)));
    nodes.layers.insert(
      b,
      Layers {
        clip,
        ..Layers::default()
      },
    );

    let scene = Compositor::default().compose(&mut tree, root, &mut nodes);

    assert_eq!(
      shape(&scene, &names),
      ["fragment root", "fragment a", "fragment c", "fragment b"]
    );
  }

  // A clip whose children are all hidden pushes nothing either.
  #[test]
  fn no_clip_when_no_child_is_drawn() {
    let mut tree = UiTree::new();
    let [root, a, b, c] = sample(&mut tree);
    let names = HashMap::from([(root, "root"), (a, "a"), (b, "b"), (c, "c")]);
    let mut nodes = Table::default();
    let clip = Some(Shape::Rect(Rect::new(0.0, 0.0, 4.0, 4.0)));
    nodes.layers.insert(
      a,
      Layers {
        clip,
        ..Layers::default()
      },
    );
    nodes.layers.insert(
      c,
      Layers {
        visible: false,
        ..Layers::default()
      },
    );

    let scene = Compositor::default().compose(&mut tree, root, &mut nodes);

    assert_eq!(shape(&scene, &names), ["fragment root", "fragment a", "fragment b"]);
  }

  // A node that leaves the subtree loses its fragment, so freed nodes don't leak.
  #[test]
  fn removed_nodes_lose_their_fragments() {
    let mut tree = UiTree::new();
    let [root, a, _, _] = sample(&mut tree);
    let mut compositor = Compositor::default();
    compositor.compose(&mut tree, root, &mut Table::default());

    tree.detach(TreeKind::Visual, a);
    compositor.compose(&mut tree, root, &mut Table::default());

    assert_eq!(compositor.fragments.len(), 2, "root and b");
  }

  // A recorded fragment holds what the node drew.
  #[test]
  fn fragments_hold_the_recording() {
    let mut tree = UiTree::new();
    let root = tree.alloc(Box::new(Node));

    let scene = Compositor::default().compose(&mut tree, root, &mut Table::default());

    let SceneItem::Fragment(fragment) = &scene.items()[0] else {
      panic!("a fragment");
    };
    assert_eq!(
      fragment.commands(),
      [Command::Fill {
        shape: Rect::new(0.0, 0.0, 1.0, 1.0).into(),
        brush: Color::rgb(0, 0, 0).into()
      }]
    );
  }

  static PAINT: PropertyKey<i32> = PropertyKey::new("Paint", Affects::RENDER, Inheritance::None);

  #[derive(Debug)]
  struct Painted(Property<i32>);

  impl Object for Painted {
    fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
      visitor.visit(&self.0);
    }
  }

  // The compositor counts the nodes the last composition recorded: all of them at first, then only
  // the render-dirty ones.
  #[test]
  fn compose_reports_recorded() {
    let mut tree = UiTree::new();
    let [root, _, b, _] = sample(&mut tree);
    let mut compositor = Compositor::default();
    let mut nodes = Table::default();

    compositor.compose(&mut tree, root, &mut nodes);
    let first = compositor.recorded();
    tree.mark_dirty(b, Affects::RENDER);
    compositor.compose(&mut tree, root, &mut nodes);
    let second = compositor.recorded();
    compositor.compose(&mut tree, root, &mut nodes);

    assert_eq!((first, second, compositor.recorded()), (4, 1, 0));
  }

  // A stale node is recorded again although it isn't render-dirty.
  #[test]
  fn stale_node_is_recorded_again() {
    let mut tree = UiTree::new();
    let [root, a, b, c] = sample(&mut tree);
    let mut compositor = Compositor::default();
    let mut nodes = Table::default();
    compositor.compose(&mut tree, root, &mut nodes);

    nodes.stale.insert(a);
    compositor.compose(&mut tree, root, &mut nodes);

    assert_eq!([a, b, c].map(|node| nodes.recorded[&node]), [2, 1, 1]);
  }

  // A write made while a node is recorded keeps it render-dirty for the next frame.
  #[test]
  fn write_during_record_stays_dirty() {
    let mut tree = UiTree::new();
    let paint = Property::new(&PAINT, 0);
    let root = tree.alloc(Box::new(Painted(paint)));
    let mut nodes = Table::default();
    nodes.writes.insert(root, paint);

    Compositor::default().compose(&mut tree, root, &mut nodes);

    assert!(tree.dirty(root).contains(Affects::RENDER));
  }
}
