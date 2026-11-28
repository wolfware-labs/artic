use crate::tree::NodeId;

/// How a UI language writes a value of this type, and whether a staged object can become one.
///
/// A property's value type implements it, and the code a UI type's derive generates picks the
/// value's conversions by its two types alone. A type set from text implements it beside its
/// `FromStr` and [`ToRust`](crate::ToRust), with [`text_value!`](crate::text_value):
///
/// ```
/// use artic_core::{RustExpr, ToRust};
/// use std::str::FromStr;
///
/// #[derive(Debug, Clone, PartialEq)]
/// struct Percent(f32);
///
/// impl FromStr for Percent {
///   type Err = std::num::ParseFloatError;
///
///   fn from_str(text: &str) -> Result<Percent, Self::Err> {
///     text.trim_end_matches('%').parse().map(Percent)
///   }
/// }
///
/// impl ToRust for Percent {
///   fn to_rust(&self, out: &mut RustExpr) {
///     out.call(env!("CARGO_CRATE_NAME"), "Percent", &[&self.0]);
///   }
/// }
///
/// artic_core::text_value!(Percent);
/// ```
///
/// `#[derive(UiEnum)]` and `#[derive(UiType)]` implement it for the types they derive. A type that
/// implements `UiType` by hand implements it with [`object_value!`](crate::object_value), or, for a
/// value that also implements `FromUiObject`, writes the impl out with [`TakesObject`].
#[diagnostic::on_unimplemented(
  message = "`{Self}` can't be written in a UI language",
  label = "neither text, an object nor a child element",
  note = "derive `UiType` or `UiEnum`; a `UiType` implemented by hand adds `artic::core::object_value!({Self})`, and a type set from text implements `FromStr` and `ToRust` and adds `artic::core::text_value!({Self})` (`artic_core::` without the `artic` crate)"
)]
pub trait Written {
  /// How the value is written: [`AsText`], [`AsObject`] or [`AsChild`].
  type As;
  /// [`TakesObject`] when a staged object becomes this value, else [`RefusesObject`].
  type FromObject;
}

/// The value is written as text, which parses into it and is written back as Rust.
#[derive(Debug)]
pub enum AsText {}

/// The value is a UI type, written as an object.
#[derive(Debug)]
pub enum AsObject {}

/// The value is a child element, filled in by the element written inside.
#[derive(Debug)]
pub enum AsChild {}

/// A staged object becomes the value.
#[derive(Debug)]
pub enum TakesObject {}

/// No staged object becomes the value: one given is taken out of the tree and refused.
#[derive(Debug)]
pub enum RefusesObject {}

/// Implements [`Written`] for each type given as one written as text and not taken from an object:
/// a type that implements `FromStr` and [`ToRust`](crate::ToRust).
#[macro_export]
macro_rules! text_value {
  ($($ty:ty),+ $(,)?) => {$(
    impl $crate::Written for $ty {
      type As = $crate::AsText;
      type FromObject = $crate::RefusesObject;
    }
  )+};
}

/// Implements [`Written`] for each type given as a UI type written as an object and not taken from
/// one: a type that implements `UiType` by hand. A value that implements `FromUiObject` writes the
/// impl out instead, with [`AsObject`] and [`TakesObject`].
#[macro_export]
macro_rules! object_value {
  ($($ty:ty),+ $(,)?) => {$(
    impl $crate::Written for $ty {
      type As = $crate::AsObject;
      type FromObject = $crate::RefusesObject;
    }
  )+};
}

impl Written for NodeId {
  type As = AsChild;
  type FromObject = RefusesObject;
}
