//! [`ToRust`]: values that write the Rust expression that builds them, for code generated at
//! build time.

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::num::{
  NonZeroI8, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI128, NonZeroIsize, NonZeroU8, NonZeroU16, NonZeroU32, NonZeroU64,
  NonZeroU128, NonZeroUsize,
};

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

/// The expression being written, and the paths generated code reaches crates by.
///
/// A string builder: it checks nothing, so the code generator parses what it writes.
#[derive(Debug, Clone, Default)]
pub struct RustExpr {
  text: String,
  paths: BTreeMap<String, String>,
}

impl RustExpr {
  /// An empty expression that reaches every crate at `::<crate>`.
  #[must_use]
  pub fn new() -> RustExpr {
    RustExpr::default()
  }

  /// Generated code reaches `krate` as `path`, such as `artic_layout` as `::artic::layout`.
  pub fn path(&mut self, krate: &str, path: &str) {
    self.paths.insert(krate.to_owned(), path.to_owned());
  }

  /// Writes the path of an item at `path` in crate `krate`, such as `Thickness` in
  /// `artic_layout`, as generated code reaches it: `::artic::layout::Thickness`.
  pub fn type_path(&mut self, krate: &str, path: &str) {
    match self.paths.get(krate) {
      Some(prefix) => self.text.push_str(prefix),
      None => {
        self.text.push_str("::");
        self.text.push_str(krate);
      }
    }
    self.text.push_str("::");
    self.text.push_str(path);
  }

  /// Writes `value` as a literal that reads back to the same bits: `4.0_f32`, `-0.0_f32`,
  /// `f32::INFINITY`, `f32::NAN`, and any other NaN as `f32::from_bits(…)`.
  pub fn f32(&mut self, value: f32) {
    if value.to_bits() == f32::NAN.to_bits() {
      self.raw("f32::NAN");
    } else if value.is_nan() {
      self.raw(&format!("f32::from_bits({}_u32)", value.to_bits()));
    } else if value == f32::INFINITY {
      self.raw("f32::INFINITY");
    } else if value == f32::NEG_INFINITY {
      self.raw("f32::NEG_INFINITY");
    } else {
      self.suffixed(value, "f32");
    }
  }

  /// Writes `value` as a literal that reads back to the same bits, as [`f32`](RustExpr::f32)
  /// does.
  pub fn f64(&mut self, value: f64) {
    if value.to_bits() == f64::NAN.to_bits() {
      self.raw("f64::NAN");
    } else if value.is_nan() {
      self.raw(&format!("f64::from_bits({}_u64)", value.to_bits()));
    } else if value == f64::INFINITY {
      self.raw("f64::INFINITY");
    } else if value == f64::NEG_INFINITY {
      self.raw("f64::NEG_INFINITY");
    } else {
      self.suffixed(value, "f64");
    }
  }

  /// Writes `text` as an escaped string literal.
  pub fn str(&mut self, text: &str) {
    self.text.push_str(&format!("{text:?}"));
  }

  /// Writes Rust source as is, such as `::new(` or `, `.
  pub fn raw(&mut self, source: &str) {
    self.text.push_str(source);
  }

  /// The expression written so far.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.text
  }

  /// Returns the expression and starts a new one, keeping the paths.
  pub fn take(&mut self) -> String {
    std::mem::take(&mut self.text)
  }

  /// Writes a float's shortest text that reads back exactly, with `suffix`.
  fn suffixed(&mut self, value: impl Debug, suffix: &str) {
    self.text.push_str(&format!("{value:?}_{suffix}"));
  }
}

macro_rules! integers {
  ($($ty:ident)*) => {$(
    impl ToRust for $ty {
      fn to_rust(&self, out: &mut RustExpr) {
        out.raw(&format!("{self}_{}", stringify!($ty)));
      }
    }
  )*};
}

integers!(i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize);

macro_rules! non_zero {
  ($($ty:ident($int:ident))*) => {$(
    impl ToRust for $ty {
      fn to_rust(&self, out: &mut RustExpr) {
        out.raw(&format!("::core::num::{}::new({self}_{}).unwrap()", stringify!($ty), stringify!($int)));
      }
    }
  )*};
}

non_zero!(
  NonZeroI8(i8) NonZeroI16(i16) NonZeroI32(i32) NonZeroI64(i64) NonZeroI128(i128) NonZeroIsize(isize)
  NonZeroU8(u8) NonZeroU16(u16) NonZeroU32(u32) NonZeroU64(u64) NonZeroU128(u128) NonZeroUsize(usize)
);

impl ToRust for f32 {
  fn to_rust(&self, out: &mut RustExpr) {
    out.f32(*self);
  }
}

impl ToRust for f64 {
  fn to_rust(&self, out: &mut RustExpr) {
    out.f64(*self);
  }
}

impl ToRust for bool {
  fn to_rust(&self, out: &mut RustExpr) {
    out.raw(if *self { "true" } else { "false" });
  }
}

impl ToRust for char {
  fn to_rust(&self, out: &mut RustExpr) {
    out.raw(&format!("{self:?}"));
  }
}

