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
// Besides text, a property's access takes a node as the value (`set_node`) and a staged object
// (`set_object`), which a property not taken from objects takes out of the tree and refuses.
//
// Compare against main as the core benches do:
//
//   cargo bench -p artic-controls --bench loader -- --save-baseline main
//   cargo bench -p artic-controls --bench loader -- --baseline main
//
// Each bench checks once, before it's timed, that both ways write the value they say they do.

use artic_controls::{Border, TextBlock};
use artic_core::property::{Precedence, Property};
use artic_core::tree::{NodeId, UiTree};
use artic_layout::HorizontalAlignment;
use artic_layout::geometry::Thickness;
use artic_macros::UiType;
use artic_registry::{ContentInfo, FromUiObject, PropertyAccess, TypeInfo, UiType as _, ValueError};
use artic_render::{Brush, Color, GradientStop, LinearGradientBrush, SolidColorBrush};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

/// Two texts written in turn, so no write is skipped as setting the value already there.
const PADDINGS: [&str; 2] = ["4,8", "8,4"];
const ALIGNMENTS: [&str; 2] = ["Left", "Right"];
const COLORS: [&str; 2] = ["#3B82F6", "SteelBlue"];
const OPACITIES: [&str; 2] = ["0.5", "0.25"];
/// Which of two child nodes each write sets, named as text so they go through `compare`.
const CHILDREN: [&str; 2] = ["first", "second"];

/// A value holding its stops in a `Vec`, a collection no brush uses: an app's own value type
/// can.
#[derive(Debug, Clone, PartialEq, Default, UiType)]
#[ui(value)]
struct Ramp {
  #[ui(content)]
  stops: Vec<GradientStop>,
}

/// A node type with a property that holds a node, which no control has: a UI language sets it
/// from an element written inside.
#[derive(Debug, UiType)]
struct Holder {
  #[ui]
  child: Property<Option<NodeId>>,
}

/// The function that creates a node of `type_info` from its defaults.
fn constructor(type_info: &'static TypeInfo) -> fn(&mut UiTree) -> NodeId {
  type_info.new.expect("the type is created from its default")
}

/// The function that adds a child to `type_info`'s collection content.
fn add_child_of(type_info: &'static TypeInfo) -> fn(&mut UiTree, NodeId, NodeId) -> Result<(), ValueError> {
  let Some(ContentInfo::Collection { add_child, .. }) = type_info.content else {
    panic!("the type's content is a collection");
  };
  add_child
}

/// A tree holding one control created from its descriptor, and the control's node.
fn one(type_info: &'static TypeInfo) -> (UiTree, NodeId) {
  let mut tree = UiTree::new();
  let node = constructor(type_info)(&mut tree);
  (tree, node)
}

/// Times `access` and `typed` in one group named `name`, each writing the two `texts` in turn.
fn compare(
  c: &mut Criterion,
  name: &str,
  tree: &mut UiTree,
  texts: [&str; 2],
  mut access: impl FnMut(&mut UiTree, &str),
  mut typed: impl FnMut(&mut UiTree, &str),
) {
  let mut group = c.benchmark_group(name);
  let mut turn = 0;
  group.bench_function("access", |b| {
    b.iter(|| {
      turn ^= 1;
      access(tree, black_box(texts[turn]));
    });
  });
  group.bench_function("typed", |b| {
    b.iter(|| {
      turn ^= 1;
      typed(tree, black_box(texts[turn]));
    });
  });
  group.finish();
}

