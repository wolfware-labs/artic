//! Skipping a test on a machine without what it needs. A job that promises the machine has it sets
//! the requirement's variable to `1`, and the test fails instead.

use std::fmt::Display;

/// Something a test needs from the machine, and the variable that makes it required.
struct Need {
  missing: &'static str,
  variable: &'static str,
}

const GPU: Need = Need {
  missing: "no usable GPU",
  variable: "ARTIC_REQUIRE_GPU",
};

const DISPLAY: Need = Need {
  missing: "no X11 display",
  variable: "ARTIC_REQUIRE_DISPLAY",
};

/// Skips a test on a machine without a usable GPU, printing `reason`.
///
/// # Panics
///
/// When `ARTIC_REQUIRE_GPU` is `1`: a job that promises a GPU fails without one.
pub fn no_gpu(reason: impl Display) {
  GPU.skip(&reason, flag(GPU.variable));
}

/// Skips a test on a machine without an X11 display, printing `reason`.
///
/// # Panics
///
/// When `ARTIC_REQUIRE_DISPLAY` is `1`: a job that promises a display fails without one.
pub fn no_display(reason: impl Display) {
  DISPLAY.skip(&reason, flag(DISPLAY.variable));
}

impl Need {
  fn skip(&self, reason: &dyn Display, required: bool) {
    assert!(!required, "{}, and {}=1: {reason}", self.missing, self.variable);
    eprintln!("skipped: {reason}");
  }
}

pub(crate) fn flag(name: &str) -> bool {
  std::env::var_os(name).is_some_and(|value| value == "1")
}

#[cfg(test)]
mod tests {
  use super::*;

  // Without a GPU a test skips by default.
  #[test]
  fn missing_gpu_skips_by_default() {
    GPU.skip(&"no adapter", false);
  }

  // A job that requires a GPU fails without one.
  #[test]
  #[should_panic(expected = "no usable GPU, and ARTIC_REQUIRE_GPU=1: no adapter")]
  fn required_gpu_fails_without_one() {
    GPU.skip(&"no adapter", true);
  }

  // A job that requires a display fails without one.
  #[test]
  #[should_panic(expected = "no X11 display, and ARTIC_REQUIRE_DISPLAY=1: no server")]
  fn required_display_fails_without_one() {
    DISPLAY.skip(&"no server", true);
  }
}
