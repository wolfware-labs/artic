// What a layout pass costs in LayoutManager alone: the floor under every control's own work. The
// nodes are bare: a leaf returns its fixed size, and a stack sums its children's heights.
//
// The budget is 100 ns a node for a full relayout, so ten thousand nodes take about 1 ms of a
// 16.6 ms frame. The same tree of real controls is `cargo bench -p artic-controls --bench layout`;
// the difference between the two is what element code adds.
//
// Compare against main as the core benches do:
//
//   cargo bench -p artic-layout --bench layout -- --save-baseline main
//   cargo bench -p artic-layout --bench layout -- --baseline main
//
// Each bench checks once, before it's timed, that a pass measures the nodes it says it does.

use artic_core::Object;
use artic_core::property::PropertyVisitor;
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_layout::geometry::{Rect, Size};
use artic_layout::text::{Paragraph, ParagraphStyle, TextEngine, TextLimits, TextStyle};
use artic_layout::{ArrangeCx, LayoutManager, LayoutNodes, LayoutStats, Layoutable, MeasureCx, PixelGrid};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::cell::Cell;

const WINDOW: Size = Size::new(800.0, 600.0);
const GRID: PixelGrid = PixelGrid { scale: 1.0, snap: false };

/// A leaf of a fixed size, or a stack of its children from top to bottom, side by side when
/// `across`.
#[derive(Debug)]
enum Node {
  Leaf(Cell<Size>),
  Stack { across: bool },
}

impl Object for Node {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl Layoutable for Node {
  fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    let across = match self {
      Node::Leaf(size) => return size.get(),
      Node::Stack { across } => *across,
    };
    let children: Vec<NodeId> = cx.children().collect();
    children.into_iter().fold(Size::ZERO, |total, child| {
      let desired = cx.measure(child, available);
      if across {
        Size::new(total.width + desired.width, total.height.max(desired.height))
      } else {
        Size::new(total.width.max(desired.width), total.height + desired.height)
      }
    })
  }

  fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
    let Node::Stack { across } = self else { return };
    let children: Vec<NodeId> = cx.children().collect();
    let mut offset = 0.0;
    for child in children {
      let desired = cx.desired(child);
      if *across {
        cx.arrange(child, Rect::new(offset, 0.0, desired.width, rect.height));
        offset += desired.width;
      } else {
        cx.arrange(child, Rect::new(0.0, offset, rect.width, desired.height));
        offset += desired.height;
      }
    }
  }
}

struct Nodes;

impl LayoutNodes for Nodes {
  fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
    tree.get_as::<Node>(node).map(|node| node as &dyn Layoutable)
  }
}

/// No text: these nodes shape none.
struct NoText;

impl TextEngine for NoText {
  fn paragraph(&mut self, _text: &str, _style: &TextStyle, _paragraph: &ParagraphStyle, _limits: TextLimits) -> Paragraph {
    Paragraph::default()
  }
}

/// A column of rows, each of ten leaves side by side: `leaves` leaves under `leaves / 10` rows
/// under one root. Returns every node, the root first.
fn build(tree: &mut UiTree, leaves: usize) -> Vec<NodeId> {
  let root = tree.alloc(Box::new(Node::Stack { across: false }));
  let mut all = vec![root];
  for _ in 0..leaves / 10 {
    let row = tree.alloc(Box::new(Node::Stack { across: true }));
    tree.add_child(TreeKind::Visual, root, row);
    all.push(row);
    for _ in 0..10 {
      let leaf = tree.alloc(Box::new(Node::Leaf(Cell::new(Size::new(20.0, 10.0)))));
      tree.add_child(TreeKind::Visual, row, leaf);
      all.push(leaf);
    }
  }
  all
}

fn nodes(leaves: usize) -> u32 {
  (1 + leaves / 10 + leaves) as u32
}

fn run(layout: &mut LayoutManager, tree: &mut UiTree, root: NodeId) -> LayoutStats {
  layout.run(tree, &Nodes, root, WINDOW, GRID, &mut NoText)
}

