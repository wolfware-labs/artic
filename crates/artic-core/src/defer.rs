//! Cleanup that runs when a scope ends, whether it returns or unwinds.

/// Runs its cleanup when dropped, on return and while a panic unwinds alike, unless cancelled.
pub(crate) struct Defer<F: FnOnce()>(Option<F>);

/// A guard that runs `cleanup` when it goes out of scope.
pub(crate) fn defer<F: FnOnce()>(cleanup: F) -> Defer<F> {
  Defer(Some(cleanup))
}

impl<F: FnOnce()> Defer<F> {
  /// Drops the cleanup without running it.
  pub(crate) fn cancel(mut self) {
    self.0 = None;
  }
}

impl<F: FnOnce()> Drop for Defer<F> {
  fn drop(&mut self) {
    if let Some(cleanup) = self.0.take() {
      cleanup();
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::cell::Cell;
  use std::panic::{AssertUnwindSafe, catch_unwind};

  // The cleanup runs when the guard's scope returns and when it unwinds, and never once cancelled.
  #[test]
  fn cleanup_runs_on_return_and_unwind_unless_cancelled() {
    let runs = Cell::new(0);
    let bump = || runs.set(runs.get() + 1);

    drop(defer(bump));
    let _ = catch_unwind(AssertUnwindSafe(|| {
      let _guard = defer(bump);
      panic!("unwinds");
    }));
    defer(bump).cancel();

    assert_eq!(runs.get(), 2);
  }
}
