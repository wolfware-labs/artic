#[cfg(test)]
mod tests;

use crate::finite;
use crate::geometry::{Rect, Size};
use crate::text::TextEngine;
use artic_core::property::Affects;
use artic_core::tree::{NodeId, TreeKind, UiTree};
use std::any::Any;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Something layout can size and place: an element, seen by the layout pass.
///
/// Both methods take `&self`: an element's inputs are its properties, readable through a shared
/// reference, and what layout works out (sizes, rectangles, caches, and the results a node sets
/// with [`MeasureCx::set_measured`] and [`ArrangeCx::set_arranged`]) is kept by the
/// [`LayoutManager`], not by the element.
pub trait Layoutable {
  /// Returns the size the element wants within `available`, its margin included.
  ///
  /// An axis of `available` may be infinite: the parent offers as much as the element wants. The
  /// returned size must be finite. Children are measured through [`MeasureCx::measure`]; a child
  /// it doesn't measure is forgotten, with its subtree, until a later measure measures it.
  fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size;

  /// Places the element in `rect`, in its parent's coordinates, and its children through
  /// [`ArrangeCx::arrange`], in the element's own coordinates: `(0, 0)` is its top-left corner.
  fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect);

  /// Whether the node takes no part in layout: its parent doesn't see it among its children, and
  /// it isn't measured or arranged, with its subtree. By default, false. A layoutable whose answer
  /// changes marks itself with [`LayoutManager::invalidate_measure`], as `#[element]` does through
  /// `Visibility`, which affects layout.
  fn collapsed(&self) -> bool {
    false
  }
}

/// Finds the [`Layoutable`] of a node, for layout to measure and arrange it.
///
/// The frame pipeline answers it from the elements; tests answer it directly.
pub trait LayoutNodes {
  /// The node as layout sees it, or `None` for a node with nothing to lay out, which takes no
  /// space.
  fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable>;
}

/// What layout knows about one node.
#[derive(Debug, Clone, Copy, Default)]
struct Slot {
  /// The constraint of the last measure.
  available: Option<Size>,
  /// What the last measure returned.
  desired: Size,
  /// The rectangle the parent last offered, in its coordinates.
  offered: Option<Rect>,
  /// The node's bounds in its parent's coordinates: the offered rectangle, unless the node set its
  /// own with [`ArrangeCx::set_bounds`].
  rect: Option<Rect>,
  /// Whether the node must be arranged again even when offered the same rectangle.
  arrange_dirty: bool,
}

/// Measures and arranges a visual tree, keeping each node's results to redo only what changed.
///
/// A node is measured again when it's marked [`Affects::LAYOUT`] (a layout property changed, or its
/// visual children did) or when its parent offers it a different size. When a measured node's
/// size doesn't change, its ancestors keep theirs and only the node is arranged again.
///
/// A new [`PixelGrid`] arranges every node again, and measures only what the new logical size
/// reaches: a desired size doesn't depend on the display.
#[derive(Debug, Default)]
pub struct LayoutManager {
  slots: HashMap<NodeId, Slot>,
  sizes: HashMap<NodeId, Size>,
  results: Results,
  grid: Option<PixelGrid>,
}

/// What nodes' measures and arranges produced beside their sizes, and the nodes whose result was
/// set or dropped since a run last reported them.
#[derive(Debug, Default)]
struct Results {
  /// What each node's last measure set.
  measured: HashMap<NodeId, Box<dyn Any>>,
  /// What each node's arrange set, kept while the node only moves.
  arranged: HashMap<NodeId, Box<dyn Any>>,
  /// The nodes whose result was set or dropped, in the order they first changed.
  changed: Vec<NodeId>,
  /// The nodes in `changed`, to list each once.
  listed: HashSet<NodeId>,
}

impl Results {
  /// Notes that `node`'s result was set or dropped.
  fn changed(&mut self, node: NodeId) {
    if self.listed.insert(node) {
      self.changed.push(node);
    }
  }

  /// Drops `node`'s results, as its slot goes, and notes the node if it had one.
  fn forget(&mut self, node: NodeId) {
    let measured = self.measured.remove(&node).is_some();
    let arranged = self.arranged.remove(&node).is_some();
    if measured || arranged {
      self.changed(node);
    }
  }

