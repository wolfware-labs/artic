# Design: layout tests

Issue: #56. Builds on: the layout manager (#41), the tree (#7, #14), sizes and rectangles (#22). Unblocks: #44 (`Panel` and `StackPanel`), #71.

A layout test builds a tree with no window and no GPU, runs `LayoutManager`, and checks sizes and rectangles. When a check fails, the message names the node and shows both values. Property tests run the same checks over random available sizes and scales.

```rust,ignore
#[test]
fn stack_puts_children_top_to_bottom() {
  let mut layout = Headless::new();
  let first = layout.add("first", Fixed::new(100.0, 20.0));
  let second = layout.add("second", Fixed::new(80.0, 30.0));
  let root = layout.parent("stack", Stack::new(), &[first, second]);

  layout.run(root, Size::new(400.0, 300.0), 1.0);

  layout.assert_desired(root, Size::new(100.0, 50.0));
  layout.assert_rect(second, Rect::new(0.0, 20.0, 400.0, 30.0));
}
```

## 1. Running layout headless

The helpers live in `artic_test::layout`. The module imports `artic-core`, `artic-layout`, `artic-element` and `proptest`, and from `artic-render` only the scene types `Headless::record` returns, so a layout test never needs a GPU.

```rust,ignore
pub struct Headless<N: LayoutNodes> { /* tree, manager, nodes, labels */ }

impl<N: LayoutNodes> Headless<N> {
  pub fn new(nodes: N) -> Headless<N>;
  pub fn add(&mut self, label: &str, object: impl Object) -> NodeId;
  pub fn parent(&mut self, label: &str, object: impl Object, children: &[NodeId]) -> NodeId;
  pub fn tree(&mut self) -> &mut UiTree;
  pub fn manager(&mut self) -> &mut LayoutManager;
  pub fn set_snap(&mut self, snap: bool);
  pub fn run(&mut self, root: NodeId, available: Size, scale: f32) -> LayoutStats;
  pub fn label(&self, node: NodeId) -> String;
}
```

`Headless` owns the test text engine (`artic_test::text::engine()`, from the [text design](text.md)) and passes it to `LayoutManager::run`, so `run` keeps its arguments and text measures the same on every machine.

`parent` adds the node and links `children` as its visual children, in order. `run` calls `LayoutManager::run` with the `LayoutNodes` given to `new`. Snapping is on, since a window always snaps, until `set_snap(false)`. A test that needs something else `Headless` doesn't wrap, such as invalidating a node, calls `tree()` and `manager()` directly.

`label` returns the label given to `add` or `parent`. A node allocated through `tree()` has none and shows as its `NodeId`.

## 2. Assertions

```rust,ignore
pub const TOLERANCE: f32 = 0.001;

impl<N: LayoutNodes> Headless<N> {
  #[track_caller] pub fn assert_desired(&self, node: NodeId, expected: Size);
  #[track_caller] pub fn assert_desired_within(&self, node: NodeId, expected: Size, tolerance: f32);
  #[track_caller] pub fn assert_rect(&self, node: NodeId, expected: Rect);
  #[track_caller] pub fn assert_rect_within(&self, node: NodeId, expected: Rect, tolerance: f32);
}
```

A failure panics with the label and both values, at the test's line:

```text
"second": rect expected 0,20,400,30, got 0,20,400,24
```

`assert_desired` on a node that wasn't measured fails with `"second": not measured`, and `assert_rect` on one that wasn't arranged with `"second": not arranged`.

The tolerance is absolute, in logical units. Layout values are a few thousand units at most, and snapping already rounds to the device grid, so the remaining error is `f32` arithmetic, far below 0.001. A relative tolerance would only add a second number to reason about. Each value is compared on its own: infinity matches the same infinity only, and NaN never matches, so a NaN in a result always fails.

## 3. Test elements

Most layout tests need a leaf of a known size and a parent that places its children. `artic_test::layout` ships both, so #44 and later panels don't each write their own.

| Item | What it does |
| --- | --- |
| `Fixed` | a leaf: `Fixed::new(width, height)` asks for that size whatever is available. `set_size` changes it, for tests that invalidate. `measured()` and `arranged()` count the calls |
| `Stack` | places its children top to bottom, each as wide as the stack and as tall as it asked; asks for the widest child's width and the sum of the heights. It counts calls the same way |

Both are elements that draw nothing, registered with `artic_element::element_type!`, so `Headless::new()` lays them out through `ElementTypes` beside every control. A test with its own element registers it the same way. A test that needs a `LayoutNodes` of its own, such as one with elements that break the layout rules on purpose, passes it to `Headless::with_nodes`.

