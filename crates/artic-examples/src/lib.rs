//! Examples that open a window: each file in `examples/` runs artic through the `winit` host and the `vello` renderer.

use artic_core::error::Chain;
use std::fmt::{self, Debug, Display, Formatter};

/// The error an example's `main` returns. Rust prints it with `Debug`, which shows its message and
/// each cause, as [`Chain`] does.
pub struct Report(Box<dyn std::error::Error>);

impl<E: std::error::Error + 'static> From<E> for Report {
  fn from(error: E) -> Report {
    Report(Box::new(error))
  }
}

impl Debug for Report {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    Display::fmt(&Chain(&*self.0), f)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[derive(Debug)]
  struct Failed(std::io::Error);

  impl Display for Failed {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
      f.write_str("can't start")
    }
  }

  impl std::error::Error for Failed {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
      Some(&self.0)
    }
  }

  // A `main` that returns a report prints the error's message and its cause.
  #[test]
  fn report_prints_the_message_and_its_cause() {
    let report = Report::from(Failed(std::io::Error::other("no display")));

    assert_eq!(format!("{report:?}"), "can't start: no display");
  }
}
