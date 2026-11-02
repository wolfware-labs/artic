//! Cleanup that runs when a scope ends, whether it returns or unwinds.

use std::thread;

/// Runs its cleanup when dropped, on return and while a panic unwinds alike, unless cancelled. A
/// cleanup runs during unwinding, so it must not panic itself: that would abort the process.
#[must_use = "the cleanup runs when the guard is dropped, so binding it to `_` runs it at once"]
pub(crate) struct Defer<F: FnOnce(bool)> {
  cleanup: Option<F>,
  unwinding: bool,
}

/// A guard that runs `cleanup` when it goes out of scope.
pub(crate) fn defer(cleanup: impl FnOnce()) -> Defer<impl FnOnce(bool)> {
  defer_unwind(move |_| cleanup())
}

/// A guard that runs `cleanup` when it goes out of scope, telling it whether the code it guards
/// panicked. A panic already unwinding when the guard was made doesn't count.
pub(crate) fn defer_unwind<F: FnOnce(bool)>(cleanup: F) -> Defer<F> {
  Defer {
    cleanup: Some(cleanup),
    unwinding: thread::panicking(),
  }
}

impl<F: FnOnce(bool)> Defer<F> {
  /// Drops the cleanup without running it.
  pub(crate) fn cancel(mut self) {
    self.cleanup = None;
  }
}

impl<F: FnOnce(bool)> Drop for Defer<F> {
  fn drop(&mut self) {
    if let Some(cleanup) = self.cleanup.take() {
      cleanup(thread::panicking() && !self.unwinding);
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

  // A cleanup learns that its own code panicked, but not that a guard made during another panic's
  // unwinding returned normally.
  #[test]
  fn a_cleanup_learns_whether_its_own_code_panicked() {
    let (own, other) = (Cell::new(None), Cell::new(None));
    /// Makes a guard and returns normally while the panic that drops it unwinds.
    struct InDrop<'a>(&'a dyn Fn());
    impl Drop for InDrop<'_> {
      fn drop(&mut self) {
        (self.0)();
      }
    }

    let _ = catch_unwind(AssertUnwindSafe(|| {
      let _guard = defer_unwind(|panicked| own.set(Some(panicked)));
      let _returns = InDrop(&|| drop(defer_unwind(|panicked| other.set(Some(panicked)))));
      panic!("unwinds");
    }));

    assert_eq!((own.get(), other.get()), (Some(true), Some(false)));
  }
}
