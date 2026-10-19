//! The style classes an element has.

use artic_core::{RustExpr, ToRust};
use std::convert::Infallible;
use std::str::FromStr;

/// The style classes of an element, such as `primary` in `Classes="primary large"`, which style
/// selectors like `Button.primary` match.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct Classes(Vec<String>);

impl Classes {
  /// Whether `class` is among them.
  #[must_use]
  pub fn contains(&self, class: &str) -> bool {
    self.0.iter().any(|name| name == class)
  }
}

impl FromStr for Classes {
  type Err = Infallible;

  /// Splits `text` at whitespace; any text is valid.
  fn from_str(text: &str) -> Result<Self, Self::Err> {
    Ok(Self(text.split_whitespace().map(str::to_owned).collect()))
  }
}

impl<const N: usize> From<[&str; N]> for Classes {
  /// Each item split at whitespace, as `FromStr` splits the text.
  fn from(names: [&str; N]) -> Self {
    Self(
      names
        .iter()
        .flat_map(|names| names.split_whitespace())
        .map(str::to_owned)
        .collect(),
    )
  }
}

impl ToRust for Classes {
  /// Writes `Classes::from([...])` with each name, or `Classes::default()` when there are none.
  fn to_rust(&self, out: &mut RustExpr) {
    if self.0.is_empty() {
      out.type_path(env!("CARGO_CRATE_NAME"), "Classes::default()");
      return;
    }
    out.type_path(env!("CARGO_CRATE_NAME"), "Classes::from([");
    out.list(&self.0, |out, name| out.str(name));
    out.raw("])");
  }
}
