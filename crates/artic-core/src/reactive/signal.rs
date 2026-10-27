use crate::reactive::origin::Origin;
use crate::reactive::runtime::{Runtime, SignalId};
use std::marker::PhantomData;

/// A reactive value.
///
/// A signal stores a value in the thread-local reactive runtime and tracks who reads it. Reads made
/// while an observer runs ([`get`](Signal::get), [`with`](Signal::with)) subscribe that observer, and
/// writes ([`set`](Signal::set), [`update`](Signal::update)) mark every subscriber as dirty.
/// [`set_if_changed`](Signal::set_if_changed) skips the write when the new value equals the current
/// one, and [`update_with`](Signal::update_with) lets an in-place edit say whether readers can see it.
///
/// `Signal<T>` is a `Copy` handle: copies refer to the same value, and copying never clones `T`. Two
/// handles compare equal when they refer to the same signal, whatever its value.
/// Signals are neither `Send` nor `Sync`, because the runtime that holds the value belongs to one
/// thread.
///
/// # Examples
///
/// ```
/// use artic_core::reactive::Signal;
///
/// let name = Signal::new(String::from("Ada"));
///
/// // Borrow the value without cloning it.
/// let length = name.with(|name| name.len());
/// assert_eq!(length, 3);
///
/// // Change it in place.
/// name.update(|name| name.push_str(" Lovelace"));
/// assert_eq!(name.get(), "Ada Lovelace");
/// ```
///
/// # Panics
///
/// Reading or writing a signal from inside its own [`update`](Signal::update) closure panics, because
/// the value is being changed at that moment. Writing a signal from inside its own
/// [`with`](Signal::with) closure panics too, because the value is borrowed for reading. Reading a
/// signal inside its own `with` is fine, and so is reading or writing *other* signals in either
/// closure.
///
/// Reading or writing a signal after the [`Scope`](crate::reactive::Scope) that owns it was disposed
/// panics. In debug builds the message includes where the signal was created.
#[derive(Debug)]
pub struct Signal<TValue>(SignalId, Origin, PhantomData<*const TValue>)
where
  TValue: 'static;

impl<T> Clone for Signal<T> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<TValue> Copy for Signal<TValue> {}

/// Two handles are equal when they refer to the same signal, whatever its value.
impl<TValue> PartialEq for Signal<TValue> {
  fn eq(&self, other: &Self) -> bool {
    self.0 == other.0
  }
}

impl<TValue> Eq for Signal<TValue> {}

impl<TValue> Signal<TValue> {
  /// Creates a signal holding `initial_value`.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::reactive::Signal;
  ///
  /// let count = Signal::new(0);
  /// assert_eq!(count.get(), 0);
  /// ```
  #[track_caller]
  pub fn new(initial_value: TValue) -> Signal<TValue> {
    Signal(Runtime::create_signal(initial_value), Origin::caller(), PhantomData)
  }

  /// Creates a signal no scope owns, for state whose owner disposes it explicitly, as the tree does
  /// with a node's attached values.
  #[track_caller]
  pub(crate) fn new_unowned(initial_value: TValue) -> Signal<TValue> {
    Signal(Runtime::create_unowned_signal(initial_value), Origin::caller(), PhantomData)
  }

  /// Calls `function` with a reference to the value and returns its result.
  ///
  /// Use this instead of [`get`](Signal::get) to read part of a value, or a value that isn't
  /// `Clone`, without cloning it. Like `get`, it subscribes the running observer, if there is one.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::reactive::Signal;
  ///
  /// let items = Signal::new(vec![1, 2, 3]);
  /// let total: i32 = items.with(|items| items.iter().sum());
  /// assert_eq!(total, 6);
  /// ```
  ///
  /// # Panics
  ///
  /// Panics if called from inside this signal's own [`update`](Signal::update) closure, or if
  /// `function` writes this same signal through [`set`](Signal::set), [`update`](Signal::update) or
  /// [`set_if_changed`](Signal::set_if_changed):
  ///
  /// ```should_panic
  /// use artic_core::reactive::Signal;
  ///
  /// let count = Signal::new(1);
  /// count.with(|_| count.set(2)); // writes `count` while it is being read
  /// ```
  pub fn with<TResult>(&self, function: impl FnOnce(&TValue) -> TResult) -> TResult {
    self.ensure_alive();
    Runtime::with_signal_value(self.0, function)
  }

  /// Replaces the value with `new_value` and marks every subscribed observer as dirty.
  ///
  /// Subscribers are marked even when `new_value` equals the current value. To skip writes that
  /// change nothing, use [`set_if_changed`](Signal::set_if_changed).
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::reactive::Signal;
  ///
  /// let count = Signal::new(0);
  /// count.set(5);
  /// assert_eq!(count.get(), 5);
  /// ```
  ///
  /// # Panics
  ///
  /// Panics if called from inside this signal's own [`update`](Signal::update) or
  /// [`with`](Signal::with) closure.
  pub fn set(&self, new_value: TValue) {
    self.ensure_alive();
    Runtime::set_signal_value(self.0, new_value)
  }

