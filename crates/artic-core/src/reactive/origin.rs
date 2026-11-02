#[cfg(debug_assertions)]
use std::panic::Location;

#[derive(Debug, Clone, Copy)]
pub struct Origin {
  #[cfg(debug_assertions)]
  location: &'static Location<'static>,
}

impl Origin {
  #[track_caller]
  pub fn caller() -> Self {
    Self {
      #[cfg(debug_assertions)]
      location: Location::caller(),
    }
  }

  /// Panics with `message`, followed in debug builds by where the value was created.
  pub fn panic(self, message: &str) -> ! {
    #[cfg(debug_assertions)]
    panic!("{message} (created at {})", self.location);
    #[cfg(not(debug_assertions))]
    panic!("{message}");
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use static_assertions::assert_eq_size;

  #[cfg(not(debug_assertions))]
  assert_eq_size!(Origin, ());
  #[cfg(debug_assertions)]
  assert_eq_size!(Origin, &'static Location<'static>);
}
