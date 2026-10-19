// Integration tests for `#[derive(UiType)]`: derived types reach the registry with the
// descriptors their fields describe, and their generated `Default` and `Object` impls work.

use artic_core::event::Event;
use artic_core::property::{Affects, Inheritance, Precedence, Property, PropertyKey};
use artic_core::tree::{Handle, NodeId, TreeKind, UiTree};
use artic_core::{RustExpr, ToRust};
use artic_element::{Child, Children};
use artic_macros::{UiProperties, UiType};
use artic_registry::{
  AttachedTarget, ContentInfo, OverrideDefaults, PropertyGroup, PropertyRef, Registry, TypeInfo, UiCollection, UiType,
  ValueError, Whitespace,
};
use pretty_assertions::assert_eq;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::str::FromStr;

// Placeholders for value types that later issues add: geometry (#22), brushes (#35), pointer input
// and the property groups (#42). They only need to parse from text and have defaults.

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Thickness(f32);

impl Thickness {
  const ZERO: Thickness = Thickness(0.0);
}

impl FromStr for Thickness {
  type Err = std::num::ParseFloatError;

  fn from_str(text: &str) -> Result<Self, Self::Err> {
    text.parse().map(Thickness)
  }
}

impl ToRust for Thickness {
  fn to_rust(&self, out: &mut RustExpr) {
    out.call(env!("CARGO_CRATE_NAME"), "Thickness", &[&self.0]);
  }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct CornerRadius(f32);

impl FromStr for CornerRadius {
  type Err = std::num::ParseFloatError;

  fn from_str(text: &str) -> Result<Self, Self::Err> {
    text.parse().map(CornerRadius)
  }
}

impl ToRust for CornerRadius {
  fn to_rust(&self, out: &mut RustExpr) {
    out.call(env!("CARGO_CRATE_NAME"), "CornerRadius", &[&self.0]);
  }
}

#[derive(Debug, Clone, PartialEq)]
struct Brush(String);

impl FromStr for Brush {
  type Err = std::convert::Infallible;

  fn from_str(text: &str) -> Result<Self, Self::Err> {
    Ok(Brush(text.to_owned()))
  }
}

impl ToRust for Brush {
  fn to_rust(&self, out: &mut RustExpr) {
    out.call(env!("CARGO_CRATE_NAME"), "Brush", &[&self.0]);
  }
}

#[derive(Debug)]
struct PointerEvent;

#[derive(Debug, Default)]
struct Layout;

/// The `Border` from the spec's §3, with `affects` added.
#[derive(Debug, UiType)]
struct Border {
  _layout: Layout,
  #[ui(content)]
  child: Property<Option<NodeId>>,
  #[ui(default = Thickness::ZERO, affects = layout)]
  padding: Property<Thickness>,
  #[ui(affects = render)]
  background: Property<Option<Brush>>,
  #[ui(affects = render)]
  corner_radius: Property<CornerRadius>,
  #[ui]
  pointer_pressed: Event<PointerEvent>,
}

/// A collection that keeps the children pushed into it, as a value collection keeps its values.
#[derive(Debug, Default)]
struct Items(Vec<NodeId>);

impl UiCollection for Items {
  fn push_child(
    tree: &mut UiTree,
    owner: NodeId,
    child: NodeId,
    field: for<'a> fn(&'a mut UiTree, NodeId) -> Option<&'a mut Self>,
  ) -> Result<(), ValueError> {
    field(tree, owner).ok_or(ValueError::WrongNode)?.0.push(child);
    Ok(())
  }
}

#[derive(Debug, UiType)]
struct StackPanel {
  #[ui(content)]
  children: Items,
}

#[derive(Debug, UiType)]
#[ui(whitespace = "significant")]
struct Run {
  #[ui(content)]
  text: Property<String>,
}

static FONT_SIZE: PropertyKey<f32> = PropertyKey::new("FontSize", Affects::LAYOUT, Inheritance::Replace);

#[derive(Debug, UiType)]
struct Window {
  #[ui(key = FONT_SIZE, default = 14.0)]
  font_size: Property<f32>,
}

#[derive(Debug, UiType)]
struct TextBlock {
  #[ui(key = FONT_SIZE)]
  font_size: Property<f32>,
}

