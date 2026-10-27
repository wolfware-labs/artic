// The headless layout helpers, with the shared test elements and elements that break the rules
// on purpose.

use artic_core::Object;
use artic_core::property::PropertyVisitor;
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_layout::geometry::{Rect, Size};
use artic_layout::{ArrangeCx, LayoutManager, LayoutNodes, Layoutable, MeasureCx};
use artic_test::layout::strategy;
use artic_test::layout::{Boxes, Fixed, Headless, Invariants, Stack};
use pretty_assertions::assert_eq;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;
use std::cell::Cell;

const WINDOW: Size = Size::new(400.0, 300.0);

// A stack places its children top to bottom at its own width and asks for the widest child and
// the sum of the heights.
#[test]
fn headless_lays_out_a_stack() {
  let mut layout = Headless::new(Boxes);
  let first = layout.add("first", Fixed::new(100.0, 20.0));
  let second = layout.add("second", Fixed::new(80.0, 30.0));
  let root = layout.parent("stack", Stack::new(), &[first, second]);

  layout.run(root, WINDOW, 1.0);

  assert_eq!(layout.manager().desired(root), Some(Size::new(100.0, 50.0)));
  assert_eq!(layout.manager().rect(first), Some(Rect::new(0.0, 0.0, 400.0, 20.0)));
  assert_eq!(layout.manager().rect(second), Some(Rect::new(0.0, 20.0, 400.0, 30.0)));
}

// `parent` links the children as visual children, in the order given.
#[test]
fn parent_links_children_in_order() {
  let mut layout = Headless::new(Boxes);
  let first = layout.add("first", Fixed::new(1.0, 1.0));
  let second = layout.add("second", Fixed::new(1.0, 1.0));
  let root = layout.parent("stack", Stack::new(), &[second, first]);

  let children: Vec<_> = layout.tree().children(TreeKind::Visual, root).collect();

  assert_eq!(children, [second, first]);
}

// The elements count each measure and arrange, so tests can check what layout skipped.
#[test]
fn elements_count_their_calls() {
  let mut layout = Headless::new(Boxes);
  let leaf = layout.add("leaf", Fixed::new(10.0, 10.0));
  let root = layout.parent("stack", Stack::new(), &[leaf]);

  layout.run(root, WINDOW, 1.0);
  layout.run(root, WINDOW, 1.0);

  let tree = layout.tree();
  let fixed = tree.get_as::<Fixed>(leaf).expect("a fixed");
  let stack = tree.get_as::<Stack>(root).expect("a stack");
  assert_eq!((fixed.measured(), fixed.arranged()), (1, 1));
  assert_eq!((stack.measured(), stack.arranged()), (1, 1));
}

// A new size on a `Fixed` shows after its node is invalidated.
#[test]
fn set_size_shows_after_invalidating() {
  let mut layout = Headless::new(Boxes);
  let leaf = layout.add("leaf", Fixed::new(10.0, 10.0));
  let root = layout.parent("stack", Stack::new(), &[leaf]);
  layout.run(root, WINDOW, 1.0);

  layout
    .tree()
    .get_as::<Fixed>(leaf)
    .expect("a fixed")
    .set_size(Size::new(10.0, 40.0));
  LayoutManager::invalidate_measure(layout.tree(), leaf);
  layout.run(root, WINDOW, 1.0);

  assert_eq!(layout.manager().desired(root), Some(Size::new(10.0, 40.0)));
}

// `run` snaps to the device grid: at 1.25, a 10.3 tall child ends at 13 device pixels, 10.4.
#[test]
fn run_snaps_to_the_device_grid() {
  let mut layout = Headless::new(Boxes);
  let first = layout.add("first", Fixed::new(100.0, 10.3));
  let second = layout.add("second", Fixed::new(100.0, 10.0));
  let root = layout.parent("stack", Stack::new(), &[first, second]);

  layout.run(root, WINDOW, 1.25);

  let rect = layout.manager().rect(second).expect("arranged");
  assert!((rect.y - 10.4).abs() < 0.001, "{rect}");
}

