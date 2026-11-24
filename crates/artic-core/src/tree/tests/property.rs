use crate::Object;
use crate::property::{Affects, AttachedKey, Inheritance, Precedence, Property, PropertyKey, PropertyVisitor};
use crate::tree::UiTree;
use pretty_assertions::assert_eq;

static WIDTH: PropertyKey<f32> = PropertyKey::new("Width", Affects::LAYOUT, Inheritance::None);
static HEIGHT: PropertyKey<f32> = PropertyKey::new("Height", Affects::LAYOUT, Inheritance::None);
static ROW: AttachedKey<usize> = AttachedKey::new("Row", Affects::LAYOUT, Inheritance::None, || 0);

#[derive(Debug)]
struct Panel {
  width: Property<f32>,
}

impl Object for Panel {
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
    visitor.visit(&self.width);
  }
}

/// A different type holding the same key, as two types holding one property group do.
#[derive(Debug)]
struct Canvas {
  width: Property<f32>,
}

impl Object for Canvas {
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
    visitor.visit(&self.width);
  }
}

// A field is found by its key and written through the handle the lookup returns.
#[test]
fn finds_a_field_by_its_key() {
  let mut tree = UiTree::new();
  let panel = tree.alloc(Box::new(Panel {
    width: Property::new(&WIDTH, 0.0),
  }));

  tree
    .property(panel, &WIDTH)
    .expect("the field's key")
    .set_at(Precedence::StyleSetter, 80.0);

  assert_eq!(tree.get_as::<Panel>(panel).map(|panel| panel.width.get()), Some(80.0));
}

// One key reaches its property on any type that holds it, without knowing the type.
#[test]
fn one_key_reaches_every_type_that_holds_it() {
  let mut tree = UiTree::new();
  let panel = tree.alloc(Box::new(Panel {
    width: Property::new(&WIDTH, 0.0),
  }));
  let canvas = tree.alloc(Box::new(Canvas {
    width: Property::new(&WIDTH, 0.0),
  }));

  for (node, width) in [(panel, 10.0), (canvas, 20.0)] {
    tree.property(node, &WIDTH).expect("the field's key").set(width);
  }

  assert_eq!(tree.get_as::<Panel>(panel).map(|panel| panel.width.get()), Some(10.0));
  assert_eq!(tree.get_as::<Canvas>(canvas).map(|canvas| canvas.width.get()), Some(20.0));
}

// A key the node holds as neither a field nor an attached value finds nothing, and stores nothing.
#[test]
fn a_key_the_node_lacks_finds_nothing() {
  let mut tree = UiTree::new();
  let panel = tree.alloc(Box::new(Panel {
    width: Property::new(&WIDTH, 0.0),
  }));

  assert!(tree.property(panel, &HEIGHT).is_none());
  assert!(tree.property(panel, ROW.key()).is_none());
  assert_eq!(tree.attached_values.count(panel), 0);
}

// An attached value the node holds is found by its key too, as the same property.
#[test]
fn finds_an_attached_value_by_its_key() {
  let mut tree = UiTree::new();
  let panel = tree.alloc(Box::new(Panel {
    width: Property::new(&WIDTH, 0.0),
  }));
  tree.attached_property(panel, &ROW).set(2);

  tree.property(panel, ROW.key()).expect("the attached value").set(5);

  assert_eq!(tree.attached(panel, &ROW), 5);
}

// A stale node is a caller's error, as for every other lookup by node.
#[test]
#[should_panic(expected = "is not in the arena")]
fn a_stale_node_panics() {
  let mut tree = UiTree::new();
  let panel = tree.alloc(Box::new(Panel {
    width: Property::new(&WIDTH, 0.0),
  }));
  tree.free(panel);

  let _ = tree.property(panel, &WIDTH);
}