  /// Drops the results of every node `keep` turns down.
  fn retain(&mut self, keep: impl Fn(NodeId) -> bool) {
    let gone: Vec<NodeId> = self
      .measured
      .keys()
      .chain(self.arranged.keys())
      .copied()
      .filter(|node| !keep(*node))
      .collect();
    for node in gone {
      self.forget(node);
    }
  }

  /// Hands over the nodes noted since the last call.
  fn take_changed(&mut self) -> Vec<NodeId> {
    self.listed.clear();
    std::mem::take(&mut self.changed)
  }
}

/// The device-pixel grid arranged rectangles land on: device pixels per logical pixel, and whether
/// rectangles snap to it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelGrid {
  /// Device pixels per logical pixel.
  pub scale: f32,
  /// Whether every arranged rectangle has its edges on the grid.
  pub snap: bool,
}

/// What a [`LayoutManager::run`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayoutStats {
  /// The nodes measured, rather than answered from their cache.
  pub measured: u32,
  /// The nodes arranged, rather than skipped as unchanged.
  pub arranged: u32,
  /// The nodes whose arranged size differs from the last time they were arranged, in the order
  /// their arrangement finished. A node arranged for the first time isn't listed, and neither is
  /// one that only moved; one shown again after its parent stopped laying it out is compared with
  /// its size from before.
  pub resized: Vec<NodeId>,
  /// The nodes whose layout result was set or dropped since the last run, including by a
  /// [`measure`](LayoutManager::measure) call, in the order they first changed. A node draws its
  /// result, so these are recorded again.
  pub results: Vec<NodeId>,
}

impl LayoutManager {
  /// Lays out `root`'s visual subtree in `available`, on `grid`, shaping text with `text`.
  ///
  /// The root's rectangle is `available` where it's finite, and its desired size where it isn't.
  /// Results of nodes no longer in the subtree are dropped, and a collapsed root lays out nothing.
  /// Returns what it measured, arranged and resized.
  pub fn run(
    &mut self,
    tree: &mut UiTree,
    nodes: &impl LayoutNodes,
    root: NodeId,
    available: Size,
    grid: PixelGrid,
    text: &mut dyn TextEngine,
  ) -> LayoutStats {
    if self.grid.replace(grid) != Some(grid) {
      for slot in self.slots.values_mut() {
        slot.arrange_dirty = true;
      }
    }
    let (mut cx, depths, desired) = self.measure_pass(tree, nodes, root, available, text);
    if cx.pass.collapsed(root) {
      cx.pass.stats.results = cx.pass.results.take_changed();
      return cx.pass.stats;
    }
    let mut cx = ArrangeCx { pass: cx.pass, grid };
    let width = if available.width.is_finite() {
      available.width
    } else {
      desired.width
    };
    let height = if available.height.is_finite() {
      available.height
    } else {
      desired.height
    };
    cx.arrange(root, Rect::new(0.0, 0.0, width, height));

    // The nodes the upward pass stopped at, still waiting in their unchanged rectangles.
    let mut waiting: Vec<(usize, NodeId)> = cx
      .pass
      .slots
      .iter()
      .filter(|(_, slot)| slot.arrange_dirty)
      .map(|(node, _)| (depths[node], *node))
      .collect();
    waiting.sort_by_key(|(depth, _)| *depth);
    for (_, node) in waiting {
      let slot = cx.pass.slots[&node];
      if slot.arrange_dirty
        && let Some(rect) = slot.offered
      {
        cx.arrange(node, rect);
      }
    }
    cx.pass.stats.results = cx.pass.results.take_changed();
    cx.pass.stats
  }

  /// Measures `root`'s visual subtree in `available`, shaping text with `text`, as a
  /// [`run`](LayoutManager::run) does first, without arranging it, and returns the size it wants. A
  /// later `run` in the same `available` answers every measure from the cache. A collapsed root
  /// measures nothing and returns zero.
  pub fn measure(
    &mut self,
    tree: &mut UiTree,
    nodes: &impl LayoutNodes,
    root: NodeId,
    available: Size,
    text: &mut dyn TextEngine,
  ) -> Size {
    self.measure_pass(tree, nodes, root, available, text).2
  }

