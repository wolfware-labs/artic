//! A signal's sink: a mark the runtime calls in the write that reaches the signal, directly or
//! through the signals it is linked below.

use crate::reactive::runtime::{Runtime, SignalId};
use std::rc::Rc;

/// Gives the signal `of` a sink, which calls `mark` at once, in the write, when a visible write
/// reaches `of` or a signal [`link_signal`] linked it below. It isn't an effect, so no flush runs it. It
/// lives as long as `of`.
///
/// # Panics
///
/// Panics if `of` already has a sink.
pub(crate) fn sink(of: SignalId, mark: Rc<dyn Fn()>) {
  Runtime::set_sink(of, mark);
}

/// Links `of` below `source`, in place of the source it had: a visible write to `source`, or a change
/// that reaches it through its own link, then reaches `of` and the signals linked below `of`, and
/// calls each one's sink. `None` unlinks it.
///
/// # Panics
///
/// Panics if `source` is `of` or linked below it.
pub(crate) fn link_signal(of: SignalId, source: Option<SignalId>) {
  Runtime::link_signal(of, source);
}

/// Whether `signal` has a sink.
pub(crate) fn has_sink(signal: SignalId) -> bool {
  Runtime::has_sink(signal)
}
