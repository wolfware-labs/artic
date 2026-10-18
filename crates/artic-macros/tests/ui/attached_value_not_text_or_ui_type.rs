use artic_macros::UiType;

#[derive(Debug, Default, Clone, PartialEq)]
struct Telemetry;

#[derive(Debug, UiType)]
#[ui(attached(name = "Reading", ty = Telemetry, target = any))]
struct Gauge;

fn main() {}
