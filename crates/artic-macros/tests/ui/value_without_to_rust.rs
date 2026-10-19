use artic_core::property::Property;
use artic_macros::UiType;
use std::convert::Infallible;
use std::str::FromStr;

#[derive(Debug, Default, PartialEq)]
struct Telemetry;

impl FromStr for Telemetry {
  type Err = Infallible;

  fn from_str(_text: &str) -> Result<Self, Self::Err> {
    Ok(Telemetry)
  }
}

#[derive(Debug, UiType)]
struct Gauge {
  #[ui]
  reading: Property<Telemetry>,
}

fn main() {}