// With snapping off, rectangles keep their fractions.
#[test]
fn set_snap_false_keeps_fractions() {
  let mut layout = Headless::new(Boxes);
  let first = layout.add("first", Fixed::new(100.0, 10.3));
  let second = layout.add("second", Fixed::new(100.0, 10.0));
  let root = layout.parent("stack", Stack::new(), &[first, second]);

  layout.set_snap(false);
  layout.run(root, WINDOW, 1.25);

  assert_eq!(layout.manager().rect(second), Some(Rect::new(0.0, 10.3, 400.0, 10.0)));
}

// A node made through the tree directly has no label and shows as its id.
#[test]
fn nodes_without_a_label_show_their_id() {
  let mut layout = Headless::new(Boxes);
  let labeled = layout.add("leaf", Fixed::new(1.0, 1.0));
  let bare = layout.tree().alloc(Box::new(Fixed::new(1.0, 1.0)));

  assert_eq!(layout.label(labeled), "leaf");
  assert_eq!(layout.label(bare), format!("{bare:?}"));
}

fn two_in_a_stack() -> (Headless<Boxes>, [NodeId; 3]) {
  let mut layout = Headless::new(Boxes);
  let first = layout.add("first", Fixed::new(100.0, 20.0));
  let second = layout.add("second", Fixed::new(80.0, 24.0));
  let root = layout.parent("stack", Stack::new(), &[first, second]);
  layout.run(root, WINDOW, 1.0);
  (layout, [root, first, second])
}

// Matching sizes and rectangles pass.
#[test]
fn matching_assertions_pass() {
  let (layout, [root, _, second]) = two_in_a_stack();

  layout.assert_desired(root, Size::new(100.0, 44.0));
  layout.assert_rect(second, Rect::new(0.0, 20.0, 400.0, 24.0));
}

// A wrong rectangle names the node and shows both values.
#[test]
#[should_panic(expected = r#""second": rect expected 0,20,400,30, got 0,20,400,24"#)]
fn failing_assertion_names_the_node() {
  let (layout, [_, _, second]) = two_in_a_stack();

  layout.assert_rect(second, Rect::new(0.0, 20.0, 400.0, 30.0));
}

// A wrong desired size reads the same way.
#[test]
#[should_panic(expected = r#""first": desired expected 100,30, got 100,20"#)]
fn failing_desired_names_the_node() {
  let (layout, [_, first, _]) = two_in_a_stack();

  layout.assert_desired(first, Size::new(100.0, 30.0));
}

// A node outside the laid-out subtree was never measured, and says so.
#[test]
#[should_panic(expected = r#""orphan": not measured"#)]
fn unplaced_node_is_not_measured() {
  let (mut layout, _) = two_in_a_stack();
  let orphan = layout.add("orphan", Fixed::new(1.0, 1.0));

  layout.assert_desired(orphan, Size::new(1.0, 1.0));
}

// The same node was never arranged either.
#[test]
#[should_panic(expected = r#""orphan": not arranged"#)]
fn unplaced_node_is_not_arranged() {
  let (mut layout, _) = two_in_a_stack();
  let orphan = layout.add("orphan", Fixed::new(1.0, 1.0));

  layout.assert_rect(orphan, Rect::new(0.0, 0.0, 1.0, 1.0));
}

// The default tolerance absorbs f32 error; a tighter one given to `_within` doesn't.
#[test]
#[should_panic(expected = r#""first": rect expected 0,0,400,20.0005"#)]
fn tighter_tolerance_fails() {
  let (layout, [_, first, _]) = two_in_a_stack();
  layout.assert_rect(first, Rect::new(0.0, 0.0, 400.0, 20.0005));

  layout.assert_rect_within(first, Rect::new(0.0, 0.0, 400.0, 20.0005), 0.0001);
}

// Expecting infinity from a finite result fails.
#[test]
#[should_panic(expected = r#""first": desired expected inf,20, got 100,20"#)]
fn infinite_expectation_fails() {
  let (layout, [_, first, _]) = two_in_a_stack();

  layout.assert_desired_within(first, Size::new(f32::INFINITY, 20.0), f32::MAX);
}

