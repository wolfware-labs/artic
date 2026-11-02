//! A reader the runtime calls back when what it read changes: how a node's dirty flags follow its
//! properties.

use crate::reactive::runtime::{Node, Runtime, SignalId};
use std::rc::Rc;

/// Reads what `track` reads and, when a visible write reaches any of it, calls `mark` at once, in
/// the write, then reads again, so it follows what its reads depend on now. It isn't an effect: no
/// flush runs it, and no scope owns it, so whoever made it disposes it.
#[derive(Debug)]
pub(crate) struct Sink(Node);

impl Sink {
  /// A sink for the signal `of`, which then has it as its one sink. Panics if `of` already has a
  /// sink.
  pub(crate) fn new(of: SignalId, track: impl Fn() + 'static, mark: impl Fn() + 'static) -> Sink {
    Sink(Runtime::create_sink(of, Rc::new(track), Rc::new(mark)))
  }

  pub(crate) fn dispose(self) {
    Runtime::dispose_sink(self.0);
  }
}

/// Whether `signal` has a sink.
pub(crate) fn has_sink(signal: SignalId) -> bool {
  Runtime::has_sink(signal)
}
