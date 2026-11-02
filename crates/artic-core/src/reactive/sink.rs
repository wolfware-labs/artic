//! A reader the runtime calls back when what it read changes: how a node's dirty flags follow its
//! properties.

use crate::reactive::runtime::{Runtime, SignalId};
use std::rc::Rc;

/// Gives the signal `of` a sink: a reader that reads what `track` reads and, when a visible write
/// reaches any of it, calls `mark` at once, in the write, then reads again, so it follows what its
/// reads depend on now. It isn't an effect, so no flush runs it. It lives as long as `of`.
///
/// # Panics
///
/// Panics if `of` already has a sink.
pub(crate) fn sink(of: SignalId, track: impl Fn() + 'static, mark: impl Fn() + 'static) {
  Runtime::create_sink(of, Rc::new(track), Rc::new(mark));
}

/// Records, for the reader running now, a read of `of`'s sink, or of `of` itself when it has none:
/// the reader then hears of every change the sink is called back for, without reading what the sink
/// reads.
pub(crate) fn read_sink(of: SignalId) {
  Runtime::read_sink(of);
}

/// Whether `signal` has a sink.
pub(crate) fn has_sink(signal: SignalId) -> bool {
  Runtime::has_sink(signal)
}
