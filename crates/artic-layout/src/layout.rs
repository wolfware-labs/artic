#[cfg(test)]
mod tests;

use crate::finite;
use crate::geometry::{Rect, Size};
use crate::text::TextEngine;
use artic_core::property::Affects;
use artic_core::tree::{NodeId, NodeMap, TreeKind, UiTree};
use std::any::Any;
use std::collections::BTreeMap;

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

/// What a node's last measure and arrange were given and worked out.
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
/// A run visits the nodes marked since the last one, their ancestors, and what their measures and
/// arranges reach; it walks the whole subtree only after the tree's structure changed, to forget
/// the nodes that left it.
///
/// A new [`PixelGrid`] arranges every node again, and measures only what the new logical size
/// reaches: a desired size doesn't depend on the display.
#[derive(Debug, Default)]
pub struct LayoutManager {
  state: State,
  /// The root of the last structure sweep, and the tree's structure generation it saw.
  swept: Option<(NodeId, u64)>,
  grid: Option<PixelGrid>,
}

/// What layout knows about one node.
#[derive(Debug, Default)]
struct Record {
  slot: Slot,
  /// The size of the node's last arrange, kept while its parent stops laying it out.
  size: Option<Size>,
  /// What the node's last measure set.
  measured: Option<Box<dyn Any>>,
  /// What the node's arrange set, kept while the node only moves.
  arranged: Option<Box<dyn Any>>,
  /// Whether the node is in [`State::changed`].
  listed: bool,
  /// Whether the node is measured again this run, whatever its parent offers. A run ends with it
  /// unset: the node is measured, or a parent that skips it forgets it.
  dirty: bool,
  /// How far below the root the node is, as of the last structure sweep.
  depth: u32,
  /// The stamp of the measure that last measured the node, through its parent.
  seen: u64,
  /// The stamp of the last structure sweep that found the node under the root.
  swept: u64,
}

impl Record {
  fn has_result(&self) -> bool {
    self.measured.is_some() || self.arranged.is_some()
  }

  /// Drops what layout worked out for the node, keeping its size from before, and returns whether
  /// it had a result.
  fn forget(&mut self) -> bool {
    let had_result = self.has_result();
    *self = Record {
      size: self.size,
      listed: self.listed,
      depth: self.depth,
      swept: self.swept,
      ..Record::default()
    };
    had_result
  }
}

/// Every node's [`Record`], and the lists a run works through.
#[derive(Debug, Default)]
struct State {
  records: NodeMap<Record>,
  /// The nodes whose result was set or dropped, in the order they first changed.
  changed: Vec<NodeId>,
  /// The nodes marked to be arranged again since the last run, in the order they were marked.
  waiting: Vec<NodeId>,
  /// The last stamp handed out.
  stamp: u64,
}

impl State {
  /// `node`'s record, made if it has none, in place of any a freed node left in its slot; a result
  /// that record held is noted as dropped.
  fn record(&mut self, node: NodeId) -> &mut Record {
    Self::record_in(&mut self.records, &mut self.changed, node)
  }

