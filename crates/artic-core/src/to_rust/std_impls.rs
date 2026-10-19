use crate::{RustExpr, ToRust};
use std::num::{
  NonZeroI8, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI128, NonZeroIsize, NonZeroU8, NonZeroU16, NonZeroU32, NonZeroU64,
  NonZeroU128, NonZeroUsize,
};

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
    out.list(self, |out, item| item.to_rust(out));
    out.raw("]");
  }
}

impl<T: ToRust + ?Sized> ToRust for &T {
  /// Writes the value the reference points to.
  fn to_rust(&self, out: &mut RustExpr) {
    (**self).to_rust(out);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  fn written(value: &(impl ToRust + ?Sized)) -> String {
    let mut out = RustExpr::new();
    value.to_rust(&mut out);
    out.take()
  }

  // Floats write through `RustExpr`, so they read back exactly.
  #[test]
  fn floats_write_literals() {
    assert_eq!(written(&4.0_f32), "4.0_f32");
    assert_eq!(written(&-1.5_f64), "-1.5_f64");
  }

  // A reference writes what it points to, so a call can take a slice as one argument.
  #[test]
  fn reference_writes_its_target() {
    let items: &[u8] = &[1, 2];
    assert_eq!(written(&items), "[1_u8, 2_u8]");
  }

  // A `String` is built from its escaped text.
  #[test]
  fn string_writes_from() {
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
