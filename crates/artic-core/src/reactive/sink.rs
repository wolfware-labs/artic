//! A reader the runtime calls back when a write reaches its signal, or the source it is linked to:
//! how a node's dirty flags follow its properties.

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
  Runtime::create_sink(of, mark);
}

/// Links `of`'s sink to `source`, in place of the source it had: it is then marked by every change
/// `source`'s sink is marked by, or by every visible write to `source` when that has no sink. `None`
/// leaves it reading only `of`. A signal with no sink is left alone.
pub(crate) fn link_sink(of: SignalId, source: Option<SignalId>) {
  Runtime::link_sink(of, source);
}

/// Whether `signal` has a sink.
pub(crate) fn has_sink(signal: SignalId) -> bool {
  Runtime::has_sink(signal)
}
