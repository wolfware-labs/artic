use artic_core::property::Property;
use artic_core::{AsText, RustExpr, TakesObject, ToRust, Written};
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

impl ToRust for Telemetry {
  fn to_rust(&self, out: &mut RustExpr) {
    out.raw("Telemetry");
  }
}

impl Written for Telemetry {
  type As = AsText;
  type FromObject = TakesObject;
}

#[derive(Debug, UiType)]
struct Gauge {
  #[ui]
  reading: Property<Telemetry>,
}

fn main() {}
