use crate::reactive::runtime::Runtime;

/// Runs `run` now and again whenever a signal or memo it read changes.
///
/// `run` runs once right away, recording every signal and memo it reads. After one of them changes,
/// it doesn't run at the moment of the write: it runs at the next flush point, which is
/// [`flush_effects`] or the end of the outermost [`batch`]. Like a memo, it records its dependencies
/// afresh on each run.
///
/// Effects are where reactive values reach the outside world: they update widgets, write logs or
/// mark layout as dirty. They return nothing, and their closure may keep its own mutable state
/// between runs. An effect lives until the [`Scope`] that was active when it was created is
/// disposed; there is no handle to it.
///
/// If `run` panics during the first run, the panic reaches the caller and the next flush runs it
/// again.
///
/// [`flush_effects`]: crate::reactive::flush_effects
/// [`batch`]: crate::reactive::batch
/// [`Scope`]: crate::reactive::Scope
///
/// # Examples
///
/// ```
/// use artic_core::reactive::{Signal, effect, flush_effects};
/// use std::cell::Cell;
/// use std::rc::Rc;
///
/// let count = Signal::new(1);
/// let seen = Rc::new(Cell::new(0));
/// let seen_in_effect = Rc::clone(&seen);
/// effect(move || seen_in_effect.set(count.get()));
/// assert_eq!(seen.get(), 1);
///
/// count.set(2);
/// assert_eq!(seen.get(), 1, "effects wait for a flush point");
/// flush_effects();
/// assert_eq!(seen.get(), 2);
/// ```
pub fn effect(run: impl FnMut() + 'static) {
  Runtime::create_effect(run);
}
