// What a layout pass costs through real controls: a StackPanel of rows, each a StackPanel of ten
// Borders, laid out through the cast table as the frame pipeline does.
//
// The budget is 100 ns a node for a full relayout, so ten thousand nodes take about 1 ms of a
// 16.6 ms frame. The same tree of bare nodes is `cargo bench -p artic-layout --bench layout`, the
// floor under this one; the difference between the two is what element code adds.
//
// Compare against main as the core benches do:
//
//   cargo bench -p artic-controls --bench layout -- --save-baseline main
//   cargo bench -p artic-controls --bench layout -- --baseline main
//
// Each bench checks once, before it's timed, that a pass measures the nodes it says it does.

use artic_controls::{Border, StackPanel};
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_element::ElementTypes;
use artic_layout::geometry::Size;
use artic_layout::text::TextEngine;
use artic_layout::{LayoutManager, LayoutStats, Orientation, PixelGrid};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

const WINDOW: Size = Size::new(800.0, 600.0);
const GRID: PixelGrid = PixelGrid { scale: 1.0, snap: false };

/// A column of rows, each of ten 20 by 10 borders side by side: `leaves` borders under
/// `leaves / 10` rows under one root. Returns every node, the root first.
fn build(tree: &mut UiTree, leaves: usize) -> Vec<NodeId> {
  let root = tree.alloc(Box::new(StackPanel::default()));
  let mut all = vec![root];
  for _ in 0..leaves / 10 {
    let row = StackPanel::default();
    row.orientation.set(Orientation::Horizontal);
    let row = tree.alloc(Box::new(row));
    tree.add_child(TreeKind::Visual, root, row);
    all.push(row);
    for _ in 0..10 {
      let leaf = Border::default();
      leaf.layout.width.set(Some(20.0));
      leaf.layout.height.set(Some(10.0));
      let leaf = tree.alloc(Box::new(leaf));
      tree.add_child(TreeKind::Visual, row, leaf);
      all.push(leaf);
    }
  }
  all
}

fn nodes(leaves: usize) -> u32 {
  (1 + leaves / 10 + leaves) as u32
}

/// What a pass needs besides the tree: the cast table and the tests' text engine, which borders
/// never use.
struct Services {
  types: ElementTypes,
  text: Box<dyn TextEngine>,
}

impl Services {
  fn new() -> Self {
    Self {
      types: ElementTypes::from_link_time(),
      text: Box::new(artic_test::text::engine()),
    }
  }

  fn run(&mut self, layout: &mut LayoutManager, tree: &mut UiTree, root: NodeId) -> LayoutStats {
    layout.run(tree, &self.types, root, WINDOW, GRID, &mut *self.text)
  }
}

// The first layout of a tree, by a manager that knows nothing about it.
fn cold(c: &mut Criterion) {
  let mut group = c.benchmark_group("controls/cold");
  let mut services = Services::new();
  for leaves in [1000, 10_000] {
    let mut tree = UiTree::new();
    let root = build(&mut tree, leaves)[0];
    assert_eq!(
      services.run(&mut LayoutManager::default(), &mut tree, root).measured,
      nodes(leaves)
    );
    group.bench_function(BenchmarkId::from_parameter(leaves), |b| {
      b.iter(|| services.run(&mut LayoutManager::default(), &mut tree, root));
    });
  }
  group.finish();
}

// A pass over a tree that hasn't changed since the last one.
fn unchanged(c: &mut Criterion) {
  let mut group = c.benchmark_group("controls/unchanged");
  let mut services = Services::new();
  for leaves in [1000, 10_000] {
    let mut tree = UiTree::new();
    let root = build(&mut tree, leaves)[0];
    let mut layout = LayoutManager::default();
    services.run(&mut layout, &mut tree, root);
    assert_eq!(services.run(&mut layout, &mut tree, root).measured, 0);
    group.bench_function(BenchmarkId::from_parameter(leaves), |b| {
      b.iter(|| services.run(&mut layout, &mut tree, root));
    });
  }
  group.finish();
}

// Every node changed, so every node is measured again; marking them is part of the time.
fn relayout(c: &mut Criterion) {
  let mut group = c.benchmark_group("controls/relayout");
  let mut services = Services::new();
  for leaves in [1000, 10_000] {
    let mut tree = UiTree::new();
    let all = build(&mut tree, leaves);
    let root = all[0];
    let mut layout = LayoutManager::default();
    services.run(&mut layout, &mut tree, root);
    let mut pass = |layout: &mut LayoutManager, tree: &mut UiTree| {
      for node in &all {
        LayoutManager::invalidate_measure(tree, *node);
      }
      services.run(layout, tree, root)
    };
    assert_eq!(pass(&mut layout, &mut tree).measured, nodes(leaves));
    group.bench_function(BenchmarkId::from_parameter(leaves), |b| {
      b.iter(|| pass(&mut layout, &mut tree));
    });
  }
  group.finish();
}

// One border's height changes: it, its row and the root are measured again, and nothing else.
// The property marks the border itself, so the write is part of the time.
fn one_leaf(c: &mut Criterion) {
  let mut group = c.benchmark_group("controls/one_leaf");
  let mut services = Services::new();
  for leaves in [1000, 10_000] {
    let mut tree = UiTree::new();
    let all = build(&mut tree, leaves);
    let (root, leaf) = (all[0], *all.last().expect("a leaf"));
    let mut layout = LayoutManager::default();
    services.run(&mut layout, &mut tree, root);
    let mut pass = |layout: &mut LayoutManager, tree: &mut UiTree| {
      let height = &tree.get_as::<Border>(leaf).expect("the last node is a border").layout.height;
      height.set(height.get().map(|height| 30.0 - height));
      services.run(layout, tree, root)
    };
    assert_eq!(pass(&mut layout, &mut tree).measured, 3);
    group.bench_function(BenchmarkId::from_parameter(leaves), |b| {
      b.iter(|| pass(&mut layout, &mut tree));
    });
  }
  group.finish();
}

criterion_group!(benches, cold, unchanged, relayout, one_leaf);
criterion_main!(benches);