  fn record_in<'s>(records: &'s mut NodeMap<Record>, changed: &mut Vec<NodeId>, node: NodeId) -> &'s mut Record {
    if records.get(node).is_none()
      && let Some((freed, record)) = records.insert(node, Record::default())
    {
      Self::dropped(changed, freed, &record);
    }
    records.get_mut(node).expect("made above")
  }

  /// Notes that `node`, whose `record` goes, dropped its result if it had one.
  fn dropped(changed: &mut Vec<NodeId>, node: NodeId, record: &Record) {
    if record.has_result() && !record.listed {
      changed.push(node);
    }
  }

  fn next_stamp(&mut self) -> u64 {
    self.stamp += 1;
    self.stamp
  }

  /// Notes that `node`'s result was set or dropped.
  fn changed(&mut self, node: NodeId) {
    let record = self.record(node);
    if !record.listed {
      record.listed = true;
      self.changed.push(node);
    }
  }

  /// Drops what layout worked out for `node`, and notes the node if it had a result.
  fn forget(&mut self, node: NodeId) {
    if self.records.get_mut(node).is_some_and(Record::forget) {
      self.changed(node);
    }
  }

  /// Drops what layout worked out for every node, and notes the nodes that had a result.
  fn forget_all(&mut self) {
    for (node, record) in self.records.iter_mut() {
      if record.forget() && !record.listed {
        record.listed = true;
        self.changed.push(node);
      }
    }
    self.waiting.clear();
  }

  /// Marks `node` to be arranged again, even in the same rectangle.
  fn arrange_later(&mut self, node: NodeId) {
    let record = Self::record_in(&mut self.records, &mut self.changed, node);
    Self::wait(&mut self.waiting, node, record);
  }

  /// Marks every node to be arranged again, even in the same rectangle.
  fn arrange_all_later(&mut self) {
    for (node, record) in self.records.iter_mut() {
      Self::wait(&mut self.waiting, node, record);
    }
  }

  fn wait(waiting: &mut Vec<NodeId>, node: NodeId, record: &mut Record) {
    if !record.slot.arrange_dirty {
      record.slot.arrange_dirty = true;
      waiting.push(node);
    }
  }

  /// Hands over the nodes noted since the last call.
  fn take_changed(&mut self) -> Vec<NodeId> {
    for node in &self.changed {
      if let Some(record) = self.records.get_mut(*node) {
        record.listed = false;
      }
    }
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
  /// Results of nodes no longer in the subtree are dropped, and a collapsed or stale root lays out
  /// nothing. Returns what it measured, arranged and resized.
  pub fn run(
    &mut self,
    tree: &mut UiTree,
    nodes: &impl LayoutNodes,
    root: NodeId,
    available: Size,
    grid: PixelGrid,
    text: &mut dyn TextEngine,
  ) -> LayoutStats {
    if !tree.contains(root) {
      return LayoutStats::default();
    }
    if self.grid.replace(grid) != Some(grid) {
      self.state.arrange_all_later();
    }
    let (cx, desired) = self.measure_pass(tree, nodes, root, available, text);
    if cx.pass.collapsed(root) {
      let mut pass = cx.pass;
      pass.stats.results = pass.state.take_changed();
      return pass.stats;
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
    let records = &cx.pass.state.records;
    let mut waiting: Vec<(u32, NodeId)> = std::mem::take(&mut cx.pass.state.waiting)
      .into_iter()
      .filter_map(|node| {
        records
          .get(node)
          .filter(|record| record.slot.arrange_dirty)
          .map(|record| (record.depth, node))
      })
      .collect();
    waiting.sort_by_key(|(depth, _)| *depth);
    for (_, node) in waiting {
      let slot = cx.pass.state.records.get(node).map(|record| record.slot);
      if let Some(slot) = slot
        && slot.arrange_dirty
        && let Some(rect) = slot.offered
      {
        cx.arrange(node, rect);
      }
    }
    cx.pass.stats.results = cx.pass.state.take_changed();
    cx.pass.stats
  }

  /// Measures `root`'s visual subtree in `available`, shaping text with `text`, as a
  /// [`run`](LayoutManager::run) does first, without arranging it, and returns the size it wants. A
  /// later `run` in the same `available` answers every measure from the cache. A collapsed or stale
  /// root measures nothing and returns zero.
  pub fn measure(
    &mut self,
    tree: &mut UiTree,
    nodes: &impl LayoutNodes,
    root: NodeId,
    available: Size,
    text: &mut dyn TextEngine,
  ) -> Size {
    if !tree.contains(root) {
      return Size::ZERO;
    }
    self.measure_pass(tree, nodes, root, available, text).1
  }

  /// Gives every node under `root` its depth, and forgets the nodes no longer under it.
  fn sweep(&mut self, tree: &UiTree, root: NodeId) {
    let state = &mut self.state;
    let stamp = state.next_stamp();
    let mut pending = vec![(root, 0)];
    while let Some((node, depth)) = pending.pop() {
      let record = state.record(node);
      (record.depth, record.swept) = (depth, stamp);
      pending.extend(tree.children(TreeKind::Visual, node).map(|child| (child, depth + 1)));
    }
    let changed = &mut state.changed;
    state.records.retain(|node, record| {
      let kept = record.swept == stamp;
      if !kept {
        State::dropped(changed, node, record);
      }
      kept
    });
  }

  fn measure_pass<'a>(
    &'a mut self,
    tree: &'a mut UiTree,
    nodes: &'a impl LayoutNodes,
    root: NodeId,
    available: Size,
    text: &'a mut dyn TextEngine,
  ) -> (MeasureCx<'a>, Size) {
    let swept = Some((root, tree.structure_generation()));
    if self.swept != swept {
      self.swept = swept;
      self.sweep(tree, root);
    }
    let mut dirty = tree.take_dirty(Affects::LAYOUT, TreeKind::Visual, root);
    let state = &mut self.state;
    if collapsed(nodes, tree, root) {
      state.forget_all();
      dirty.clear();
    }
    for node in &dirty {
      state.record(*node).dirty = true;
    }

    let is_dirty = |state: &State, node: NodeId| state.records.get(node).is_some_and(|record| record.dirty);
    let depth = |state: &State, node: NodeId| state.records.get(node).map_or(0, |record| record.depth);
    let mut by_depth: BTreeMap<u32, Vec<NodeId>> = BTreeMap::new();
    let mut between = Vec::new();
    for node in &dirty {
      let mut path = Vec::new();
      let mut nested = false;
      for ancestor in tree.ancestors(TreeKind::Visual, *node) {
        if is_dirty(state, ancestor) {
          nested = true;
          between.append(&mut path);
          break;
        }
        if ancestor == root {
          break;
        }
        path.push(ancestor);
      }
      if !nested {
        by_depth.entry(depth(state, *node)).or_default().push(*node);
      }
    }
    for node in between {
      state.record(node).dirty = true;
    }
    let mut cx = MeasureCx {
      pass: Pass {
        tree,
        nodes,
        state,
        text,
        current: None,
        stats: LayoutStats::default(),
      },
      measuring: None,
    };

    // Upward, deepest first: a dirty node measures again in the size it had. If its desired size
    // is unchanged, the change stops there and only the node is arranged again; otherwise its
    // parent is next. The order is what lets a clean child answer from its cache: when a node is
    // measured here, every dirty node below it has either been measured and marked its way up, or
    // sits under a dirty ancestor on a path `between` kept dirty, so a child still clean has
    // nothing new under it.
    while let Some((_, deepest)) = by_depth.pop_last() {
      for node in deepest {
        if node == root || !is_dirty(cx.pass.state, node) {
          continue;
        }
        let slot = cx.pass.state.records.get(node).map(|record| record.slot).unwrap_or_default();
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
          && !is_dirty(cx.pass.state, parent)
        {
          cx.pass.state.record(parent).dirty = true;
          by_depth.entry(depth(cx.pass.state, parent)).or_default().push(parent);
        }
      }
    }

    // Downward: measure from the root; clean nodes answer from their cache.
    let desired = cx.measure(root, available);
    (cx, desired)
  }

  /// Marks `node` to be measured again in the next [`run`](LayoutManager::run).
  pub fn invalidate_measure(tree: &mut UiTree, node: NodeId) {
    tree.mark_dirty(node, Affects::LAYOUT);
  }

  /// Marks `node` to be arranged again in the next [`run`](LayoutManager::run), in the same
  /// rectangle, for a change that moves its children without changing its size. A node not laid
  /// out yet is arranged with its first layout anyway.
  pub fn invalidate_arrange(&mut self, node: NodeId) {
    if let Some(record) = self.state.records.get_mut(node) {
      State::wait(&mut self.state.waiting, node, record);
    }
  }

  /// The size `node` last asked for, if it was measured.
  #[must_use]
  pub fn desired(&self, node: NodeId) -> Option<Size> {
    self
      .state
      .records
      .get(node)
      .filter(|record| record.slot.available.is_some())
      .map(|record| record.slot.desired)
  }

  /// What `node`'s layout produced for it to draw: its arranged result if it has one, and its
  /// measured result otherwise.
  #[must_use]
  pub fn result(&self, node: NodeId) -> Option<&dyn Any> {
    let record = self.state.records.get(node)?;
    record.arranged.as_deref().or(record.measured.as_deref())
  }

  /// The bounds `node` was last arranged in, in its parent's coordinates.
  #[must_use]
  pub fn rect(&self, node: NodeId) -> Option<Rect> {
    self.state.records.get(node).and_then(|record| record.slot.rect)
  }
}

