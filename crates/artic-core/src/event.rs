//! Events a node raises for handlers to react to.

use std::fmt::{Debug, Formatter, Result};
use std::marker::PhantomData;

/// An event a node raises, whose handlers receive arguments of type `A`.
///
/// Declaring an `Event<A>` field makes it an event of the node's type, such as
/// `PointerPressed`. For now the field only declares the event; raising it and calling handlers
/// come with event routing.
///
/// # Examples
///
/// ```
/// use artic_core::event::Event;
///
/// #[derive(Debug)]
/// struct PointerArgs;
///
/// let pressed: Event<PointerArgs> = Event::default();
/// ```
pub struct Event<A>(PhantomData<fn(&A)>);

impl<A> Default for Event<A> {
  fn default() -> Self {
    Event(PhantomData)
  }
}

impl<A> Debug for Event<A> {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    formatter.write_str("Event")
  }
}
