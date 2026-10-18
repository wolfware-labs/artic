use artic_core::property::Property;
use artic_macros::UiType;

#[derive(Debug, Default, PartialEq)]
struct Telemetry;

#[derive(Debug, UiType)]
struct Gauge {
  #[ui]
  reading: Property<Telemetry>,
}

fn main() {}
