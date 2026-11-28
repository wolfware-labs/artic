use artic_core::Object;
use artic_core::property::{Property, PropertyVisitor};
use artic_macros::UiType;
use artic_registry::{TypeInfo, UiType};

#[derive(Debug, Default, PartialEq)]
struct Telemetry;

static TELEMETRY: TypeInfo = TypeInfo::node("Telemetry", "telemetry::Telemetry", |_| false);

impl UiType for Telemetry {
  const TYPE_INFO: &'static TypeInfo = &TELEMETRY;
}

impl Object for Telemetry {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

#[derive(Debug, UiType)]
struct Gauge {
  #[ui]
  reading: Property<Telemetry>,
}

fn main() {}
