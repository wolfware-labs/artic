//! Fine-grained reactivity: values that know who reads them.
//!
//! A [`Signal`] holds a value and remembers which observers read it. An observer is something that
//! reads signals and is marked dirty when they change; a [`Memo`] and an [`effect`](fn@effect) each use one to
//! know when their last run is out of date. Reading a signal while an observer runs subscribes that
//! observer to the signal, and writing the signal marks every subscribed observer as dirty. Nothing
//! subscribes explicitly: the reads themselves record the dependencies.
//!
//! The runtime that stores signal values is thread-local, and signals can only be used on the
//! thread that created them. A `Signal<T>` is a small `Copy` handle into that runtime, so copying it
//! never copies the value.
//!
//! ```
//! use artic_core::reactive::Signal;
//!
//! let count = Signal::new(0);
//! let copy = count;            // a second handle to the same value
//! count.set(5);
//! assert_eq!(copy.get(), 5);
//! ```
//!
//! # Derived values
//!
//! A [`Memo`] computes a value from signals and other memos and remembers it. It runs its closure
//! only when read after a dependency changed, and a run that produces an equal value leaves the
//! memo's own readers up to date.
//!
//! ```
//! use artic_core::reactive::{Memo, Signal};
//!
//! let count = Signal::new(2);
//! let is_even = Memo::new(move || count.get() % 2 == 0);
//! assert!(is_even.get());
//! ```
//!
//! # Effects
//!
//! An [`effect`](fn@effect) runs a closure once when created and again after something it read changes. It
//! doesn't run at the moment of the write: writes only mark it dirty, and it runs at the next flush
//! point. A flush point is a call to [`flush_effects`], the end of the outermost [`batch`], and the
//! end of each task run by [`Dispatcher::run_pending`](crate::dispatcher::Dispatcher::run_pending).
//!
//! A flush visits only the effects that the writes since the last flush can reach: it follows each
//! written signal to the memos that read it, and on to the effects behind them. Its cost depends on
//! what the writes touched, not on how many effects exist. It visits those effects in the order
//! they were created. Each one first brings the memos it read up to date and runs only if one of
//! its inputs actually changed. Because a memo is brought up to date when it is read, an effect
//! never sees a fresh signal next to a stale memo, whatever order the effects run in.
//!
//! If an effect writes a signal during a flush, the effects that write reaches run too: in the same
//! pass if they were created later, otherwise in another pass, so when the flush returns every
//! effect has seen the final values. Effects that keep writing each other's
//! inputs never settle; the flush panics after 100 passes instead of looping forever.
//!
//! ```
//! use artic_core::reactive::{Signal, effect, flush_effects};
//! use std::cell::RefCell;
//! use std::rc::Rc;
//!
//! let name = Signal::new(String::from("Ada"));
//! let title = Rc::new(RefCell::new(String::new()));
//! let title_in_effect = Rc::clone(&title);
//! effect(move || *title_in_effect.borrow_mut() = format!("Hello, {}", name.get()));
//!
//! name.set(String::from("Grace"));
//! assert_eq!(*title.borrow(), "Hello, Ada"); // not flushed yet
//! flush_effects();
//! assert_eq!(*title.borrow(), "Hello, Grace");
//! ```
//!
//! # Scopes
//!
//! A [`Scope`] owns the signals, memos and effects created while it is active, and
//! [`Scope::dispose`] frees them all at once. This is how a closed view drops its reactive state
//! without unsubscribing anything by hand. Scopes form a tree, and disposing one disposes its child
//! scopes first.
//!
//! ```
//! use artic_core::reactive::{Scope, Signal, effect, flush_effects};
//! use std::cell::Cell;
//! use std::rc::Rc;
//!
//! let theme = Signal::new("light");
//! let border_updates = Rc::new(Cell::new(0));
//! let border_updates_in_effect = Rc::clone(&border_updates);
//!
//! let view = Scope::new_root();
//! view.run(|| {
//!   effect(move || {
//!     theme.get();
//!     border_updates_in_effect.set(border_updates_in_effect.get() + 1);
//!   });
//! });
//!
//! view.dispose();
//! theme.set("dark");
//! flush_effects();
//! assert_eq!(border_updates.get(), 1, "the disposed effect didn't run again");
//! ```
//!
//! Nodes created while no scope is active have no owner and live until the thread ends.
//!
//! Each effect and memo also owns what its closure creates. Before the closure runs again, the
//! nodes from its previous run are disposed, so an effect that creates an effect on every run keeps
//! only the latest one.
//!
//! A signal or memo that outlives an effect that read it forgets that effect when the effect is
//! disposed. Using a signal or memo after its scope was disposed panics; in debug builds the message
//! says where it was created. [`live_counts`](fn@live_counts) reports how many nodes are alive, for leak checks.
//!
//! # Skipping writes that change nothing
//!
//! [`Signal::set`] and [`Signal::update`] always mark subscribers as dirty, even when the new value
//! equals the old one, so they work for every value type. For types that implement `PartialEq`,
//! [`Signal::set_if_changed`] compares first and does nothing when the values are equal.
//!
//! The skip is a separate method because stable Rust can't give one method two behaviors depending
//! on whether `T: PartialEq` (that would need specialization). Requiring `PartialEq` on `set` itself
//! would rule out signals of types that can't be compared, such as closures or handles, and storing
//! a comparison function per signal at construction would cost every signal for a choice most
//! writes don't need. A separate method keeps `set` unrestricted and makes skipping a visible,
//! opt-in choice at the call site.
//!
//! The comparison inside `set_if_changed` reads the value without subscribing the running
//! observer, so it is still only a write.
//!
//! When only some edits matter to readers, [`Signal::update_with`] edits the value in place and
//! lets the closure say whether readers can see the edit: a [`Change::Hidden`] edit is kept but
//! wakes nobody. [`Property`](crate::property::Property) uses it so that a write below the winning
//! level stays hidden.

