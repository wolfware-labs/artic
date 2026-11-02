//! A signal's sink: a mark the runtime calls when a write reaches the signal, or the source it is
//! linked to. It is how a node's dirty flags follow its properties.

use crate::reactive::runtime::{Runtime, SignalId};
use std::rc::Rc;

/// Gives the signal `of` a sink, which calls `mark` at once, in the write, when a visible write
/// reaches `of` or the source [`link_sink`] gave it. It isn't an effect, so no flush runs it. It
/// lives as long as `of`.
///
/// # Panics
///
/// Panics if `of` already has a sink.
pub(crate) fn sink(of: SignalId, mark: Rc<dyn Fn()>) {
  Runtime::set_sink(of, mark);
}

/// Links `of` to `source`, in place of the source it had: `of`'s sink is then marked by every visible
/// write to `source` and every change that reaches `source` through its own link. `None` unlinks
/// it. A signal with no sink is left alone.
pub(crate) fn link_sink(of: SignalId, source: Option<SignalId>) {
  Runtime::link_sink(of, source);
}

/// Whether `signal` has a sink.
pub(crate) fn has_sink(signal: SignalId) -> bool {
  Runtime::has_sink(signal)
}