/// Looks `name` up in the default namespace of a registry built from every linked type.
fn lookup(name: &str) -> &'static TypeInfo {
  let registry = Registry::from_link_time().expect("no duplicates");
  registry.type_by_name("derive", name).found().expect("the type is registered")
}

// The spec's `Border` is registered with its four properties, its event and its content field.
#[test]
fn border_is_registered_with_its_members() {
  let border = lookup("Border");

  let properties: Vec<(&str, &str, Affects)> = border
    .properties
    .iter()
    .map(|property| (property.name, property.field, property.affects))
    .collect();
  assert_eq!(
    properties,
    [
      ("Child", "child", Affects::NONE),
      ("Padding", "padding", Affects::LAYOUT),
      ("Background", "background", Affects::RENDER),
      ("CornerRadius", "corner_radius", Affects::RENDER),
    ]
  );
  assert_eq!(
    border.events.iter().map(|event| event.name).collect::<Vec<_>>(),
    ["PointerPressed"]
  );
  assert!(!border.is_collection);
  assert!(border.rust_path.ends_with("::Border"));
  assert!(std::ptr::eq(border, Border::TYPE_INFO));
}

// The descriptor's constructor creates a node of the type with its default values.
#[test]
fn new_creates_a_node_with_defaults() {
  let mut tree = UiTree::new();

  let node = (lookup("Border").new)(&mut tree);

  let border = tree.get_as::<Border>(node).expect("a Border node");
  assert_eq!(border.child.get(), None);
  assert_eq!(border.background.get(), None);
  assert_eq!(border.corner_radius.get(), CornerRadius(0.0));
  let _ = &border.pointer_pressed;
}

// `default = expr` fills the property's Default level.
#[test]
fn default_expression_sets_the_default_level() {
  let border = Border::default();

  assert_eq!(border.padding.get(), Thickness::ZERO);
  assert_eq!(border.padding.effective_source(), Precedence::Default);
  assert_eq!(Window::default().font_size.get(), 14.0, "differs from f32's own default");
}

// A type with a collection field accepts children.
#[test]
fn collection_field_makes_the_type_a_collection() {
  let panel = lookup("StackPanel");

  assert!(panel.is_collection);
  assert!(StackPanel::default().children.0.is_empty());
}

// `whitespace = "significant"` reaches the descriptor.
#[test]
fn whitespace_setting_reaches_the_descriptor() {
  assert_eq!(lookup("Run").whitespace, Whitespace::Significant);
  assert_eq!(lookup("Border").whitespace, Whitespace::Collapse);
}

// Two types that share a key inherit from each other through the logical tree, and the key's
// metadata reaches both descriptors.
#[test]
fn shared_key_inherits_across_types() {
  let mut tree = UiTree::new();
  let window = (Window::TYPE_INFO.new)(&mut tree);
  let text = (TextBlock::TYPE_INFO.new)(&mut tree);

  tree.add_child(TreeKind::Logical, window, text);
  tree.get_as::<Window>(window).expect("a Window").font_size.set(20.0);

  assert_eq!(tree.get_as::<TextBlock>(text).expect("a TextBlock").font_size.get(), 20.0);
  assert_eq!(TextBlock::TYPE_INFO.properties[0].name, "FontSize");
  assert!(TextBlock::TYPE_INFO.properties[0].inherits);
}

/// A panel owning two attached properties: `Row`, set on its children, and `Label`, set anywhere.
#[derive(Debug, UiType)]
#[ui(attached(name = "Row", ty = usize, affects = layout, target = children))]
#[ui(attached(name = "Label", ty = String, default = "unnamed".to_owned(), target = any))]
struct Grid {
  #[ui]
  columns: Property<usize>,
}

// Attached properties reach the registry, found by their owner and name, with their targets.
#[test]
fn attached_property_is_found_by_owner_and_name() {
  let registry = Registry::from_link_time().expect("no duplicates");
  let grid = lookup("Grid");

  let row = registry.attached_property(grid, "Row").expect("Grid.Row");
  let label = registry.attached_property(grid, "Label").expect("Grid.Label");

  assert_eq!((row.value_type, row.target), ("usize", AttachedTarget::ChildrenOfOwner));
  assert_eq!((label.value_type, label.target), ("String", AttachedTarget::Any));
  assert!(registry.attached_property(grid, "Columns").is_none());
}