  fn measure_pass<'a>(
    &'a mut self,
    tree: &'a mut UiTree,
    nodes: &'a impl LayoutNodes,
    root: NodeId,
    available: Size,
    text: &'a mut dyn TextEngine,
  ) -> (MeasureCx<'a>, HashMap<NodeId, usize>, Size) {
    let mut depths = HashMap::new();
    let mut dirty = HashSet::new();
    let mut pending = vec![(root, 0_usize)];
    while let Some((node, depth)) = pending.pop() {
      depths.insert(node, depth);
      if tree.dirty(node).contains(Affects::LAYOUT) {
        dirty.insert(node);
      }
      pending.extend(tree.children(TreeKind::Visual, node).map(|child| (child, depth + 1)));
    }
    for node in &dirty {
      tree.clear_dirty(*node, Affects::LAYOUT);
    }
    self.slots.retain(|node, _| depths.contains_key(node));
    self.sizes.retain(|node, _| depths.contains_key(node));
    self.results.retain(|node| depths.contains_key(&node));
    if collapsed(nodes, tree, root) {
      self.slots.clear();
      self.results.retain(|_| false);
      dirty.clear();
    }

    let mut nested = HashSet::new();
    let mut between = Vec::new();
    for node in &dirty {
      let mut path = Vec::new();
      for ancestor in tree
        .ancestors(TreeKind::Visual, *node)
        .take_while(|ancestor| depths.contains_key(ancestor))
      {
        if dirty.contains(&ancestor) {
          nested.insert(*node);
          between.append(&mut path);
          break;
        }
        path.push(ancestor);
      }
    }
    let mut by_depth: BTreeMap<usize, Vec<NodeId>> = BTreeMap::new();
    for node in dirty.iter().filter(|node| !nested.contains(*node)) {
      by_depth.entry(depths[node]).or_default().push(*node);
    }
    dirty.extend(between);
    let mut cx = MeasureCx {
      pass: Pass {
        tree,
        nodes,
        slots: &mut self.slots,
        sizes: &mut self.sizes,
        results: &mut self.results,
        text,
        current: None,
        stats: LayoutStats::default(),
      },
      dirty,
      measured: Vec::new(),
    };

    // Upward: a dirty node measures again in the size it had. If its desired size is unchanged,
    // the change stops there and only the node is arranged again; otherwise its parent is next.
    while let Some((_, deepest)) = by_depth.pop_last() {
      for node in deepest {
        if node == root || !cx.dirty.contains(&node) {
          continue;
        }
        let slot = cx.pass.slots.get(&node).copied().unwrap_or_default();
        let changed = if cx.pass.collapsed(node) {
          slot.available.is_some()
        } else {
          match slot.available {
            Some(available) => cx.measure(node, available) != slot.desired,
            None => true,
          }
        };
        if changed
          && let Some(parent) = cx.pass.tree.parent(TreeKind::Visual, node)
          && cx.dirty.insert(parent)
        {
          by_depth.entry(depths[&parent]).or_default().push(parent);
        }
      }
    }

    // Downward: measure from the root; clean nodes answer from their cache.
    let desired = cx.measure(root, available);
    (cx, depths, desired)
  }

  /// Marks `node` to be measured again in the next [`run`](LayoutManager::run).
  pub fn invalidate_measure(tree: &mut UiTree, node: NodeId) {
    tree.mark_dirty(node, Affects::LAYOUT);
  }

  /// Marks `node` to be arranged again in the next [`run`](LayoutManager::run), in the same
  /// rectangle, for a change that moves its children without changing its size.
  pub fn invalidate_arrange(&mut self, node: NodeId) {
    self.slots.entry(node).or_default().arrange_dirty = true;
  }

  /// The size `node` last asked for, if it was measured.
  #[must_use]
  pub fn desired(&self, node: NodeId) -> Option<Size> {
    self
      .slots
      .get(&node)
      .filter(|slot| slot.available.is_some())
      .map(|slot| slot.desired)
  }

  /// What `node`'s layout produced for it to draw: its arranged result if it has one, and its
  /// measured result otherwise.
  #[must_use]
  pub fn result(&self, node: NodeId) -> Option<&dyn Any> {
    self
      .results
      .arranged
      .get(&node)
      .map(|result| &**result)
      .or_else(|| self.results.measured.get(&node).map(|result| &**result))
  }

  /// The bounds `node` was last arranged in, in its parent's coordinates.
  #[must_use]
  pub fn rect(&self, node: NodeId) -> Option<Rect> {
    self.slots.get(&node).and_then(|slot| slot.rect)
  }
}

