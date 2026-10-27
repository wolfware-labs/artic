//! A control crate the fixture app's namespace map names and no view uses, so its types reach the
//! app's registry only through the generated files' `use` lines.

use artic_core::property::Property;
use artic_macros::UiType;

/// A control no view uses.
#[derive(Debug, UiType)]
pub struct Gauge {
  /// The value shown.
  #[ui]
  pub value: Property<f32>,
}
