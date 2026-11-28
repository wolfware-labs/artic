use artic_core::Object;
use artic_core::property::{Property, PropertyVisitor};
use artic_macros::UiType;
use artic_registry::{TypeInfo, TypeKind, UiType, UiValue};

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

artic_core::node_type_value!(Reading);

#[derive(Debug, UiType)]
struct Gauge {
  #[ui]
  reading: Property<Option<Reading>>,
}

fn main() {}
