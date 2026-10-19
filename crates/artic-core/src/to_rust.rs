//! [`ToRust`]: values that write the Rust expression that builds them, for code generated at
//! build time.
//!
//! - [`ToRust`]: the trait, implemented here for the standard types a value can hold.
//! - [`RustExpr`]: the string builder an impl writes into.

mod rust_expr;
mod std_impls;

pub use rust_expr::RustExpr;

/// A value that can write the Rust expression that builds it, for code generated at build time.
///
/// A value type set from text implements it beside its `FromStr`, so a build script parses the
/// text once and writes the value into the code it generates. An impl names its own type with
/// [`RustExpr::type_path`] and `env!("CARGO_CRATE_NAME")`, and standard types in full with
/// [`RustExpr::raw`].
pub trait ToRust {
  /// Writes an expression that evaluates to this value.
  fn to_rust(&self, out: &mut RustExpr);
}
