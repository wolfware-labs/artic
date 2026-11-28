use artic_core::property::Property;
use artic_core::{AsChild, RefusesObject, Written};
use artic_macros::UiType;

#[derive(Debug, Default, PartialEq)]
struct Telemetry;

impl Written for Telemetry {
  type As = AsChild;
  type FromObject = RefusesObject;
}

#[derive(Debug, UiType)]
struct Gauge {
  #[ui]
  reading: Property<Telemetry>,
}

fn main() {}
