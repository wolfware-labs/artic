//! Why the host failed.

/// Why the host failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
  /// The event loop couldn't be created, such as when no display is available.
  #[error("can't start the event loop")]
  Start(#[source] Box<dyn std::error::Error + Send + Sync>),
  /// winit's event loop ended with an error, such as a lost connection to the display server.
  #[error("the event loop failed")]
  Loop(#[source] Box<dyn std::error::Error + Send + Sync>),
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::error::Error as _;

  // A host that can't start says so, and the windowing library's error is its source, printed once.
  #[test]
  fn start_error_shows_its_cause_once() {
    let error = Error::Start(Box::new(std::io::Error::other("no display")));

    assert_eq!(error.to_string(), "can't start the event loop");
    assert_eq!(error.source().map(ToString::to_string), Some("no display".to_owned()));
  }
}