mod effect;
mod live_counts;
mod memo;
mod origin;
mod runtime;
mod scope;
mod signal;
#[cfg(test)]
mod tests;

pub use effect::effect;
#[cfg(any(test, feature = "testing"))]
pub use live_counts::assert_no_reactive_leaks;
pub use live_counts::{LiveCounts, live_counts};
pub use memo::Memo;
#[cfg(debug_assertions)]
pub(crate) use runtime::SignalId;
pub use scope::Scope;
pub use signal::{Change, Signal};

/// Runs every effect whose dependencies changed since it last ran.
///
/// Only effects that a write since the last flush can reach are visited, through any memos in
/// between. They run in the order they were created, each at most once per pass. If an effect
/// writes a signal during the flush, the effects that write reaches run too, so when
/// `flush_effects` returns every effect has seen the latest values. Effects that never settle,
/// because they keep writing each other's inputs, make it panic after 100 passes.
///
/// Calling it while a flush is already running, for example from inside an effect, does nothing:
/// the running flush picks up the change. Inside a [`batch`] it does nothing either; the outermost
/// `batch` flushes when it returns.
///
/// # Panics
///
/// Panics if the effects don't settle after 100 passes. A panic inside an effect continues to the
/// caller; that effect and the reached effects not yet visited run on the next flush.
pub fn flush_effects() {
  runtime::Runtime::flush_effects();
}

/// Runs `action` and then flushes effects once, so all the writes inside it cause a single flush.
///
/// Batches nest: only the outermost `batch` flushes. If `action` panics, nothing is flushed and the
/// panic continues; the pending effects run at the next flush point.
///
/// # Examples
///
/// ```
/// use artic_core::reactive::{Signal, batch, effect};
/// use std::cell::Cell;
/// use std::rc::Rc;
///
/// let first = Signal::new(String::from("Ada"));
/// let last = Signal::new(String::from("Byron"));
/// let runs = Rc::new(Cell::new(0));
/// let runs_in_effect = Rc::clone(&runs);
/// effect(move || {
///   runs_in_effect.set(runs_in_effect.get() + 1);
///   format!("{} {}", first.get(), last.get());
/// });
///
/// batch(|| {
///   first.set(String::from("Augusta Ada"));
///   last.set(String::from("King"));
/// });
/// assert_eq!(runs.get(), 2, "one run on creation, one for the whole batch");
/// ```
pub fn batch<TResult>(action: impl FnOnce() -> TResult) -> TResult {
  runtime::Runtime::batch(action)
}
