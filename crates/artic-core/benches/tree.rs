// How a property change reaches the nodes that inherit it: a write to a root's property that a
// whole tree of followers inherits, and building such a tree.
//
// Compare against main as the reactive benches do:
//
//   cargo bench -p artic-core --bench tree -- --save-baseline main
//   cargo bench -p artic-core --bench tree -- --baseline main
//
// The write bench checks, inside its closure, that its writes reached every node: under CodSpeed
// each bench runs in a process of its own.

use artic_core::Object;
use artic_core::property::{Affects, Inheritance, Property, PropertyKey, PropertyVisitor, inherit};
use artic_core::tree::{NodeId, TreeKind, UiTree};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

static ENABLED: PropertyKey<bool> = PropertyKey::new("Enabled", Affects::RENDER, Inheritance::Combine(inherit::and));

/// A node whose enabled flag combines with its ancestors', as an element's does.
#[derive(Debug)]
struct Toggle {
  enabled: Property<bool>,
}

impl Object for Toggle {
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
    visitor.visit(&self.enabled);
  }
}

/// A root and `count` nodes under it, in rows of ten, each row under the one before, so the tree
/// is both wide and deep. Returns the root's flag and every node.
fn build(tree: &mut UiTree, count: usize) -> (Property<bool>, Vec<NodeId>) {
  let enabled = Property::new(&ENABLED, true);
  let mut nodes = vec![tree.alloc(Box::new(Toggle { enabled }))];
  for index in 0..count {
    let node = tree.alloc(Box::new(Toggle {
      enabled: Property::new(&ENABLED, true),
    }));
    let parent = nodes[index / 10];
    tree.add_child(TreeKind::Logical, parent, node);
    nodes.push(node);
  }
  (enabled, nodes)
}

// A write to the root's flag, which every node inherits: each is marked render-dirty.
fn root_write(c: &mut Criterion) {
  let mut group = c.benchmark_group("inherit/root_write");
  for count in [100, 1000] {
    let mut tree = UiTree::new();
    let (enabled, nodes) = build(&mut tree, count);
    for node in &nodes {
      tree.clear_dirty(*node, Affects::LAYOUT | Affects::RENDER);
    }
    let mut value = true;
    group.bench_function(BenchmarkId::from_parameter(count), |b| {
      b.iter(|| {
        value = !value;
        enabled.set(value);
      });
      assert!(
        nodes.iter().all(|node| tree.dirty(*node).contains(Affects::RENDER)),
        "a node missed the change"
      );
    });
  }
  group.finish();
}

// Building a tree of `count` inheriting nodes under a root.
fn build_tree(c: &mut Criterion) {
  let mut group = c.benchmark_group("inherit/build");
  for count in [100, 1000] {
    group.bench_function(BenchmarkId::from_parameter(count), |b| {
      b.iter(|| {
        let mut tree = UiTree::new();
        let (_, nodes) = build(&mut tree, count);
        for node in nodes.into_iter().rev() {
          tree.free(node);
        }
      });
    });
  }
  group.finish();
}

criterion_group!(tree, root_write, build_tree);
criterion_main!(tree);
