// Value types written as Rust: each case checks that `to_rust` writes the expression given, and,
// by compiling that expression here, that it builds the same value. This crate depends on
// artic-layout and artic-render, so one file covers every crate's value types.

use artic_core::{RustExpr, ToRust};
use artic_layout::{
  CornerRadius, FontFamily, FontStyle, FontWeight, Orientation, Point, Rect, RelativePoint, RelativeScalar, Size, TextWrapping,
  Thickness, Transform,
};
use pretty_assertions::assert_eq;
use std::fmt::Debug;
use std::num::NonZeroU32;
use std::str::FromStr;

fn parse<T: FromStr<Err: Debug>>(text: &str) -> T {
  text.parse().expect("the text parses")
}

// Checks that `$value` writes `$expected`, compared as parsed Rust, and that `$expected`
// compiled here equals `$value`.
macro_rules! assert_writes {
  ($value:expr => $($expected:tt)+) => {{
    let value = $value;
    let mut out = RustExpr::new();
    value.to_rust(&mut out);
    let written: syn::Expr = syn::parse_str(out.as_str()).expect("to_rust writes an expression");
    let expected: syn::Expr = syn::parse_quote!($($expected)+);
    assert_eq!(written, expected);
    assert_eq!($($expected)+, value);
  }};
}

// Numbers and NonZero integers compile to the values they were written from.
#[test]
fn primitives() {
  assert_writes!(parse::<f32>("0.1") => 0.1_f32);
  assert_writes!(parse::<i32>("-2147483648") => -2147483648_i32);
  assert_writes!(parse::<NonZeroU32>("3") => ::core::num::NonZeroU32::new(3_u32).unwrap());
  assert_writes!(parse::<String>("say \"hi\"\n") => ::std::string::String::from("say \"hi\"\n"));
}

// Geometry writes the constructor its fields came from.
#[test]
fn geometry() {
  assert_writes!(parse::<Thickness>("4,8") => ::artic_layout::Thickness::new(4.0_f32, 8.0_f32, 4.0_f32, 8.0_f32));
  assert_writes!(parse::<CornerRadius>("1,2,3,4") => ::artic_layout::CornerRadius::new(1.0_f32, 2.0_f32, 3.0_f32, 4.0_f32));
  assert_writes!(parse::<Point>("-1.5,2") => ::artic_layout::Point::new(-1.5_f32, 2.0_f32));
  assert_writes!(parse::<Size>("10,20") => ::artic_layout::Size::new(10.0_f32, 20.0_f32));
  assert_writes!(parse::<Rect>("1,2,3,4") => ::artic_layout::Rect::new(1.0_f32, 2.0_f32, 3.0_f32, 4.0_f32));
}

// A transform writes its matrix as a struct literal.
#[test]
fn transform() {
  assert_writes!(parse::<Transform>("translate(10, 20) scale(2)") => ::artic_layout::Transform {
    xx: 2.0_f32, yx: 0.0_f32, xy: 0.0_f32, yy: 2.0_f32, dx: 10.0_f32, dy: 20.0_f32
  });
}

// Relative values write the constructor of their scaling.
#[test]
fn relative_values() {
  assert_writes!(parse::<RelativePoint>("50%,25%") => ::artic_layout::RelativePoint::relative(0.5_f32, 0.25_f32));
  assert_writes!(parse::<RelativePoint>("10,20") => ::artic_layout::RelativePoint::fixed(10.0_f32, 20.0_f32));
  assert_writes!(parse::<RelativeScalar>("50%") => ::artic_layout::RelativeScalar::relative(0.5_f32));
  assert_writes!(parse::<RelativeScalar>("10") => ::artic_layout::RelativeScalar::fixed(10.0_f32));
}

// A font family writes its names, escaped, in order.
#[test]
fn font_family() {
  assert_writes!(parse::<FontFamily>("'Noto \"Sans\"', sans-serif") => ::artic_layout::FontFamily::from_names([
    ::artic_layout::FamilyName::Named(::std::sync::Arc::from("Noto \"Sans\"")),
    ::artic_layout::FamilyName::Generic(::artic_layout::GenericFamily::SansSerif)
  ]));
}

// A named weight writes its constant; any other weight writes `new`.
#[test]
fn font_weight() {
  assert_writes!(parse::<FontWeight>("Bold") => ::artic_layout::FontWeight::BOLD);
  assert_writes!(parse::<FontWeight>("450") => ::artic_layout::FontWeight::new(450_u16).unwrap());
}

// Enums write their variant's path at the crate root.
#[test]
fn layout_enums() {
  assert_writes!(parse::<Orientation>("vertical") => ::artic_layout::Orientation::Vertical);
  assert_writes!(parse::<TextWrapping>("Wrap") => ::artic_layout::TextWrapping::Wrap);
  assert_writes!(parse::<FontStyle>("Italic") => ::artic_layout::FontStyle::Italic);
}