## 4. Strategies and invariants

```rust,ignore
pub mod strategy {
  pub fn available() -> impl Strategy<Value = Size>;
  pub fn scale() -> impl Strategy<Value = f32>;
}
```

`available` picks each axis on its own: 0, a value from 1 to 1000, or infinity. `scale` picks 1, 1.25, 1.5, 2 or 3, the scales displays use, where snapping differs.

```rust,ignore
#[derive(Default)]
pub struct Invariants { /* contained: bool */ }

impl Invariants {
  pub fn contained(self) -> Invariants;
  #[track_caller]
  pub fn assert<N: LayoutNodes>(&self, layout: &mut Headless<N>, root: NodeId, available: Size, scale: f32);
}
```

The manager already enforces the first of the three rules in #56, finite results: a desired size or rectangle that isn't finite fails a `debug_assert!` naming the node by `NodeId`, and release builds replace it with 0. Tests run in debug builds, so a property test meets that panic before `assert` could see the value. `assert` runs layout from `root` and checks the other two rules on every node under it:

1. **Deterministic.** A fresh `LayoutManager` on the same tree gives the same desired sizes and rectangles, within the tolerance. This catches results cached from an earlier run that should have been recomputed, such as a missed invalidation, and an element whose answer changes from one call to the next. Both managers measure the same element objects, so state an element keeps in itself goes unseen.
2. **Contained**, only after `.contained()`. When `available` is finite on both axes, each child's rectangle lies inside `(0, 0, parent width, parent height)`, within the tolerance.

Containment is opt-in because some layouts break it on purpose: a negative margin, a canvas, a scrolled child, or a stack taller than its parent, since `Stack` gives each child the height it asked for. With snapping on, the check allows one device pixel. The manager snaps a child's edges in its parent's coordinates, so they can round past the parent's own snapped size: a parent at y 0.6 with height 0.8 snaps to height 0, and its child to 0..1.

`layout_invariants_hold` leaves containment off, since its children can add up to more than the available height. A broken invariant panics with the node's label and the input. The manager's panic carries no label, but either one fails the `proptest!` case, and proptest shrinks it to the smallest failing input.

```rust,ignore
proptest! {
  #[test]
  fn layout_invariants_hold(
    sizes in vec((0.0..500.0f32, 0.0..500.0f32), 0..6),
    available in strategy::available(),
    scale in strategy::scale(),
  ) {
    let mut layout = Headless::new();
    let children: Vec<_> = sizes
      .iter()
      .enumerate()
      .map(|(index, (w, h))| layout.add(&format!("child {index}"), Fixed::new(*w, *h)))
      .collect();
    let root = layout.parent("stack", Stack::new(), &children);
    Invariants::default().assert(&mut layout, root, available, scale);
  }
}
```

## 5. The #41 tests stay

The tests in `artic-layout/src/layout/tests.rs` keep their own helpers. Using `artic-test` from them would need `artic-test` as a dev-dependency of `artic-layout`, and `artic-test` already depends on `artic-layout`. The [dependency rules](../architecture.md#rules) allow a dev-dependency on `artic-test` only when `artic-test` doesn't depend back. `Fixed` and `Stack` start from the test node those tests use, so the two stay alike.

## 6. What waits

| Feature | How it fits | Tracked in |
| --- | --- | --- |
| `StackPanel` and `Panel` tests | `Headless` with the element types, and `Invariants` over random children | #44 |
| Simulated input | a separate `TestHost` | #101 |
| Tree snapshots | a separate serializer | #60 |

## 7. Testing

| Crate | Tests |
| --- | --- |
| `artic-test` | `layout_invariants_hold`; `invariants_accept_children_inside_their_parent`, a stack whose children fit; `invariants_catch_a_child_outside_its_parent` and `invariants_catch_a_changing_answer`, each with a broken element that must fail; `failing_assertion_names_the_node`, `nodes_without_a_label_show_their_id`; `rects_within_tolerance_match`, `infinity_matches_only_infinity`, `nan_never_matches`; `available_sizes_cover_zero_finite_and_infinite` |

The tests with a broken element prove that `Invariants` can fail. Without them, a helper that checked nothing would pass forever.

## 8. Pull requests

| PR | Contents | Done when |
| --- | --- | --- |
| A | this document | approved |
| B | `artic_test::layout` with `Headless`, the assertions, `Fixed`, `Stack`, `strategy` and `Invariants`, with a rustdoc example; `proptest` as a dependency of `artic-test` | the tests in section 7 pass, and a layout test needs no GPU |