// The generated helpers set and read an attached value on another type's node, and a local value
// overrides a style value until it's cleared.
#[test]
fn attached_helpers_set_and_read_with_precedence() {
  let mut tree = UiTree::new();
  let border = (Border::TYPE_INFO.new)(&mut tree);
  assert_eq!(Grid::row(&tree, border), 0);
  assert_eq!(Grid::label(&tree, border), "unnamed");

  Grid::set_row(&mut tree, border, 3);
  assert_eq!(Grid::row(&tree, border), 3);

  let row = tree.attached_property(border, Grid::ROW);
  row.set_at(Precedence::StyleSetter, 2);
  assert_eq!(Grid::row(&tree, border), 3, "a local value overrides a style value");
  row.clear(Precedence::Local);
  assert_eq!(Grid::row(&tree, border), 2);
}

// An attached key carries the metadata its declaration gives.
#[test]
fn attached_key_carries_its_metadata() {
  assert_eq!(Grid::ROW.key().name(), "Row");
  assert_eq!(Grid::ROW.key().affects(), Affects::LAYOUT);
  assert_eq!(Grid::LABEL.key().affects(), Affects::NONE);
  assert_eq!(Grid::LABEL.default_value(), "unnamed");
}

/// An owner of one inheriting attached property, as a platform family owner is.
#[derive(Debug, UiType)]
#[ui(attached(name = "Accent", ty = u32, affects = render | surface, target = any, inherits))]
struct Theme;

// An inheriting attached property set on an element reaches its logical descendants, and its key
// carries the surface stage.
#[test]
fn attached_property_inherits_through_the_logical_tree() {
  let mut tree = UiTree::new();
  let outer = (Border::TYPE_INFO.new)(&mut tree);
  let inner = (Border::TYPE_INFO.new)(&mut tree);
  tree.add_child(TreeKind::Logical, outer, inner);

  Theme::set_accent(&mut tree, outer, 7);

  assert_eq!(Theme::accent(&tree, inner), 7);
  assert_eq!(Theme::ACCENT.key().affects(), Affects::RENDER | Affects::SURFACE);
  assert!(Theme::ACCENT.key().inherits());
}

/// A value provider with one positional argument and an optional named one.
#[derive(Debug, UiType)]
#[ui(value_provider(positional(key)))]
struct Localize {
  #[ui]
  key: Property<String>,
  #[ui]
  fallback: Property<Option<String>>,
}

/// A value provider without arguments.
#[derive(Debug, UiType)]
#[ui(value_provider)]
struct Nothing;

// A value provider's schema lists its positional arguments in order, and every property as a
// named argument, with the names and value types of its properties.
#[test]
fn value_provider_schema_is_registered() {
  let schema = |info: &TypeInfo| {
    let extension = info.value_provider.expect("a value provider");
    let arguments = |arguments: &[artic_registry::ArgInfo]| {
      arguments
        .iter()
        .map(|argument| (argument.name, argument.value_type))
        .collect::<Vec<_>>()
    };
    (arguments(extension.positional), arguments(extension.named))
  };

  assert_eq!(
    schema(lookup("Localize")),
    (
      vec![("Key", "String")],
      vec![("Key", "String"), ("Fallback", "Option < String >")]
    )
  );
  assert_eq!(schema(lookup("Nothing")), (vec![], vec![]));
  assert!(lookup("Border").value_provider.is_none());
}

/// A property group two widgets share.
#[derive(Debug, artic_macros::UiProperties)]
struct Common {
  #[ui(affects = layout)]
  margin: Property<i32>,
  #[ui(default = true, inherits)]
  enabled: Property<bool>,
}

/// A widget that flattens `Common` and adds a property of its own.
#[derive(Debug, UiType)]
struct Widget {
  #[ui(flatten)]
  common: Common,
  #[ui]
  size: Property<i32>,
}

/// Another widget, to check that a group's inheriting property crosses types.
#[derive(Debug, UiType)]
struct Gadget {
  #[ui(flatten)]
  common: Common,
}

