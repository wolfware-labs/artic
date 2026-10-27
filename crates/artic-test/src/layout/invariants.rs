use super::TOLERANCE;
use super::compare::{rects_match, sizes_match};
use super::headless::Headless;
use artic_core::tree::{NodeId, TreeKind};
use artic_layout::geometry::{Rect, Size};
use artic_layout::{LayoutManager, PixelGrid};
use std::fmt::Display;

/// The rules a layout keeps for any input, checked on every node under a root.
///
/// It always checks that a fresh [`LayoutManager`] gives the same desired sizes and rectangles as
/// the layout's own. That catches results cached from an earlier run that should have been
/// recomputed, such as a missed invalidation, and an element whose answer changes from one call
/// to the next. Both managers measure the same element objects, so state an element keeps in
/// itself goes unseen. The extra runs also count in [`Fixed`](super::Fixed) and
/// [`Stack`](super::Stack) call counters. [`contained`](Invariants::contained) adds that each
/// child lies inside its parent. Finite results aren't checked here: the manager already panics
/// on them in debug builds.
#[derive(Debug, Clone, Copy, Default)]
pub struct Invariants {
  contained: bool,
}

impl Invariants {
  /// Also checks that each child's rectangle lies inside `(0, 0, parent width, parent height)`,
  /// when the available size is finite on both axes. With snapping on, an edge may stick out by
  /// one device pixel: the manager snaps a child's edges in its parent's coordinates, so they can
  /// round past the parent's own snapped size.
  #[must_use]
  pub fn contained(self) -> Invariants {
    Invariants { contained: true }
  }

  /// Lays out `root` in `available` at `scale`, then checks the invariants on every node under it.
  ///
  /// # Panics
  ///
  /// When one doesn't hold, naming the node and the input. Children are checked before their
  /// parents, so a wrong answer is reported where it starts rather than where it spreads to.
  #[track_caller]
  pub fn assert(&self, layout: &mut Headless, root: NodeId, available: Size, scale: f32) {
    layout.run(root, available, scale);
    let mut fresh = LayoutManager::default();
    let grid = PixelGrid {
      scale,
      snap: layout.snap,
    };
    fresh.run(&mut layout.tree, &layout.types, root, available, grid, &mut layout.text);
    let input = format!("for available {available} at scale {scale}");
    let contained = self.contained && available.width.is_finite() && available.height.is_finite();
    let mut order = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
      order.push(node);
      pending.extend(layout.tree.children(TreeKind::Visual, node));
    }
    for &node in order.iter().rev() {
      let (desired, again) = (layout.manager.desired(node), fresh.desired(node));
      assert!(
        same(desired, again, sizes_match),
        "{:?}: desired {} from the layout, {} from a fresh one, {input}",
        layout.label(node),
        shown(desired),
        shown(again)
      );
      let (rect, again) = (layout.manager.rect(node), fresh.rect(node));
      assert!(
        same(rect, again, rects_match),
        "{:?}: rect {} from the layout, {} from a fresh one, {input}",
        layout.label(node),
        shown(rect),
        shown(again)
      );
    }
    if !contained {
      return;
    }
    let slack = if layout.snap { TOLERANCE + 1.0 / scale } else { TOLERANCE };
    for &node in &order {
      let Some(parent) = layout.manager.rect(node) else {
        continue;
      };
      for child in layout.tree.children(TreeKind::Visual, node) {
        if let Some(rect) = layout.manager.rect(child) {
          assert!(
            inside(rect, parent, slack),
            "{:?}: rect {rect} lies outside its parent {:?} ({}), {input}",
            layout.label(child),
            layout.label(node),
            Size::new(parent.width, parent.height)
          );
        }
      }
    }
  }
}

fn same<T: Copy>(a: Option<T>, b: Option<T>, matches: fn(T, T, f32) -> bool) -> bool {
  match (a, b) {
    (Some(a), Some(b)) => matches(a, b, TOLERANCE),
    (None, None) => true,
    _ => false,
  }
}

fn shown<T: Display>(value: Option<T>) -> String {
  value.map_or_else(|| "nothing".to_owned(), |value| value.to_string())
}

fn inside(child: Rect, parent: Rect, slack: f32) -> bool {
  child.x >= -slack
    && child.y >= -slack
    && child.x + child.width <= parent.width + slack
    && child.y + child.height <= parent.height + slack
}