struct Pass<'a> {
  tree: &'a UiTree,
  nodes: &'a dyn LayoutNodes,
  slots: &'a mut HashMap<NodeId, Slot>,
  sizes: &'a mut HashMap<NodeId, Size>,
  results: &'a mut Results,
  text: &'a mut dyn TextEngine,
  current: Option<NodeId>,
  stats: LayoutStats,
}

impl<'a> Pass<'a> {
  fn node(&self) -> NodeId {
    self.current.expect("called from a node's measure or arrange")
  }

  fn children(&self) -> impl Iterator<Item = NodeId> + use<'a> {
    let (tree, nodes, node) = (self.tree, self.nodes, self.node());
    tree
      .children(TreeKind::Visual, node)
      .filter(move |child| !collapsed(nodes, tree, *child))
  }

  fn collapsed(&self, node: NodeId) -> bool {
    collapsed(self.nodes, self.tree, node)
  }

  fn describe_parent(&self) -> String {
    match self.current {
      Some(parent) => format!("node {parent:?}"),
      None => "the layout root".to_owned(),
    }
  }
}

/// What an element measures its children through, during a [`LayoutManager::run`].
///
/// A desired size is in logical pixels and can't depend on the display, so measuring has no scale
/// and no pixel grid, and doesn't place anything:
///
/// ```compile_fail
/// # use artic_layout::geometry::{Rect, Size};
/// # use artic_layout::{ArrangeCx, Layoutable, MeasureCx};
/// struct Scaled;
///
/// impl Layoutable for Scaled {
///   fn measure(&self, cx: &mut MeasureCx<'_>, _available: Size) -> Size {
///     Size::new(cx.scale(), cx.scale())
///   }
///
///   fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}
/// }
/// ```
///
/// ```compile_fail
/// # use artic_layout::geometry::{Rect, Size};
/// # use artic_layout::{ArrangeCx, Layoutable, MeasureCx};
/// struct Early;
///
/// impl Layoutable for Early {
///   fn measure(&self, cx: &mut MeasureCx<'_>, _available: Size) -> Size {
///     cx.set_bounds(Rect::new(0.0, 0.0, 1.0, 1.0));
///     Size::ZERO
///   }
///
///   fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}
/// }
/// ```
///
/// ```compile_fail
/// # use artic_layout::geometry::{Rect, Size};
/// # use artic_layout::{ArrangeCx, Layoutable, MeasureCx};
/// struct Mixed;
///
/// impl Layoutable for Mixed {
///   fn measure(&self, cx: &mut MeasureCx<'_>, _available: Size) -> Size {
///     cx.set_arranged(1_u32);
///     Size::ZERO
///   }
///
///   fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}
/// }
/// ```
pub struct MeasureCx<'a> {
  pass: Pass<'a>,
  /// The nodes to measure again, whatever their parent offers.
  dirty: HashSet<NodeId>,
  /// The children measured so far by each node being measured, innermost last.
  measured: Vec<HashSet<NodeId>>,
}

impl<'a> MeasureCx<'a> {
  /// The node being measured.
  ///
  /// # Panics
  ///
  /// Panics outside a node's `measure`.
  #[must_use]
  pub fn node(&self) -> NodeId {
    self.pass.node()
  }