/// The loader's access to `property` of `type_info`, an own property or a group's.
fn access(type_info: &'static TypeInfo, property: &str) -> &'static PropertyAccess {
  let info = type_info.property(property).expect("the property is registered").info;
  info.access.expect("the access feature is on")
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

  let loaded = |tree: &mut UiTree, text: &str| {
    (padding.set_text)(tree, node, text, Precedence::Local).expect("a Thickness");
  };
  compare(c, "loader/own_property", &mut tree, PADDINGS, loaded, typed);
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

  let loaded = |tree: &mut UiTree, text: &str| {
    (alignment.set_text)(tree, node, text, Precedence::Local).expect("an alignment");
  };
  compare(c, "loader/group_property", &mut tree, ALIGNMENTS, loaded, typed);
}

/// `Border.Background` from a brush written as an element: the brush is staged in the tree, its
/// color set from text, and the staged brush moved into the property.
fn object_property(c: &mut Criterion) {
  let (mut tree, node) = one(Border::TYPE_INFO);
  let new_brush = constructor(SolidColorBrush::TYPE_INFO);
  let color = access(SolidColorBrush::TYPE_INFO, "Color");
  let background = access(Border::TYPE_INFO, "Background");
  let staged = |tree: &mut UiTree, text: &str| {
    let brush = new_brush(tree);
    (color.set_text)(tree, brush, text, Precedence::Local).expect("a color");
    (background.set_object)(tree, node, brush, Precedence::Local).expect("a brush");
  };
  let typed = |tree: &mut UiTree, text: &str| {
    let value: Color = text.parse().expect("a color");
    tree
      .get_as::<Border>(node)
      .expect("a Border node")
      .background
      .set_at(Precedence::Local, Some(Brush::from(SolidColorBrush::new(value))));
  };
  let read = |tree: &UiTree| tree.get_as::<Border>(node).and_then(|border| border.background.get());
  staged(&mut tree, COLORS[0]);
  assert_eq!(read(&tree), Some(Brush::from(Color::rgb(0x3B, 0x82, 0xF6))));
  typed(&mut tree, COLORS[1]);
  assert_eq!(read(&tree), Some(Brush::from(Color::STEEL_BLUE)));

  compare(c, "loader/object_property", &mut tree, COLORS, staged, typed);
}

/// `Border.Background` from a gradient written as an element with two stops: each stop is staged,
/// given its offset and color from text, and taken into the gradient's stops, which grow by one for
/// each; then the staged gradient is moved into the property.
fn gradient_property(c: &mut Criterion) {
  let (mut tree, node) = one(Border::TYPE_INFO);
  let new_gradient = constructor(LinearGradientBrush::TYPE_INFO);
  let new_stop = constructor(GradientStop::TYPE_INFO);
  let add_child = add_child_of(LinearGradientBrush::TYPE_INFO);
  let offset = access(GradientStop::TYPE_INFO, "Offset");
  let color = access(GradientStop::TYPE_INFO, "Color");
  let background = access(Border::TYPE_INFO, "Background");
  let staged = |tree: &mut UiTree, text: &str| {
    let gradient = new_gradient(tree);
    for (at, stop_color) in [("0", "White"), ("1", text)] {
      let stop = new_stop(tree);
      (offset.set_text)(tree, stop, at, Precedence::Local).expect("a number");
      (color.set_text)(tree, stop, stop_color, Precedence::Local).expect("a color");
      add_child(tree, gradient, stop).expect("a stop");
    }
    (background.set_object)(tree, node, gradient, Precedence::Local).expect("a brush");
  };
  let typed = |tree: &mut UiTree, text: &str| {
    let stop =
      |at: &str, stop_color: &str| GradientStop::new(at.parse().expect("a number"), stop_color.parse().expect("a color"));
    let gradient = LinearGradientBrush {
      stops: [stop("0", "White"), stop("1", text)].into(),
      ..LinearGradientBrush::default()
    };
    tree
      .get_as::<Border>(node)
      .expect("a Border node")
      .background
      .set_at(Precedence::Local, Some(Brush::from(gradient)));
  };
  let read = |tree: &UiTree| tree.get_as::<Border>(node).and_then(|border| border.background.get());
  let expected = |end: Color| {
    Brush::from(LinearGradientBrush {
      stops: [GradientStop::new(0.0, Color::WHITE), GradientStop::new(1.0, end)].into(),
      ..LinearGradientBrush::default()
    })
  };
  staged(&mut tree, COLORS[0]);
  assert_eq!(read(&tree), Some(expected(Color::rgb(0x3B, 0x82, 0xF6))));
  typed(&mut tree, COLORS[1]);
  assert_eq!(read(&tree), Some(expected(Color::STEEL_BLUE)));

  compare(c, "loader/gradient_property", &mut tree, COLORS, staged, typed);
}

/// A `Vec` of values filled from staged stops: two stops staged, each given its color from text
/// and taken into the ramp, which is then taken out of the tree as a `Ramp`.
fn value_collection(c: &mut Criterion) {
  let mut tree = UiTree::new();
  let new_ramp = constructor(Ramp::TYPE_INFO);
  let new_stop = constructor(GradientStop::TYPE_INFO);
  let add_child = add_child_of(Ramp::TYPE_INFO);
  let color = access(GradientStop::TYPE_INFO, "Color");
  let staged = |tree: &mut UiTree, text: &str| -> Ramp {
    let ramp = new_ramp(tree);
    for stop_color in ["White", text] {
      let stop = new_stop(tree);
      (color.set_text)(tree, stop, stop_color, Precedence::Local).expect("a color");
      add_child(tree, ramp, stop).expect("a stop");
    }
    Ramp::from_object(tree, ramp).expect("a Ramp")
  };
  let typed = |text: &str| Ramp {
    stops: ["White", text]
      .map(|stop_color| GradientStop {
        color: stop_color.parse().expect("a color"),
        ..GradientStop::default()
      })
      .into(),
  };
  assert_eq!(staged(&mut tree, COLORS[0]), typed(COLORS[0]));

  compare(
    c,
    "loader/value_collection",
    &mut tree,
    COLORS,
    |tree, text| {
      black_box(staged(tree, text));
    },
    |_, text| {
      black_box(typed(text));
    },
  );
}

/// A gradient's `Opacity` set from text and cleared again on the staged gradient: clearing a
/// value's field resets it to the field's value in the type's default.
fn value_field_clear(c: &mut Criterion) {
  let (mut tree, gradient) = one(LinearGradientBrush::TYPE_INFO);
  let opacity = access(LinearGradientBrush::TYPE_INFO, "Opacity");
  let read = |tree: &UiTree| tree.get_as::<LinearGradientBrush>(gradient).map(|brush| brush.opacity);
  let loaded = |tree: &mut UiTree, text: &str| {
    (opacity.set_text)(tree, gradient, text, Precedence::Local).expect("a number");
    (opacity.clear)(tree, gradient, Precedence::Local).expect("a gradient");
  };
  let typed = |tree: &mut UiTree, text: &str| {
    let brush = tree
      .get_as_mut::<LinearGradientBrush>(gradient)
      .expect("a LinearGradientBrush node");
    brush.opacity = text.parse().expect("a number");
    brush.opacity = LinearGradientBrush::default().opacity;
  };
  (opacity.set_text)(&mut tree, gradient, OPACITIES[0], Precedence::Local).expect("a number");
  assert_eq!(read(&tree), Some(0.5));
  loaded(&mut tree, OPACITIES[1]);
  assert_eq!(read(&tree), Some(1.0));
  typed(&mut tree, OPACITIES[0]);
  assert_eq!(read(&tree), Some(1.0));

  compare(c, "loader/value_field_clear", &mut tree, OPACITIES, loaded, typed);
}

/// `Holder.Child` set to a node: the loader's `set_node` against setting the property directly.
fn node_property(c: &mut Criterion) {
  let (mut tree, node) = one(Holder::TYPE_INFO);
  let children = [
    tree.alloc(Box::new(Holder::default())),
    tree.alloc(Box::new(Holder::default())),
  ];
  let pick = move |text: &str| children[usize::from(text == CHILDREN[1])];
  let child = access(Holder::TYPE_INFO, "Child");
  let read = |tree: &UiTree| tree.get_as::<Holder>(node).and_then(|holder| holder.child.get());
  let loaded = |tree: &mut UiTree, text: &str| {
    (child.set_node)(tree, node, pick(text), Precedence::Local).expect("a node property");
  };
  let typed = |tree: &mut UiTree, text: &str| {
    tree
      .get_as::<Holder>(node)
      .expect("a Holder node")
      .child
      .set_at(Precedence::Local, Some(pick(text)));
  };
  loaded(&mut tree, CHILDREN[0]);
  assert_eq!(read(&tree), Some(children[0]));
  typed(&mut tree, CHILDREN[1]);
  assert_eq!(read(&tree), Some(children[1]));

  compare(c, "loader/node_property", &mut tree, CHILDREN, loaded, typed);
}

/// `Border.Padding` given a staged `Ramp`: a property not taken from objects takes the staged
/// object out of the tree and refuses it. The `typed` side stages the ramp and takes it out, so the
/// gap is the access's check and its error.
fn refused_object(c: &mut Criterion) {
  let (mut tree, node) = one(Border::TYPE_INFO);
  let new_ramp = constructor(Ramp::TYPE_INFO);
  let padding = access(Border::TYPE_INFO, "Padding");
  let loaded = |tree: &mut UiTree, _: &str| {
    let ramp = new_ramp(tree);
    let refused = (padding.set_object)(tree, node, ramp, Precedence::Local);
    black_box(refused).expect_err("a Thickness isn't taken from an object");
  };
  let typed = |tree: &mut UiTree, _: &str| {
    let ramp = new_ramp(tree);
    black_box(tree.take(ramp));
  };
  let staged = new_ramp(&mut tree);
  assert_eq!(
    (padding.set_object)(&mut tree, node, staged, Precedence::Local),
    Err(ValueError::WrongType {
      expected: "Thickness",
      found: "an object",
    })
  );
  assert!(!tree.contains(staged), "the refused object is taken out of the tree");

  compare(c, "loader/refused_object", &mut tree, PADDINGS, loaded, typed);
}

criterion_group!(
  benches,
  own_property,
  group_property,
  object_property,
  gradient_property,
  value_collection,
  value_field_clear,
  node_property,
  refused_object
);
criterion_main!(benches);