  /// Changes the value in place through `action`, then marks every subscribed observer as dirty.
  ///
  /// Use this instead of [`set`](Signal::set) when building a new value would be wasteful, for
  /// example to push into a `Vec`.
  ///
  /// If `action` panics, the panic continues to the caller after the value is put back and the
  /// subscribers are marked dirty. The value keeps whatever changes `action` made before panicking.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::reactive::Signal;
  ///
  /// let items = Signal::new(vec![1, 2]);
  /// items.update(|items| items.push(3));
  /// assert_eq!(items.get(), vec![1, 2, 3]);
  /// ```
  ///
  /// # Panics
  ///
  /// Panics if called from inside this signal's own [`with`](Signal::with) closure, or if `action`
  /// reads or writes this same signal:
  ///
  /// ```should_panic
  /// use artic_core::reactive::Signal;
  ///
  /// let count = Signal::new(1);
  /// count.update(|value| *value += count.get()); // reads `count` while it is being updated
  /// ```
  pub fn update(&self, action: impl FnOnce(&mut TValue)) {
    self.ensure_alive();
    Runtime::with_signal_mut_value(self.0, action);
  }

  /// Changes the value in place through `action`, and marks subscribed observers as dirty only if
  /// `action` returns [`Change::Visible`].
  ///
  /// Use this when only some edits matter to readers, for example a value with parts that readers
  /// never see. The edit is always kept; a [`Change::Hidden`] edit just doesn't wake anyone, and a
  /// flush after it runs nothing.
  ///
  /// If `action` panics, the value is put back with whatever changes `action` made, subscribers
  /// are marked dirty as after a visible change, and the panic continues to the caller.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::reactive::{Change, Signal};
  ///
  /// // A list whose readers only care about its length.
  /// let items = Signal::new(vec![1, 2]);
  /// items.update_with(|items| {
  ///   items[0] = 10; // same length: nothing readers see changed
  ///   Change::Hidden
  /// });
  /// assert_eq!(items.get(), vec![10, 2]);
  /// ```
  ///
  /// # Panics
  ///
  /// Panics in the same cases as [`update`](Signal::update).
  pub fn update_with(&self, action: impl FnOnce(&mut TValue) -> Change) {
    self.ensure_alive();
    Runtime::update_signal_value_with(self.0, action);
  }

  /// Frees the signal now rather than with its scope. Disposing it again, or disposing its scope
  /// later, does nothing more.
  pub(crate) fn dispose(self) {
    Runtime::dispose_signal(self.0);
  }

  pub(crate) fn is_alive(&self) -> bool {
    Runtime::contains_signal(self.0)
  }

  pub(crate) fn with_untracked<TResult>(&self, function: impl FnOnce(&TValue) -> TResult) -> TResult {
    self.ensure_alive();
    Runtime::with_untracked_signal_value(self.0, function)
  }

  fn ensure_alive(&self) {
    if !Runtime::contains_signal(self.0) {
      self.1.panic("signal used after it was disposed");
    }
  }
}

impl<TValue> Signal<TValue>
where
  TValue: Clone,
{
  /// Returns a clone of the value.
  ///
  /// If an observer is running, it is subscribed to this signal. To read without cloning, use
  /// [`with`](Signal::with).
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::reactive::Signal;
  ///
  /// let greeting = Signal::new(String::from("hello"));
  /// assert_eq!(greeting.get(), "hello");
  /// ```
  ///
  /// # Panics
  ///
  /// Panics if called from inside this signal's own [`update`](Signal::update) closure.
  pub fn get(&self) -> TValue {
    self.ensure_alive();
    Runtime::get_signal_value(self.0)
  }
}

impl<TValue> Signal<TValue>
where
  TValue: PartialEq,
{
  /// Replaces the value with `new_value` only if it differs from the current value.
  ///
  /// When the values are equal, nothing happens: the value is kept and no observer is marked dirty.
  /// Otherwise this behaves like [`set`](Signal::set). The comparison doesn't subscribe the running
  /// observer, so calling this inside an observer is a write, not a read.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::reactive::Signal;
  ///
  /// let count = Signal::new(1);
  /// count.set_if_changed(1); // equal: skipped
  /// count.set_if_changed(2); // different: stored
  /// assert_eq!(count.get(), 2);
  /// ```
  ///
  /// # Panics
  ///
  /// Panics if called from inside this signal's own [`update`](Signal::update) closure. The message
  /// reports a read, because the comparison reads the value first. Also panics if called from inside
  /// this signal's own [`with`](Signal::with) closure with a different value, because the write
  /// happens while the value is borrowed. An equal value writes nothing there, so it doesn't panic.
  pub fn set_if_changed(&self, new_value: TValue) {
    self.ensure_alive();
    if Runtime::with_untracked_signal_value(self.0, |v: &TValue| *v != new_value) {
      self.set(new_value)
    }
  }
}

/// Whether an edit made through [`Signal::update_with`] is visible to the signal's readers.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Change {
  /// Readers can see the edit: subscribed observers are marked dirty.
  Visible,
  /// Readers can't see the edit: it is kept, but nobody is marked dirty.
  Hidden,
}

#[cfg(test)]
impl<TValue> Signal<TValue> {
  pub(crate) fn id(&self) -> SignalId {
    self.0
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use static_assertions::{assert_impl_all, assert_not_impl_any};

  assert_not_impl_any!(Signal<i32>: Send, Sync);
  assert_impl_all!(Signal<i32>: Copy);
}