  /// The tree being laid out.
  #[must_use]
  pub fn tree(&self) -> &'a UiTree {
    self.pass.tree
  }

  /// The visual children of the node being measured, in order, leaving out collapsed ones.
  pub fn children(&self) -> impl Iterator<Item = NodeId> + use<'a> {
    self.pass.children()
  }

  /// The text engine, for shaping the node's text into the paragraph its size comes from.
  pub fn text(&mut self) -> &mut dyn TextEngine {
    &mut *self.pass.text
  }

  /// Measures `child` in `available` and returns the size it wants, its margin included. A child
  /// that isn't marked and was last measured in the same `available` answers from its cache. A
  /// collapsed child isn't measured: it returns zero and keeps no results.
  ///
  /// # Panics
  ///
  /// In debug builds, panics if `child` isn't a visual child of the node being measured (its
  /// results would be forgotten right after), if `available` has a negative or NaN axis, or if the
  /// child returns an infinite, NaN or negative size; release builds treat such an axis as zero.
  pub fn measure(&mut self, child: NodeId, available: Size) -> Size {
    debug_assert!(
      self
        .pass
        .current
        .is_none_or(|parent| self.pass.tree.parent(TreeKind::Visual, child) == Some(parent)),
      "{} measured node {child:?}, which isn't a visual child of it; measure only visual children",
      self.pass.describe_parent(),
    );
    if self.pass.collapsed(child) {
      return Size::ZERO;
    }
    let available = self.checked_available(child, available);
    if let Some(siblings) = self.measured.last_mut() {
      siblings.insert(child);
    }
    let slot = self.pass.slots.entry(child).or_default();
    if !self.dirty.contains(&child) && slot.available == Some(available) {
      return slot.desired;
    }
    self.pass.stats.measured += 1;
    let had_measured = self.pass.results.measured.remove(&child).is_some();
    let had_arranged = self.pass.results.arranged.remove(&child).is_some();
    let desired = match self.pass.nodes.layoutable(self.pass.tree, child) {
      Some(layoutable) => {
        let parent = self.pass.current.replace(child);
        self.measured.push(HashSet::new());
        let desired = layoutable.measure(self, available);
        let measured = self.measured.pop().expect("pushed above");
        self.pass.current = parent;
        self.forget_unmeasured(child, &measured);
        desired
      }
      None => {
        self.forget_unmeasured(child, &HashSet::new());
        Size::ZERO
      }
    };
    let desired = checked_desired(child, desired, available);
    if had_measured || had_arranged || self.pass.results.measured.contains_key(&child) {
      self.pass.results.changed(child);
    }
    self.dirty.remove(&child);
    let slot = self.pass.slots.entry(child).or_default();
    slot.available = Some(available);
    slot.desired = desired;
    slot.arrange_dirty = true;
    desired
  }

  /// Measures every visual child of the node being measured in `available`, as children sharing
  /// one box, and returns the largest width and the largest height they want.
  pub fn measure_children(&mut self, available: Size) -> Size {
    let children: Vec<NodeId> = self.children().collect();
    children.into_iter().fold(Size::ZERO, |largest, child| {
      let desired = self.measure(child, available);
      Size::new(largest.width.max(desired.width), largest.height.max(desired.height))
    })
  }

  /// Keeps `result` as what this measure produced, beside the size it returns, until the node's
  /// next measure runs; the element reads it back when it draws. A measure answered from the cache
  /// keeps it, and one that runs without calling this drops it.
  ///
  /// # Panics
  ///
  /// Panics outside a node's `measure`.
  pub fn set_measured<T: Any>(&mut self, result: T) {
    let node = self.pass.node();
    self.pass.results.measured.insert(node, Box::new(result));
  }

  /// Forgets what layout knew about `node`'s visual children that its measure didn't measure, and
  /// about their subtrees, so neither the upward nor the waiting pass lays them out.
  fn forget_unmeasured(&mut self, node: NodeId, measured: &HashSet<NodeId>) {
    let skipped: Vec<NodeId> = self
      .pass
      .tree
      .children(TreeKind::Visual, node)
      .filter(|child| !measured.contains(child))
      .collect();
    for child in skipped {
      self.pass.slots.remove(&child);
      self.pass.results.forget(child);
      for descendant in self.pass.tree.descendants(TreeKind::Visual, child) {
        self.pass.slots.remove(&descendant);
        self.pass.results.forget(descendant);
      }
    }
  }

  fn checked_available(&self, child: NodeId, available: Size) -> Size {
    let valid = |axis: f32| axis >= 0.0;
    debug_assert!(
      valid(available.width) && valid(available.height),
      "{} offered node {child:?} an invalid available size ({available}): each axis must be zero or more, or infinite",
      self.pass.describe_parent(),
    );
    Size::new(
      if valid(available.width) { available.width } else { 0.0 },
      if valid(available.height) { available.height } else { 0.0 },
    )
  }
}

