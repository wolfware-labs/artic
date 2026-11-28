use artic_core::property::{Property, PropertyVisitor};
use artic_core::tree::{NodeId, UiTree};
use artic_core::{AsObject, Object, TakesObject, Written};
use artic_macros::UiType;
use artic_registry::{FromUiObject, TypeInfo, UiType, ValueError};

#[derive(Debug, Default, PartialEq)]
struct Telemetry;

static TELEMETRY: TypeInfo = TypeInfo::node("Telemetry", "telemetry::Telemetry", |_| false);

impl UiType for Telemetry {
  const TYPE_INFO: &'static TypeInfo = &TELEMETRY;
}

impl Object for Telemetry {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl FromUiObject for Telemetry {
  fn from_object(tree: &mut UiTree, object: NodeId) -> Result<Telemetry, ValueError> {
    drop(tree.take(object));
    Ok(Telemetry)
  }
}

impl Written for Telemetry {
  type As = AsObject;
  type FromObject = TakesObject;
}

#[derive(Debug, UiType)]
struct Gauge {
  #[ui]
  telemetry: Property<Telemetry>,
}

fn main() {}