// A flattened group's properties are registered as the type's, with its field, and visited.
#[test]
fn flattened_group_is_registered_and_visited() {
  let info = <Widget as UiType>::TYPE_INFO;
  assert_eq!(info.groups[0].field, "common");
  assert_eq!(info.property("Margin").map(|found| found.group), Some(Some("common")));
  assert_eq!(info.property("Size").map(|found| found.group), Some(None));

  let mut visited = 0;
  artic_core::Object::visit_properties(&Widget::default(), &mut |_| visited += 1);
  assert_eq!(visited, 3);
}

// A group's inheriting property uses one key, so it inherits across the types that flatten it.
#[test]
fn group_property_inherits_across_types() {
  let mut tree = UiTree::new();
  let widget = tree.alloc(Box::new(Widget::default()));
  let gadget = tree.alloc(Box::new(Gadget::default()));
  tree.add_child(TreeKind::Logical, widget, gadget);

  tree.get_as::<Widget>(widget).unwrap().common.enabled.set(false);

  assert!(!tree.get_as::<Gadget>(gadget).unwrap().common.enabled.get());
}

/// A node whose enabled flag every ancestor must allow.
#[derive(Debug, UiType)]
struct Switch {
  #[ui(default = true, inherits = and)]
  enabled: Property<bool>,
}

// A combining property ANDs with its ancestor's: a disabled Switch disables a child Switch that
// enables itself, until the parent is enabled again.
#[test]
fn combining_property_ands_with_its_ancestor() {
  let mut tree = UiTree::new();
  let parent = tree.alloc(Box::new(Switch::default()));
  let child = tree.alloc(Box::new(Switch::default()));
  tree.add_child(TreeKind::Logical, parent, child);
  tree.get_as::<Switch>(child).unwrap().enabled.set(true);
  let enabled = |tree: &UiTree| tree.get_as::<Switch>(child).unwrap().enabled.get();

  tree.get_as::<Switch>(parent).unwrap().enabled.set(false);
  assert!(!enabled(&tree));
  tree.get_as::<Switch>(parent).unwrap().enabled.set(true);
  assert!(enabled(&tree));
}

#[derive(Debug, UiType)]
#[ui(override_defaults)]
struct Gauge {
  #[ui(default = 1)]
  low: Property<i32>,
  #[ui(default = 2)]
  high: Property<i32>,
}

impl OverrideDefaults for Gauge {
  fn override_defaults(&self) {
    self.high.set_default(self.low.get() + 10);
  }
}

// The hook runs after every field has its declared default, so it can read them.
#[test]
fn override_defaults_runs_after_field_defaults() {
  let gauge = Gauge::default();

  assert_eq!((gauge.low.get(), gauge.high.get()), (1, 11));
}

// A property marked `content` is the type's content, as its own property.
#[test]
fn content_child_on_a_field() {
  assert!(matches!(
    lookup("Border").content,
    Some(ContentInfo::Property(PropertyRef { group: None, info })) if info.name == "Child"
  ));
}

// A collection field marked `content` is the type's content, by its field name.
#[test]
fn content_children_on_a_field() {
  assert!(matches!(
    lookup("StackPanel").content,
    Some(ContentInfo::Collection { field: "children", .. })
  ));
}

#[derive(Debug, UiProperties)]
struct Labeled {
  #[ui(content)]
  label: Property<String>,
}

#[derive(Debug, UiType)]
struct Tag {
  #[ui(flatten)]
  labeled: Labeled,
}

// Content marked in a group reaches the type as that group's property.
#[test]
fn content_property_in_a_group() {
  assert!(matches!(
    <Tag as UiType>::TYPE_INFO.content,
    Some(ContentInfo::Property(PropertyRef { group: Some("labeled"), info })) if info.name == "Label"
  ));
}

// The generated `add_child` reaches the content field of the node it's given.
#[test]
fn add_child_pushes_into_the_content_field() {
  let mut tree = UiTree::new();
  let panel = (lookup("StackPanel").new)(&mut tree);
  let child = tree.alloc(Box::new(StackPanel::default()));
  let Some(ContentInfo::Collection { add_child, .. }) = lookup("StackPanel").content else {
    panic!("a collection");
  };

  add_child(&mut tree, panel, child).expect("added");

  assert_eq!(tree.get_as::<StackPanel>(panel).unwrap().children.0, [child]);
}