fn sample<T: std::fmt::Debug>(strategy: impl Strategy<Value = T>, count: usize) -> Vec<T> {
  let mut runner = TestRunner::deterministic();
  (0..count)
    .map(|_| strategy.new_tree(&mut runner).expect("a value").current())
    .collect()
}

// Each axis of an available size comes out zero, finite from 1 to 1000, or infinite, and all
// three show up on both axes.
#[test]
fn available_sizes_cover_zero_finite_and_infinite() {
  let sizes = sample(strategy::available(), 500);
  let axes: [fn(&Size) -> f32; 2] = [|size| size.width, |size| size.height];

  for axis in axes {
    let values: Vec<f32> = sizes.iter().map(axis).collect();
    assert!(
      values
        .iter()
        .all(|value| *value == 0.0 || (1.0..1000.0).contains(value) || *value == f32::INFINITY)
    );
    assert!(values.contains(&0.0));
    assert!(values.contains(&f32::INFINITY));
    assert!(values.iter().any(|value| value.is_finite() && *value >= 1.0));
  }
}

// Scales are the ones displays use, and every one comes up.
#[test]
fn scales_are_display_scales() {
  let scales = sample(strategy::scale(), 200);

  for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
    assert!(scales.contains(&scale), "{scale} never came up");
  }
  assert!(scales.iter().all(|scale| [1.0, 1.25, 1.5, 2.0, 3.0].contains(scale)));
}

#[derive(Debug, Default)]
struct Growing {
  calls: Cell<f32>,
}

impl Object for Growing {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl Layoutable for Growing {
  fn measure(&self, _cx: &mut MeasureCx<'_>, _available: Size) -> Size {
    self.calls.set(self.calls.get() + 1.0);
    Size::new(10.0 * self.calls.get(), 10.0)
  }

  fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}
}

#[derive(Debug, Default)]
struct Shove;

impl Object for Shove {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl Layoutable for Shove {
  fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    for child in cx.children() {
      let _ = cx.measure(child, available);
    }
    Size::new(10.0, 10.0)
  }

  fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
    for child in cx.children() {
      cx.arrange(child, Rect::new(rect.width, 0.0, 10.0, 10.0));
    }
  }
}

#[derive(Debug, Default)]
struct FirstOnly;

impl Object for FirstOnly {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl Layoutable for FirstOnly {
  fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    cx.children().next().map_or(Size::ZERO, |child| cx.measure(child, available))
  }

  fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
    if let Some(child) = cx.children().next() {
      cx.arrange(child, Rect::new(0.0, 0.0, rect.width, rect.height));
    }
  }
}

#[derive(Debug)]
struct Broken;

impl LayoutNodes for Broken {
  fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
    Boxes
      .layoutable(tree, node)
      .or_else(|| tree.get_as::<Growing>(node).map(|node| node as &dyn Layoutable))
      .or_else(|| tree.get_as::<Shove>(node).map(|node| node as &dyn Layoutable))
      .or_else(|| tree.get_as::<FirstOnly>(node).map(|node| node as &dyn Layoutable))
  }
}

proptest! {
  // A stack of any fixed-size children, laid out once at another size first, gives the same answer
  // as a fresh manager, for any available size and scale. Containment stays off: the children can
  // be taller than the stack.
  #[test]
  fn layout_invariants_hold(
    sizes in proptest::collection::vec((0.0..500.0f32, 0.0..500.0f32), 0..6),
    before in strategy::available(),
    available in strategy::available(),
    scale in strategy::scale(),
  ) {
    let mut layout = Headless::new(Boxes);
    let children: Vec<_> = sizes
      .iter()
      .enumerate()
      .map(|(index, (width, height))| layout.add(&format!("child {index}"), Fixed::new(*width, *height)))
      .collect();
    let root = layout.parent("stack", Stack::new(), &children);
    layout.run(root, before, scale);

    Invariants::default().assert(&mut layout, root, available, scale);
  }
}

