// What a UI language's runtime loader pays to write one property from text through the property's
// access: function pointers whose bodies parse the text and find the property on the node by its
// key. The `typed` side parses the same text and sets the field directly, so the gap is the lookup
// and the indirection. A compiled view pays neither, nor the parse: its values are Rust
// expressions written at build time. The access is resolved once, outside the timed loop.
//
// Finding a property by key visits every property the node holds, whatever the property's place,
// so the cost follows the node's property count: `Border.Padding` is a property of the type itself,
// `TextBlock.HorizontalAlignment` one of the layout group every control holds.
//
// Compare against main as the core benches do:
//
//   cargo bench -p artic-controls --bench loader -- --save-baseline main
//   cargo bench -p artic-controls --bench loader -- --baseline main
//
// Each bench checks once, before it's timed, that both ways write the value they say they do.

use artic_controls::{Border, TextBlock};
use artic_core::property::Precedence;
use artic_core::tree::{NodeId, UiTree};
use artic_layout::HorizontalAlignment;
use artic_layout::geometry::Thickness;
use artic_registry::{PropertyAccess, TypeInfo, UiType};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

/// Two texts written in turn, so no write is skipped as setting the value already there.
const PADDINGS: [&str; 2] = ["4,8", "8,4"];
const ALIGNMENTS: [&str; 2] = ["Left", "Right"];

/// A tree holding one control created from its descriptor, and the control's node.
fn one(type_info: &'static TypeInfo) -> (UiTree, NodeId) {
  let mut tree = UiTree::new();
  let node = (type_info.new.expect("the control has a constructor"))(&mut tree);
  (tree, node)
}

/// The loader's access to `property` of `type_info`, an own property or a group's.
fn access(type_info: &'static TypeInfo, property: &str) -> &'static PropertyAccess {
  type_info.property(property).expect("the property is registered").info.access
}

/// `Border.Padding`, a property of the type itself.
fn own_property(c: &mut Criterion) {
  let (mut tree, node) = one(Border::TYPE_INFO);
  let padding = access(Border::TYPE_INFO, "Padding");
  let typed = |tree: &mut UiTree, text: &str| {
    let value: Thickness = text.parse().expect("a Thickness");
    tree
      .get_as::<Border>(node)
      .expect("a Border node")
      .padding
      .set_at(Precedence::Local, value);
  };
  (padding.set_text)(&mut tree, node, PADDINGS[0], Precedence::Local).expect("a Thickness");
  assert_eq!(
    tree.get_as::<Border>(node).map(|border| border.padding.get()),
    Some(Thickness::new(4.0, 8.0, 4.0, 8.0))
  );
  typed(&mut tree, PADDINGS[1]);
  assert_eq!(
    tree.get_as::<Border>(node).map(|border| border.padding.get()),
    Some(Thickness::new(8.0, 4.0, 8.0, 4.0))
  );

  let mut group = c.benchmark_group("loader/own_property");
  let mut turn = 0;
  group.bench_function("access", |b| {
    b.iter(|| {
      turn ^= 1;
      (padding.set_text)(&mut tree, node, black_box(PADDINGS[turn]), Precedence::Local).expect("a Thickness");
    });
  });
  group.bench_function("typed", |b| {
    b.iter(|| {
      turn ^= 1;
      typed(&mut tree, black_box(PADDINGS[turn]));
    });
  });
  group.finish();
}

/// `TextBlock.HorizontalAlignment`, a property of the layout group every control holds.
fn group_property(c: &mut Criterion) {
  let (mut tree, node) = one(TextBlock::TYPE_INFO);
  let alignment = access(TextBlock::TYPE_INFO, "HorizontalAlignment");
  let typed = |tree: &mut UiTree, text: &str| {
    let value: HorizontalAlignment = text.parse().expect("an alignment");
    tree
      .get_as::<TextBlock>(node)
      .expect("a TextBlock node")
      .layout
      .horizontal_alignment
      .set_at(Precedence::Local, value);
  };
  let read = |tree: &UiTree| {
    tree
      .get_as::<TextBlock>(node)
      .map(|block| block.layout.horizontal_alignment.get())
  };
  (alignment.set_text)(&mut tree, node, ALIGNMENTS[0], Precedence::Local).expect("an alignment");
  assert_eq!(read(&tree), Some(HorizontalAlignment::Left));
  typed(&mut tree, ALIGNMENTS[1]);
  assert_eq!(read(&tree), Some(HorizontalAlignment::Right));

  let mut group = c.benchmark_group("loader/group_property");
  let mut turn = 0;
  group.bench_function("access", |b| {
    b.iter(|| {
      turn ^= 1;
      (alignment.set_text)(&mut tree, node, black_box(ALIGNMENTS[turn]), Precedence::Local).expect("an alignment");
    });
  });
  group.bench_function("typed", |b| {
    b.iter(|| {
      turn ^= 1;
      typed(&mut tree, black_box(ALIGNMENTS[turn]));
    });
  });
  group.finish();
}

criterion_group!(benches, own_property, group_property);
criterion_main!(benches);