// Given a node of another type, `add_child` refuses before changing anything.
#[test]
fn add_child_refuses_another_type() {
  let mut tree = UiTree::new();
  let other = (lookup("Border").new)(&mut tree);
  let child = tree.alloc(Box::new(StackPanel::default()));
  let Some(ContentInfo::Collection { add_child, .. }) = lookup("StackPanel").content else {
    panic!("a collection");
  };

  assert_eq!(add_child(&mut tree, other, child), Err(ValueError::WrongNode));
}

#[derive(Debug, UiType)]
#[ui(attached(name = "Size", ty = f32, default = 14.0, affects = layout, target = any, inherits))]
struct Text;

#[derive(Debug, UiType)]
struct Label {
  #[ui(key = Text::SIZE)]
  size: Property<f32>,
}

// A field that shares an attached key starts at its default, and is the attached property: a value
// set either way reads back both ways, and a parent's attached value reaches it.
#[test]
fn key_takes_an_attached_key() {
  let mut tree = UiTree::new();
  let parent = tree.alloc(Box::new(Text));
  let label = tree.alloc(Box::new(Label::default()));
  tree.add_child(TreeKind::Logical, parent, label);
  let field = |tree: &UiTree| tree.get_as::<Label>(label).expect("a label").size.get();

  assert_eq!(field(&tree), 14.0);
  Text::set_size(&mut tree, parent, 20.0);
  assert_eq!(field(&tree), 20.0);
  Text::set_size(&mut tree, label, 30.0);
  assert_eq!(field(&tree), 30.0);
  assert_eq!(Text::size(&tree, label), 30.0);
  assert_eq!(lookup("Label").properties[0].name, "Size");
}

static WIDTH: PropertyKey<f32> = PropertyKey::new("Width", Affects::LAYOUT, Inheritance::None);
const WIDTH_KEY: &PropertyKey<f32> = &WIDTH;

#[derive(Debug, UiType)]
struct Sized {
  #[ui(key = WIDTH_KEY, default = 3.0)]
  width: Property<f32>,
}

// A property key reached through a reference-typed constant still takes a `default`.
#[test]
fn key_through_a_reference_takes_a_default() {
  assert_eq!(Sized::default().width.get(), 3.0);
}

// A derived type's namespace is its crate: this test crate, `derive`.
#[test]
fn derived_namespace_is_the_crate() {
  assert_eq!(<Border as artic_registry::UiType>::TYPE_INFO.namespace(), "derive");
}

// Builder calls write the same values, at the same precedence, as setting the fields directly.
#[test]
fn builder_matches_direct_set() {
  let built = Border::new()
    .padding(Thickness(4.0))
    .background(Brush("red".to_owned()))
    .corner_radius(CornerRadius(2.0));

  let direct = Border::default();
  direct.padding.set(Thickness(4.0));
  direct.background.set(Some(Brush("red".to_owned())));
  direct.corner_radius.set(CornerRadius(2.0));

  assert_eq!(built.padding.get(), direct.padding.get());
  assert_eq!(built.background.get(), direct.background.get());
  assert_eq!(built.corner_radius.get(), direct.corner_radius.get());
  for (built, direct) in [
    (built.padding.effective_source(), direct.padding.effective_source()),
    (built.background.effective_source(), direct.background.effective_source()),
    (
      built.corner_radius.effective_source(),
      direct.corner_radius.effective_source(),
    ),
  ] {
    assert_eq!((built, direct), (Precedence::Local, Precedence::Local));
  }
}

// `new` gives every property its default, as `Default` does.
#[test]
fn new_is_the_default() {
  let border = Border::new();

  assert_eq!(border.padding.get(), Thickness::ZERO);
  assert_eq!(border.padding.effective_source(), Precedence::Default);
}

// `build` adds the node to the tree and returns a handle to it, values included.
#[test]
fn build_adds_the_node_to_the_tree() {
  let mut tree = UiTree::new();

  let border: Handle<Border> = Border::new().padding(Thickness(8.0)).build(&mut tree);

  assert_eq!(border.get(&tree).padding.get(), Thickness(8.0));
  assert!(tree.get_as::<Border>(border.id()).is_some());
}