// The first layout of a tree, by a manager that knows nothing about it.
fn cold(c: &mut Criterion) {
  let mut group = c.benchmark_group("floor/cold");
  for leaves in [1000, 10_000] {
    let mut tree = UiTree::new();
    let root = build(&mut tree, leaves)[0];
    assert_eq!(run(&mut LayoutManager::default(), &mut tree, root).measured, nodes(leaves));
    group.bench_function(BenchmarkId::from_parameter(leaves), |b| {
      b.iter(|| run(&mut LayoutManager::default(), &mut tree, root));
    });
  }
  group.finish();
}

// A pass over a tree that hasn't changed since the last one.
fn unchanged(c: &mut Criterion) {
  let mut group = c.benchmark_group("floor/unchanged");
  for leaves in [1000, 10_000] {
    let mut tree = UiTree::new();
    let root = build(&mut tree, leaves)[0];
    let mut layout = LayoutManager::default();
    run(&mut layout, &mut tree, root);
    assert_eq!(run(&mut layout, &mut tree, root).measured, 0);
    group.bench_function(BenchmarkId::from_parameter(leaves), |b| {
      b.iter(|| run(&mut layout, &mut tree, root));
    });
  }
  group.finish();
}

// Every node changed, so every node is measured again; marking them is part of the time.
fn relayout(c: &mut Criterion) {
  let mut group = c.benchmark_group("floor/relayout");
  for leaves in [1000, 10_000] {
    let mut tree = UiTree::new();
    let all = build(&mut tree, leaves);
    let root = all[0];
    let mut layout = LayoutManager::default();
    run(&mut layout, &mut tree, root);
    let pass = |layout: &mut LayoutManager, tree: &mut UiTree| {
      for node in &all {
        LayoutManager::invalidate_measure(tree, *node);
      }
      run(layout, tree, root)
    };
    assert_eq!(pass(&mut layout, &mut tree).measured, nodes(leaves));
    group.bench_function(BenchmarkId::from_parameter(leaves), |b| {
      b.iter(|| pass(&mut layout, &mut tree));
    });
  }
  group.finish();
}

// One leaf grows or shrinks: it, its row and the root are measured again, and nothing else.
fn one_leaf(c: &mut Criterion) {
  let mut group = c.benchmark_group("floor/one_leaf");
  for leaves in [1000, 10_000] {
    let mut tree = UiTree::new();
    let all = build(&mut tree, leaves);
    let (root, leaf) = (all[0], *all.last().expect("a leaf"));
    let mut layout = LayoutManager::default();
    run(&mut layout, &mut tree, root);
    let pass = |layout: &mut LayoutManager, tree: &mut UiTree| {
      let Some(Node::Leaf(size)) = tree.get_as::<Node>(leaf) else {
        unreachable!("the last node is a leaf")
      };
      size.set(Size::new(size.get().width, 30.0 - size.get().height));
      LayoutManager::invalidate_measure(tree, leaf);
      run(layout, tree, root)
    };
    assert_eq!(pass(&mut layout, &mut tree).measured, 3);
    group.bench_function(BenchmarkId::from_parameter(leaves), |b| {
      b.iter(|| pass(&mut layout, &mut tree));
    });
  }
  group.finish();
}

// A pass over a laid-out tree beside a detached subtree whose nodes are all queued dirty, as a
// cached page or a node not yet attached is: nothing under the root is measured.
fn detached(c: &mut Criterion) {
  let mut group = c.benchmark_group("floor/detached");
  for leaves in [1000, 10_000] {
    let mut tree = UiTree::new();
    let root = build(&mut tree, 1000)[0];
    let mut layout = LayoutManager::default();
    run(&mut layout, &mut tree, root);
    build(&mut tree, leaves);
    assert_eq!(run(&mut layout, &mut tree, root).measured, 0);
    group.bench_function(BenchmarkId::from_parameter(leaves), |b| {
      b.iter(|| run(&mut layout, &mut tree, root));
    });
  }
  group.finish();
}

criterion_group!(benches, cold, unchanged, relayout, one_leaf, detached);
criterion_main!(benches);