// Children that fit pass the containment check.
#[test]
fn invariants_accept_children_inside_their_parent() {
  let mut layout = Headless::new(Boxes);
  let first = layout.add("first", Fixed::new(100.0, 20.0));
  let second = layout.add("second", Fixed::new(80.0, 30.0));
  let root = layout.parent("stack", Stack::new(), &[first, second]);

  Invariants::default().contained().assert(&mut layout, root, WINDOW, 1.25);
}

// A parent that puts its child past its right edge fails containment, naming both.
#[test]
#[should_panic(expected = r#""pushed": rect 100,0,10,10 lies outside its parent "shove" (100,100)"#)]
fn invariants_catch_a_child_outside_its_parent() {
  let mut layout = Headless::new(Broken);
  let pushed = layout.add("pushed", Fixed::new(10.0, 10.0));
  let root = layout.parent("shove", Shove, &[pushed]);

  Invariants::default()
    .contained()
    .assert(&mut layout, root, Size::new(100.0, 100.0), 1.0);
}

// Containment says nothing when an axis is infinite.
#[test]
fn containment_ignores_infinite_available() {
  let mut layout = Headless::new(Broken);
  let pushed = layout.add("pushed", Fixed::new(10.0, 10.0));
  let root = layout.parent("shove", Shove, &[pushed]);

  Invariants::default()
    .contained()
    .assert(&mut layout, root, Size::new(100.0, f32::INFINITY), 1.0);
}

// Without `.contained()`, the same layout passes.
#[test]
fn containment_is_opt_in() {
  let mut layout = Headless::new(Broken);
  let pushed = layout.add("pushed", Fixed::new(10.0, 10.0));
  let root = layout.parent("shove", Shove, &[pushed]);

  Invariants::default().assert(&mut layout, root, Size::new(100.0, 100.0), 1.0);
}

// An element that answers differently each time fails determinism. It's the root, so the failure
// names it rather than a parent whose size follows it.
#[test]
#[should_panic(expected = r#""growing": desired 10,10 from the layout, 20,10 from a fresh one"#)]
fn invariants_catch_a_changing_answer() {
  let mut layout = Headless::new(Broken);
  let growing = layout.add("growing", Growing::default());

  Invariants::default().assert(&mut layout, growing, WINDOW, 1.0);
}

// A child its parent leaves out has no results in either manager, and that's consistent.
#[test]
fn invariants_skip_nodes_a_parent_leaves_out() {
  let mut layout = Headless::new(Broken);
  let shown = layout.add("shown", Fixed::new(10.0, 10.0));
  let hidden = layout.add("hidden", Fixed::new(10.0, 10.0));
  let root = layout.parent("first only", FirstOnly, &[shown, hidden]);

  Invariants::default().contained().assert(&mut layout, root, WINDOW, 1.0);
}

// When a child's changing answer changes its parent's too, the failure names the child, where
// the bug is.
#[test]
#[should_panic(expected = r#""growing": desired"#)]
fn invariants_name_the_deepest_node() {
  let mut layout = Headless::new(Broken);
  let growing = layout.add("growing", Growing::default());
  let root = layout.parent("stack", Stack::new(), &[growing]);

  Invariants::default().assert(&mut layout, root, WINDOW, 1.0);
}

// Snapping can move a child's edge a device pixel past its snapped parent: a parent at y 0.6
// with height 0.8 snaps to 1..1, while its child snaps to 0..1 in the parent's coordinates. That
// layout is correct, so containment allows the pixel.
#[test]
fn containment_allows_a_device_pixel_of_snapping() {
  let mut layout = Headless::new(Boxes);
  let top = layout.add("top", Fixed::new(10.0, 0.6));
  let leaf = layout.add("leaf", Fixed::new(10.0, 0.8));
  let inner = layout.parent("inner", Stack::new(), &[leaf]);
  let root = layout.parent("outer", Stack::new(), &[top, inner]);

  Invariants::default().contained().assert(&mut layout, root, WINDOW, 1.0);
}