impl ToRust for String {
  fn to_rust(&self, out: &mut RustExpr) {
    out.raw("::std::string::String::from(");
    out.str(self);
    out.raw(")");
  }
}

impl<T: ToRust> ToRust for [T] {
  /// Writes an array literal of the items.
  fn to_rust(&self, out: &mut RustExpr) {
    out.raw("[");
    for (index, item) in self.iter().enumerate() {
      if index > 0 {
        out.raw(", ");
      }
      item.to_rust(out);
    }
    out.raw("]");
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use proptest::prelude::*;
  use std::num::{NonZeroI64, NonZeroU32};

  fn written(value: &(impl ToRust + ?Sized)) -> String {
    let mut out = RustExpr::new();
    value.to_rust(&mut out);
    out.take()
  }

  // An item's crate is reached at `::<crate>` when nothing maps it.
  #[test]
  fn type_path_uses_the_crate_root() {
    let mut out = RustExpr::new();
    out.type_path("artic_layout", "Thickness");
    assert_eq!(out.as_str(), "::artic_layout::Thickness");
  }

  // A mapped crate is reached by its path, such as a facade module.
  #[test]
  fn type_path_uses_a_mapped_path() {
    let mut out = RustExpr::new();
    out.path("artic_layout", "::artic::layout");
    out.type_path("artic_layout", "Thickness");
    assert_eq!(out.as_str(), "::artic::layout::Thickness");
  }

  // `take` returns the text and leaves the map, so one `RustExpr` writes many expressions.
  #[test]
  fn take_keeps_the_paths() {
    let mut out = RustExpr::new();
    out.path("artic_layout", "::artic::layout");
    out.type_path("artic_layout", "Point");
    assert_eq!(out.take(), "::artic::layout::Point");
    out.type_path("artic_layout", "Size");
    assert_eq!(out.as_str(), "::artic::layout::Size");
  }

  // NaN, the infinities and negative zero are written as constants or signed literals.
  #[test]
  fn f32_writes_special_values() {
    assert_eq!(written(&f32::NAN), "f32::NAN");
    assert_eq!(written(&f32::INFINITY), "f32::INFINITY");
    assert_eq!(written(&f32::NEG_INFINITY), "f32::NEG_INFINITY");
    assert_eq!(written(&-0.0_f32), "-0.0_f32");
    assert_eq!(written(&4.0_f32), "4.0_f32");
    assert_eq!(written(&-1.5_f64), "-1.5_f64");
  }

  // A NaN other than the constant, such as the one `"-NaN"` parses to, keeps its bits.
  #[test]
  fn other_nans_write_their_bits() {
    let negative = "-NaN".parse::<f32>().expect("a float");
    assert_eq!(written(&negative), format!("f32::from_bits({}_u32)", negative.to_bits()));
    let negative = "-NaN".parse::<f64>().expect("a float");
    assert_eq!(written(&negative), format!("f64::from_bits({}_u64)", negative.to_bits()));
  }

  proptest! {
    // Every finite f32 is written as a literal that reads back to the same bits.
    #[test]
    fn f32_reads_back_exactly(bits in any::<u32>()) {
      let value = f32::from_bits(bits);
      prop_assume!(value.is_finite());
      let text = written(&value);
      let literal = text.strip_suffix("_f32").expect("an f32 suffix");
      prop_assert_eq!(literal.parse::<f32>().expect("a float").to_bits(), bits);
    }
  }

  // Strings are escaped so quotes, backslashes and newlines survive.
  #[test]
  fn str_escapes_text() {
    let mut out = RustExpr::new();
    out.str("a\"b\\c\nd é");
    assert_eq!(out.as_str(), r#""a\"b\\c\nd é""#);
    assert_eq!(written(&String::from("hi")), r#"::std::string::String::from("hi")"#);
  }

  // Integers carry their type's suffix, and the minimum values are written as negated literals.
  #[test]
  fn integers_carry_their_suffix() {
    assert_eq!(written(&-5_i32), "-5_i32");
    assert_eq!(written(&i32::MIN), "-2147483648_i32");
    assert_eq!(written(&7_usize), "7_usize");
    assert_eq!(written(&255_u8), "255_u8");
  }

  // `NonZero` integers are built with `new`, the only safe constructor.
  #[test]
  fn non_zero_writes_new() {
    assert_eq!(
      written(&NonZeroU32::new(3).expect("non-zero")),
      "::core::num::NonZeroU32::new(3_u32).unwrap()"
    );
    assert_eq!(
      written(&NonZeroI64::new(-2).expect("non-zero")),
      "::core::num::NonZeroI64::new(-2_i64).unwrap()"
    );
  }

  // Booleans, characters and slices write their literals.
  #[test]
  fn other_primitives_write_literals() {
    assert_eq!(written(&true), "true");
    assert_eq!(written(&'\''), r"'\''");
    assert_eq!(written(&[1_u8, 2_u8][..]), "[1_u8, 2_u8]");
    assert_eq!(written(&[0_u8; 0][..]), "[]");
  }
}