struct Pass<'a> {
  tree: &'a UiTree,
  nodes: &'a dyn LayoutNodes,
  state: &'a mut State,
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
  /// The stamp of the measure running, which each child it measures records.
  measuring: Option<u64>,
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
    let layoutable = self.pass.nodes.layoutable(self.pass.tree, child);
    if layoutable.is_some_and(Layoutable::collapsed) {
      return Size::ZERO;
    }
    let available = self.checked_available(child, available);
    let record = self.pass.state.record(child);
    if let Some(stamp) = self.measuring {
      record.seen = stamp;
    }
    if !record.dirty && record.slot.available == Some(available) {
      return record.slot.desired;
    }
    let had_measured = record.measured.take().is_some();
    let had_arranged = record.arranged.take().is_some();
    self.pass.stats.measured += 1;
    let stamp = self.pass.state.next_stamp();
    let desired = match layoutable {
      Some(layoutable) => {
        let parent = self.pass.current.replace(child);
        let outer = self.measuring.replace(stamp);
        let desired = layoutable.measure(self, available);
        self.measuring = outer;
        self.pass.current = parent;
        desired
      }
      None => Size::ZERO,
    };
    self.forget_unmeasured(child, stamp);
    let desired = checked_desired(child, desired, available);
    let record = self.pass.state.record(child);
    let has_result = record.measured.is_some();
    record.dirty = false;
    record.slot.available = Some(available);
    record.slot.desired = desired;
    if had_measured || had_arranged || has_result {
      self.pass.state.changed(child);
    }
    self.pass.state.arrange_later(child);
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
    self.pass.state.record(node).measured = Some(Box::new(result));
  }

  /// Forgets what layout knew about `node`'s visual children that its measure, stamped `stamp`,
  /// didn't measure, and about their subtrees, so neither the upward nor the waiting pass lays them
  /// out.
  fn forget_unmeasured(&mut self, node: NodeId, stamp: u64) {
    let (tree, state) = (self.pass.tree, &mut *self.pass.state);
    for child in tree.children(TreeKind::Visual, node) {
      if state.records.get(child).is_some_and(|record| record.seen == stamp) {
        continue;
      }
      state.forget(child);
      for descendant in tree.descendants(TreeKind::Visual, child) {
        state.forget(descendant);
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
    self
      .pass
      .state
      .records
      .get(child)
      .map_or(Size::ZERO, |record| record.slot.desired)
  }

  /// Arranges `child` in `rect`, in the coordinates of the node being arranged, snapped to the
  /// device-pixel grid. A rectangle that reaches past the largest finite coordinate is cut to end
  /// there. A child already offered that rectangle, with nothing changed, is skipped, and a
  /// collapsed child isn't arranged.
  ///
  /// # Panics
  ///
  /// In debug builds, panics if `rect` isn't finite or has a negative size; release builds treat
  /// such a field as zero.
  pub fn arrange(&mut self, child: NodeId, rect: Rect) {
    let layoutable = self.pass.nodes.layoutable(self.pass.tree, child);
    if layoutable.is_some_and(Layoutable::collapsed) {
      return;
    }
    let rect = self.snap(self.checked_rect(child, rect));
    let record = self.pass.state.record(child);
    let slot = &mut record.slot;
    if slot.offered == Some(rect) && !slot.arrange_dirty {
      return;
    }
    let moved = !slot.arrange_dirty && slot.offered.map(Rect::size) == Some(rect.size());
    slot.offered = Some(rect);
    slot.rect = Some(rect);
    slot.arrange_dirty = false;
    let dropped = !moved && record.arranged.take().is_some();
    self.pass.stats.arranged += 1;
    if dropped {
      self.pass.state.changed(child);
    }
    if let Some(layoutable) = layoutable {
      let parent = self.pass.current.replace(child);
      layoutable.arrange(self, rect);
      self.pass.current = parent;
    }
    if let Some(record) = self.pass.state.records.get_mut(child)
      && let Some(after) = record.slot.rect.map(Rect::size)
      && record.size.replace(after).is_some_and(|before| before != after)
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
    self.pass.state.record(node).arranged = Some(Box::new(result));
    self.pass.state.changed(node);
  }

  /// The arranged result the node kept from an arrange it has only moved since, if it's a `T`.
  ///
  /// # Panics
  ///
  /// Panics outside a node's `arrange`.
  #[must_use]
  pub fn arranged<T: Any>(&self) -> Option<&T> {
    self
      .pass
      .state
      .records
      .get(self.pass.node())?
      .arranged
      .as_deref()?
      .downcast_ref()
  }

  /// The result the node's last measure kept, if it's a `T`, so an arrange can tell whether the
  /// box it got changes what it measured.
  ///
  /// # Panics
  ///
  /// Panics outside a node's `arrange`.
  #[must_use]
  pub fn measured<T: Any>(&self) -> Option<&T> {
    self
      .pass
      .state
      .records
      .get(self.pass.node())?
      .measured
      .as_deref()?
      .downcast_ref()
  }

  /// Sets the bounds of the node being arranged, in its parent's coordinates, when they aren't the
  /// rectangle its parent offered: after a margin and alignment, for instance. Its children are
  /// placed relative to these bounds. They're snapped and cut to a finite far edge like
  /// [`arrange`](ArrangeCx::arrange)'s, and returned as recorded, so the node arranges its children
  /// in the box layout reports.
  ///
  /// # Panics
  ///
  /// Panics outside a node's `arrange`.
  pub fn set_bounds(&mut self, bounds: Rect) -> Rect {
    let node = self.pass.node();
    let bounds = self.snap(self.checked_rect(node, bounds));
    self.pass.state.record(node).slot.rect = Some(bounds);
    bounds
  }

  fn checked_rect(&self, child: NodeId, rect: Rect) -> Rect {
    let fields = [rect.x, rect.y, rect.width, rect.height];
    debug_assert!(
      fields.iter().all(|field| field.is_finite()) && rect.width >= 0.0 && rect.height >= 0.0,
      "{} arranged node {child:?} in a rectangle that isn't finite or has a negative size ({rect})",
      self.pass.describe_parent(),
    );
    let (x, y) = (finite::or_zero(rect.x), finite::or_zero(rect.y));
    Rect::new(
      x,
      y,
      finite::span(x, finite::usable(rect.width)),
      finite::span(y, finite::usable(rect.height)),
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