// A field with a shared key gets a method that writes its own property at the local level.
#[test]
fn shared_key_method_sets_local() {
  let block = TextBlock::new().font_size(20.0);

  assert_eq!(block.font_size.get(), 20.0);
  assert_eq!(block.font_size.effective_source(), Precedence::Local);
}

/// A type with a raw-identifier field, as a UI language's `Type` property would be.
#[derive(Debug, UiType)]
struct Input {
  #[ui]
  r#type: Property<String>,
}

// A raw-identifier field keeps its raw name as a method.
#[test]
fn raw_identifier_field_gets_a_method() {
  let input = Input::new().r#type("password");

  assert_eq!(input.r#type.get(), "password");
}

// A property with its own key gets a public constant for that key, named after the field.
#[test]
fn own_key_has_a_public_constant() {
  let border = Border::new();

  assert!(std::ptr::eq(Border::PADDING, border.padding.key()));
  assert_eq!(Border::CORNER_RADIUS.name(), "CornerRadius");
  assert_eq!(Input::TYPE.name(), "Type");
}

/// A single-child element, as `Border` holds its child.
#[derive(Debug, UiType)]
struct Frame {
  #[ui(content)]
  child: Child,
}

/// A panel, as `StackPanel` holds its children.
#[derive(Debug, UiType)]
struct Panel {
  #[ui(content)]
  children: Children,
}

// `.children` builds the node, then links each child in order.
#[test]
fn children_builder_links_in_order() {
  let mut tree = UiTree::new();
  let first = Frame::new().build(&mut tree);
  let second = Frame::new().build(&mut tree);

  let panel = Panel::new().children([first.id(), second.id()]).build(&mut tree);

  assert_eq!(
    tree.children(TreeKind::Logical, panel.id()).collect::<Vec<_>>(),
    [first.id(), second.id()]
  );
}

// `.child` links the one child in both trees.
#[test]
fn child_builder_links_in_both_trees() {
  let mut tree = UiTree::new();
  let leaf = Frame::new().build(&mut tree);

  let frame = Frame::new().child(leaf).build(&mut tree);

  for kind in [TreeKind::Logical, TreeKind::Visual] {
    assert_eq!(tree.children(kind, frame.id()).collect::<Vec<_>>(), [leaf.id()]);
  }
}

// A child that isn't in the tree is a bug in the calling code, and `build` panics.
#[test]
#[should_panic(expected = "isn't in the tree")]
fn build_panics_on_a_child_not_in_the_tree() {
  let mut tree = UiTree::new();
  let leaf = Frame::new().build(&mut tree);
  tree.free(leaf.id());

  let _ = Frame::new().child(leaf).build(&mut tree);
}

// A child listed twice would be linked twice and end up once, so `build` panics.
#[test]
#[should_panic(expected = "listed twice")]
fn build_panics_on_a_repeated_child() {
  let mut tree = UiTree::new();
  let leaf = Frame::new().build(&mut tree);

  let _ = Panel::new().children([leaf, leaf]).build(&mut tree);
}

// The checks run before the node is allocated, so a failed build leaves the tree as it was.
#[test]
fn failed_build_allocates_nothing() {
  let mut tree = UiTree::new();
  let leaf = Frame::new().build(&mut tree);
  let before = tree.len();

  let result = catch_unwind(AssertUnwindSafe(|| Panel::new().children([leaf, leaf]).build(&mut tree)));

  assert!(result.is_err());
  assert_eq!(tree.len(), before);
}

// A flattened group's methods set its properties at the local level, beside the type's own.
#[test]
fn group_methods_set_local() {
  let widget = Widget::new().margin(3).enabled(false).size(2);

  assert_eq!(widget.common.margin.get(), 3);
  assert!(!widget.common.enabled.get());
  assert_eq!(widget.common.margin.effective_source(), Precedence::Local);
  assert_eq!(widget.size.get(), 2);
}

// A group names its methods trait, and its keys have public names on it.
#[test]
fn group_describes_its_trait_and_keys() {
  assert!(<Common as PropertyGroup>::METHODS.ends_with("::CommonMethods"));
  assert_eq!(Common::MARGIN.name(), "Margin");
}
