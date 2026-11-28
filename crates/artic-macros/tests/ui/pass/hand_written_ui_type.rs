// A `UiType` implemented by hand is a property value once it says how it is written: one line of
// `object_value!` for a type not taken from objects, which the loader then refuses, and the
// written-out impl with `TakesObject` for a hand-written value, which the loader moves in.

use artic_core::Object;
use artic_core::property::{Precedence, Property, PropertyVisitor};
use artic_core::tree::UiTree;
use artic_core::{AsObject, TakesObject, Written};
use artic_macros::UiType;
use artic_registry::{TypeInfo, TypeKind, UiType, UiValue, ValueError};

#[derive(Debug, Default, PartialEq)]
struct Telemetry;

static TELEMETRY: TypeInfo = TypeInfo::node("Telemetry", "telemetry::Telemetry", |_| false);

impl UiType for Telemetry {
  const TYPE_INFO: &'static TypeInfo = &TELEMETRY;
}

impl Object for Telemetry {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

artic_core::object_value!(Telemetry);

#[derive(Debug, Default, Clone, PartialEq)]
struct Reading(u8);

static READING: TypeInfo = TypeInfo::node("Reading", "telemetry::Reading", |_| false).with_kind(TypeKind::Value);

impl UiType for Reading {
  const TYPE_INFO: &'static TypeInfo = &READING;
}

impl Object for Reading {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl UiValue for Reading {}

impl Written for Reading {
  type As = AsObject;
  type FromObject = TakesObject;
}

#[derive(Debug, UiType)]
struct Gauge {
  #[ui]
  telemetry: Property<Telemetry>,
  #[ui]
  reading: Property<Option<Reading>>,
}

fn main() {
  let mut tree = UiTree::new();
  let gauge = tree.alloc(Box::new(Gauge::default()));
  let property = |name| Gauge::TYPE_INFO.property(name).expect("a property").info.access.expect("thunks");

  let staged = tree.alloc(Box::new(Telemetry));
  let refused = (property("Telemetry").set_object)(&mut tree, gauge, staged, Precedence::Local);
  assert!(matches!(refused, Err(ValueError::WrongType { found: "an object", .. })), "{refused:?}");
  assert!(!tree.contains(staged));

  let staged = tree.alloc(Box::new(Reading(7)));
  let taken = (property("Reading").set_object)(&mut tree, gauge, staged, Precedence::Local);
  assert_eq!(taken, Ok(()));
  assert!(!tree.contains(staged));
  assert_eq!(tree.get_as::<Gauge>(gauge).map(|gauge| gauge.reading.get()), Some(Some(Reading(7))));
}
