// What the accessibility pass costs a frame through real controls, while a screen reader reads the
// window: a StackPanel of rows, each a StackPanel of ten Borders holding a TextBlock, laid out once.
//
// The whole tree is what a screen reader gets when it starts; a frame with nothing it sees changed
// walks the tree and compares child lists; a frame where one text got a new layout result describes
// that one node again.
//
// Compare against main as the core benches do:
//
//   cargo bench -p artic --bench accessibility -- --save-baseline main
//   cargo bench -p artic --bench accessibility -- --baseline main
//
// Each bench checks once, before it's timed, that the pass hands over what it says it does.

use artic::controls::{Border, StackPanel, TextBlock};
use artic::core::property::Affects;
use artic::core::tree::{NodeId, TreeKind, UiTree};
use artic::element::{ElementNodes, ElementTypes};
use artic::layout::geometry::Size;
use artic::layout::{LayoutManager, Orientation, PixelGrid};
use artic::render::Compositor;
use artic_platform::AccessTree;
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use std::collections::HashSet;
use std::hint::black_box;

const LEAVES: [usize; 2] = [100, 1000];

/// A column of rows, each of ten 20 by 10 borders holding a text: `leaves` borders under
/// `leaves / 10` rows under one root, laid out in 800 by 600 and composed once, as a frame leaves
/// it: no node render-dirty. Returns the tree, the root, the texts, the layout and the element
/// table.
fn laid_out(leaves: usize) -> (UiTree, NodeId, Vec<NodeId>, LayoutManager, ElementTypes) {
  let mut tree = UiTree::new();
  let root = tree.alloc(Box::new(StackPanel::default()));
  let mut all = vec![root];
  let mut texts = Vec::with_capacity(leaves);
  for _ in 0..leaves / 10 {
    let row = StackPanel::default();
    row.orientation.set(Orientation::Horizontal);
    let row = tree.alloc(Box::new(row));
    tree.add_child(TreeKind::Visual, root, row);
    all.push(row);
    for _ in 0..10 {
      let border = Border::default();
      border.layout.width.set(Some(20.0));
      border.layout.height.set(Some(10.0));
      let border = tree.alloc(Box::new(border));
      tree.add_child(TreeKind::Visual, row, border);
      let text = TextBlock::default();
      text.text.set("cell".to_owned());
      let text = tree.alloc(Box::new(text));
      tree.add_child(TreeKind::Visual, border, text);
      all.extend([border, text]);
      texts.push(text);
    }
  }
  let types = ElementTypes::from_link_time();
  let mut layout = LayoutManager::default();
  let grid = PixelGrid { scale: 1.0, snap: true };
  let mut engine = artic_text::Engine::without_fonts();
  layout.run(&mut tree, &types, root, Size::new(800.0, 600.0), grid, &mut engine);
  let mut elements = ElementNodes {
    types: &types,
    layout: &layout,
  };
  Compositor::default().compose(&mut tree, root, &mut elements);
  assert!(
    all.iter().all(|node| !tree.dirty(*node).contains(Affects::RENDER)),
    "composition leaves no node render-dirty"
  );
  (tree, root, texts, layout, types)
}

/// The nodes under a root with `leaves` borders: the root, the rows, the borders and their texts.
fn nodes(leaves: usize) -> usize {
  1 + leaves / 10 + 2 * leaves
}

// The whole tree, as a screen reader that starts reading gets it.
fn full(c: &mut Criterion) {
  let mut group = c.benchmark_group("accessibility/full");
  for leaves in LEAVES {
    let (tree, root, _, layout, types) = laid_out(leaves);
    let elements = ElementNodes {
      types: &types,
      layout: &layout,
    };
    let stale = HashSet::new();
    let sent = AccessTree::new()
      .update(&tree, root, &elements, &stale, 1.0)
      .map(|update| update.nodes.len());
    assert_eq!(sent, Some(nodes(leaves)), "the whole tree");
    group.bench_function(BenchmarkId::from_parameter(leaves), |b| {
      b.iter_batched_ref(
        AccessTree::new,
        |access| black_box(access.update(&tree, root, &elements, &stale, 1.0)),
        BatchSize::SmallInput,
      );
    });
  }
  group.finish();
}

// A frame where nothing a screen reader sees changed: a walk that compares child lists.
fn unchanged(c: &mut Criterion) {
  let mut group = c.benchmark_group("accessibility/unchanged");
  for leaves in LEAVES {
    let (tree, root, _, layout, types) = laid_out(leaves);
    let elements = ElementNodes {
      types: &types,
      layout: &layout,
    };
    let stale = HashSet::new();
    let mut access = AccessTree::new();
    access.update(&tree, root, &elements, &stale, 1.0);
    assert!(
      access.update(&tree, root, &elements, &stale, 1.0).is_none(),
      "nothing to send"
    );
    group.bench_function(BenchmarkId::from_parameter(leaves), |b| {
      b.iter(|| black_box(access.update(&tree, root, &elements, &stale, 1.0)));
    });
  }
  group.finish();
}

// A frame where layout gave one text a new result: that one node is described again, and comes out
// the same.
fn one_text(c: &mut Criterion) {
  let mut group = c.benchmark_group("accessibility/one_text");
  for leaves in LEAVES {
    let (tree, root, texts, layout, types) = laid_out(leaves);
    let elements = ElementNodes {
      types: &types,
      layout: &layout,
    };
    let stale = HashSet::from([texts[texts.len() / 2]]);
    let mut access = AccessTree::new();
    access.update(&tree, root, &elements, &HashSet::new(), 1.0);
    assert!(
      access.update(&tree, root, &elements, &stale, 1.0).is_none(),
      "the same description"
    );
    group.bench_function(BenchmarkId::from_parameter(leaves), |b| {
      b.iter(|| black_box(access.update(&tree, root, &elements, &stale, 1.0)));
    });
  }
  group.finish();
}

criterion_group!(benches, full, unchanged, one_text);
criterion_main!(benches);