impl std::fmt::Debug for MeasureCx<'_> {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("MeasureCx")
      .field("current", &self.pass.current)
      .finish_non_exhaustive()
  }
}

/// What an element arranges its children through, during a [`LayoutManager::run`]: it knows the
/// device-pixel grid the rectangles land on.
///
/// ```
/// # use artic_layout::geometry::{Rect, Size};
/// # use artic_layout::{ArrangeCx, Layoutable, MeasureCx};
/// struct Hairline;
///
/// impl Layoutable for Hairline {
///   fn measure(&self, _cx: &mut MeasureCx<'_>, _available: Size) -> Size {
///     Size::ZERO
///   }
///
///   fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
///     let _ = cx.snap(Rect::new(rect.x, rect.y, rect.width, 1.0 / cx.scale()));
///   }
/// }
/// ```
pub struct ArrangeCx<'a> {
  pass: Pass<'a>,
  grid: PixelGrid,
}

impl<'a> ArrangeCx<'a> {
  /// The node being arranged.
  ///
  /// # Panics
  ///
  /// Panics outside a node's `arrange`.
  #[must_use]
  pub fn node(&self) -> NodeId {
    self.pass.node()
  }

  /// The tree being laid out.
  #[must_use]
  pub fn tree(&self) -> &'a UiTree {
    self.pass.tree
  }

  /// The visual children of the node being arranged, in order, leaving out collapsed ones.
  pub fn children(&self) -> impl Iterator<Item = NodeId> + use<'a> {
    self.pass.children()
  }

  /// The text engine, for shaping the node's text again at the width it was given.
  pub fn text(&mut self) -> &mut dyn TextEngine {
    &mut *self.pass.text
  }

  /// Device pixels per logical pixel.
  #[must_use]
  pub fn scale(&self) -> f32 {
    self.grid.scale
  }

  /// `rect` on the device-pixel grid, or unchanged when snapping is off.
  #[must_use]
  pub fn snap(&self, rect: Rect) -> Rect {
    if self.grid.snap { rect.snap(self.grid.scale) } else { rect }
  }

  /// The size `child` asked for in its last measure, or zero if it wasn't measured.
  #[must_use]
  pub fn desired(&self, child: NodeId) -> Size {
    self.pass.slots.get(&child).map_or(Size::ZERO, |slot| slot.desired)
  }

  /// Arranges `child` in `rect`, in the coordinates of the node being arranged, snapped to the
  /// device-pixel grid. A child already offered that rectangle, with nothing changed, is skipped,
  /// and a collapsed child isn't arranged.
  ///
  /// # Panics
  ///
  /// In debug builds, panics if `rect` isn't finite; release builds treat such a field as zero.
  pub fn arrange(&mut self, child: NodeId, rect: Rect) {
    if self.pass.collapsed(child) {
      return;
    }
    let rect = self.snap(self.checked_rect(child, rect));
    let slot = self.pass.slots.entry(child).or_default();
    if slot.offered == Some(rect) && !slot.arrange_dirty {
      return;
    }
    let moved = !slot.arrange_dirty && slot.offered.map(Rect::size) == Some(rect.size());
    slot.offered = Some(rect);
    slot.rect = Some(rect);
    slot.arrange_dirty = false;
    self.pass.stats.arranged += 1;
    if !moved && self.pass.results.arranged.remove(&child).is_some() {
      self.pass.results.changed(child);
    }
    if let Some(layoutable) = self.pass.nodes.layoutable(self.pass.tree, child) {
      let parent = self.pass.current.replace(child);
      layoutable.arrange(self, rect);
      self.pass.current = parent;
    }
    if let Some(after) = self.pass.slots.get(&child).and_then(|slot| slot.rect).map(Rect::size)
      && self.pass.sizes.insert(child, after).is_some_and(|before| before != after)
    {
      self.pass.stats.resized.push(child);
    }
  }

  /// Arranges every visual child of the node being arranged in `rect`, in its own coordinates.
  pub fn arrange_children(&mut self, rect: Rect) {
    let children: Vec<NodeId> = self.children().collect();
    for child in children {
      self.arrange(child, rect);
    }
  }

  /// Keeps `result` as what this arrange produced, in place of the node's measured result, for as
  /// long as the node only moves: it's dropped when anything but the node's position changes, such
  /// as a new measure, another offered size, a new [`PixelGrid`] or
  /// [`invalidate_arrange`](LayoutManager::invalidate_arrange).
  ///
  /// # Panics
  ///
  /// Panics outside a node's `arrange`.
  pub fn set_arranged<T: Any>(&mut self, result: T) {
    let node = self.pass.node();
    self.pass.results.arranged.insert(node, Box::new(result));
    self.pass.results.changed(node);
  }

  /// The arranged result the node kept from an arrange it has only moved since, if it's a `T`.
  ///
  /// # Panics
  ///
  /// Panics outside a node's `arrange`.
  #[must_use]
  pub fn arranged<T: Any>(&self) -> Option<&T> {
    self.pass.results.arranged.get(&self.pass.node())?.downcast_ref()
  }

  /// The result the node's last measure kept, if it's a `T`, so an arrange can tell whether the
  /// box it got changes what it measured.
  ///
  /// # Panics
  ///
  /// Panics outside a node's `arrange`.
  #[must_use]
  pub fn measured<T: Any>(&self) -> Option<&T> {
    self.pass.results.measured.get(&self.pass.node())?.downcast_ref()
  }

  /// Sets the bounds of the node being arranged, in its parent's coordinates, when they aren't the
  /// rectangle its parent offered: after a margin and alignment, for instance. Its children are
  /// placed relative to these bounds. They're snapped like [`arrange`](ArrangeCx::arrange)'s, and
  /// returned as recorded, so the node arranges its children in the box layout reports.
  ///
  /// # Panics
  ///
  /// Panics outside a node's `arrange`.
  pub fn set_bounds(&mut self, bounds: Rect) -> Rect {
    let node = self.pass.node();
    let bounds = self.snap(self.checked_rect(node, bounds));
    self.pass.slots.entry(node).or_default().rect = Some(bounds);
    bounds
  }

  fn checked_rect(&self, child: NodeId, rect: Rect) -> Rect {
    let fields = [rect.x, rect.y, rect.width, rect.height];
    debug_assert!(
      fields.iter().all(|field| field.is_finite()),
      "{} arranged node {child:?} in a rectangle that isn't finite ({rect})",
      self.pass.describe_parent(),
    );
    Rect::new(
      finite::or_zero(rect.x),
      finite::or_zero(rect.y),
      finite::or_zero(rect.width),
      finite::or_zero(rect.height),
    )
  }
}

impl std::fmt::Debug for ArrangeCx<'_> {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("ArrangeCx")
      .field("current", &self.pass.current)
      .field("grid", &self.grid)
      .finish_non_exhaustive()
  }
}

/// Whether `node`'s layoutable says it's collapsed; a node with none isn't.
fn collapsed(nodes: &dyn LayoutNodes, tree: &UiTree, node: NodeId) -> bool {
  nodes.layoutable(tree, node).is_some_and(|layoutable| layoutable.collapsed())
}

/// `desired`, checked to be finite and not negative.
fn checked_desired(child: NodeId, desired: Size, available: Size) -> Size {
  let valid = |axis: f32| finite::usable(axis) == axis;
  debug_assert!(
    valid(desired.width) && valid(desired.height),
    "node {child:?} returned an invalid desired size ({desired}) for available size ({available}): it must be finite and zero or more",
  );
  Size::new(finite::usable(desired.width), finite::usable(desired.height))
}
