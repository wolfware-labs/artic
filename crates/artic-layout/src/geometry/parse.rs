//! Parsing geometry from text, such as `Margin="4,8"`.
//!
//! Numbers are separated by a comma, by whitespace, or by both: `"4,8"`, `"4 8"` and `"4, 8"` are
//! the same. Whitespace around the text is ignored. An empty value, such as two commas in a row or
//! a leading or trailing comma, is an error, and so are NaN and infinity. Percentages are read only
//! by the relative values, such as `RelativePoint`; the rest reject them.

use crate::finite::{self, NumberProblem};
use crate::geometry::{CornerRadius, Point, Rect, RelativePoint, RelativeScalar, Scaling, Size, Thickness, Transform};
use artic_core::{RustExpr, ToRust};
use std::str::FromStr;

/// The error returned when text isn't a valid geometry value.
///
/// The message repeats the text, says what is wrong, and lists the forms the type accepts.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("invalid {type_name} '{text}': {problem}; expected {forms}")]
pub struct ParseGeometryError {
  /// The type being parsed, such as `Thickness`.
  pub type_name: &'static str,
  /// The text that failed to parse.
  pub text: String,
  /// What is wrong with it.
  pub problem: GeometryProblem,
  /// The accepted forms, as examples.
  pub forms: &'static str,
}

/// What is wrong with geometry text.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GeometryProblem {
  /// A value between separators is missing.
  #[error("a value is empty")]
  Empty,
  /// A value isn't a finite number.
  #[error(transparent)]
  Number(#[from] NumberProblem),
  /// A value that must not be negative is.
  #[error("'{0}' is negative")]
  Negative(String),
  /// The text has a number of values the type doesn't accept.
  #[error("it has {0} values")]
  Count(usize),
  /// A transform function artic doesn't know, or text that isn't a function.
  #[error("'{0}' is not translate(x, y), rotate(degrees) or scale(x, y)")]
  UnknownFunction(String),
  /// A point mixes percentages with logical units.
  #[error("it mixes percentages and fixed values")]
  MixedUnits,
  /// A `%` that doesn't follow a number.
  #[error("'{0}' is not a percentage")]
  NotAPercent(String),
}

impl FromStr for Thickness {
  type Err = ParseGeometryError;

  /// Parses `"4"` (every side), `"4,8"` (left and right, then top and bottom) or `"1,2,3,4"`
  /// (left, top, right, bottom). Negative values are allowed.
  fn from_str(text: &str) -> Result<Self, Self::Err> {
    parse(
      text,
      "Thickness",
      r#""4", "4,8" or "1,2,3,4""#,
      Sign::Any,
      |numbers| match *numbers {
        [width] => Some(Thickness::uniform(width)),
        [horizontal, vertical] => Some(Thickness::symmetric(horizontal, vertical)),
        [left, top, right, bottom] => Some(Thickness::new(left, top, right, bottom)),
        _ => None,
      },
    )
  }
}

impl FromStr for CornerRadius {
  type Err = ParseGeometryError;

  /// Parses `"4"` (every corner) or `"1,2,3,4"` (top-left, top-right, bottom-right, bottom-left).
  /// Radii can't be negative.
  fn from_str(text: &str) -> Result<Self, Self::Err> {
    parse(
      text,
      "CornerRadius",
      r#""4" or "1,2,3,4""#,
      Sign::NotNegative,
      |numbers| match *numbers {
        [radius] => Some(CornerRadius::uniform(radius)),
        [top_left, top_right, bottom_right, bottom_left] => {
          Some(CornerRadius::new(top_left, top_right, bottom_right, bottom_left))
        }
        _ => None,
      },
    )
  }
}

impl FromStr for Point {
  type Err = ParseGeometryError;

  /// Parses `"x,y"`. Negative values are allowed.
  fn from_str(text: &str) -> Result<Self, Self::Err> {
    parse(text, "Point", r#""x,y""#, Sign::Any, |numbers| match *numbers {
      [x, y] => Some(Point::new(x, y)),
      _ => None,
    })
  }
}

impl FromStr for Size {
  type Err = ParseGeometryError;

  /// Parses `"width,height"`. Neither can be negative.
  fn from_str(text: &str) -> Result<Self, Self::Err> {
    parse(
      text,
      "Size",
      r#""width,height""#,
      Sign::NotNegative,
      |numbers| match *numbers {
        [width, height] => Some(Size::new(width, height)),
        _ => None,
      },
    )
  }
}

impl FromStr for Rect {
  type Err = ParseGeometryError;

  /// Parses `"x,y,width,height"`. The position can be negative; the width and height can't.
  fn from_str(text: &str) -> Result<Self, Self::Err> {
    let forms = r#""x,y,width,height""#;
    let rect = parse(text, "Rect", forms, Sign::Any, |numbers| match *numbers {
      [x, y, width, height] => Some(Rect::new(x, y, width, height)),
      _ => None,
    })?;
    for extent in [rect.width, rect.height] {
      if extent < 0.0 {
        return Err(error(text, "Rect", forms, GeometryProblem::Negative(extent.to_string())));
      }
    }
    Ok(rect)
  }
}

impl FromStr for Transform {
  type Err = ParseGeometryError;

  /// Parses CSS transform functions: `translate(x, y)` or `translate(x)`, `rotate(degrees)`,
  /// `scale(s)` or `scale(x, y)`, such as `"translate(10, 20) rotate(45)"`. As in CSS, whitespace
  /// between functions is optional, none may come between a name and its parenthesis, and the
  /// rightmost function applies to the element first.
  fn from_str(text: &str) -> Result<Self, Self::Err> {
    const FORMS: &str = r#""translate(10, 20)", "rotate(45)", "scale(2)" or several, such as "translate(10, 0) rotate(45)""#;
    let fail = |problem| error(text, "Transform", FORMS, problem);
    let mut rest = text.trim();
    if rest.is_empty() {
      return Err(fail(GeometryProblem::Empty));
    }
    let mut functions = Vec::new();
    while !rest.is_empty() {
      let unknown = || fail(GeometryProblem::UnknownFunction(rest.to_owned()));
      let (name, after) = rest.split_once('(').ok_or_else(unknown)?;
      let (numbers, tail) = after.split_once(')').ok_or_else(unknown)?;
      let numbers = read_numbers(numbers, Sign::Any).map_err(fail)?;
      let function = match (name, numbers.as_slice()) {
        ("translate", [x]) => Transform::translate(*x, 0.0),
        ("translate", [x, y]) => Transform::translate(*x, *y),
        ("rotate", [degrees]) => Transform::rotate(degrees.to_radians()),
        ("scale", [both]) => Transform::scale(*both, *both),
        ("scale", [x, y]) => Transform::scale(*x, *y),
        ("translate" | "rotate" | "scale", numbers) => return Err(fail(GeometryProblem::Count(numbers.len()))),
        (other, _) => return Err(fail(GeometryProblem::UnknownFunction(other.to_owned()))),
      };
      functions.push(function);
      rest = tail.trim_start();
    }
    let rightmost_first = functions.into_iter().rev().reduce(Transform::then);
    Ok(rightmost_first.expect("the text isn't empty, so it has a function"))
  }
}

impl FromStr for RelativePoint {
  type Err = ParseGeometryError;

  /// Parses `"x,y"` as percentages of the box, such as `"50%,25%"`, or as logical units from its
  /// top-left corner, such as `"10,20"`. The two can't be mixed.
  fn from_str(text: &str) -> Result<Self, Self::Err> {
    let fail = |problem| error(text, "RelativePoint", r#""50%,25%" or "10,20""#, problem);
    let (numbers, scaling) = read_relative(text).map_err(fail)?;
    match *numbers {
      [x, y] => Ok(RelativePoint {
        point: Point::new(x, y),
        scaling,
      }),
      _ => Err(fail(GeometryProblem::Count(numbers.len()))),
    }
  }
}

impl FromStr for RelativeScalar {
  type Err = ParseGeometryError;

  /// Parses a percentage of an extent, such as `"50%"`, or logical units, such as `"10"`.
  fn from_str(text: &str) -> Result<Self, Self::Err> {
    let fail = |problem| error(text, "RelativeScalar", r#""50%" or "10""#, problem);
    let (numbers, scaling) = read_relative(text).map_err(fail)?;
    match *numbers {
      [value] => Ok(RelativeScalar { value, scaling }),
      _ => Err(fail(GeometryProblem::Count(numbers.len()))),
    }
  }
}

impl ToRust for Thickness {
  fn to_rust(&self, out: &mut RustExpr) {
    out.call(
      env!("CARGO_CRATE_NAME"),
      "Thickness::new",
      &[&self.left, &self.top, &self.right, &self.bottom],
    );
  }
}

impl ToRust for CornerRadius {
  fn to_rust(&self, out: &mut RustExpr) {
    out.call(
      env!("CARGO_CRATE_NAME"),
      "CornerRadius::new",
      &[&self.top_left, &self.top_right, &self.bottom_right, &self.bottom_left],
    );
  }
}

impl ToRust for Point {
  fn to_rust(&self, out: &mut RustExpr) {
    out.call(env!("CARGO_CRATE_NAME"), "Point::new", &[&self.x, &self.y]);
  }
}

impl ToRust for Size {
  fn to_rust(&self, out: &mut RustExpr) {
    out.call(env!("CARGO_CRATE_NAME"), "Size::new", &[&self.width, &self.height]);
  }
}

impl ToRust for Rect {
  fn to_rust(&self, out: &mut RustExpr) {
    out.call(
      env!("CARGO_CRATE_NAME"),
      "Rect::new",
      &[&self.x, &self.y, &self.width, &self.height],
    );
  }
}

impl ToRust for Transform {
  /// Writes a struct literal: a rotation's matrix has no shorter constructor.
  fn to_rust(&self, out: &mut RustExpr) {
    let Transform { xx, yx, xy, yy, dx, dy } = self;
    out.struct_literal(
      env!("CARGO_CRATE_NAME"),
      "Transform",
      &[("xx", xx), ("yx", yx), ("xy", xy), ("yy", yy), ("dx", dx), ("dy", dy)],
    );
  }
}

impl ToRust for RelativePoint {
  fn to_rust(&self, out: &mut RustExpr) {
    let path = match self.scaling {
      Scaling::Relative => "RelativePoint::relative",
      Scaling::Fixed => "RelativePoint::fixed",
    };
    out.call(env!("CARGO_CRATE_NAME"), path, &[&self.point.x, &self.point.y]);
  }
}

impl ToRust for RelativeScalar {
  fn to_rust(&self, out: &mut RustExpr) {
    let path = match self.scaling {
      Scaling::Relative => "RelativeScalar::relative",
      Scaling::Fixed => "RelativeScalar::fixed",
    };
    out.call(env!("CARGO_CRATE_NAME"), path, &[&self.value]);
  }
}

/// Whether the values may be negative.
#[derive(Clone, Copy, PartialEq)]
enum Sign {
  Any,
  NotNegative,
}

/// Reads the numbers in `text` and builds the value with `build`, which returns `None` for a count
/// of numbers the type doesn't accept.
fn parse<T>(
  text: &str,
  type_name: &'static str,
  forms: &'static str,
  sign: Sign,
  build: impl FnOnce(&[f32]) -> Option<T>,
) -> Result<T, ParseGeometryError> {
  let numbers = read_numbers(text, sign).map_err(|problem| error(text, type_name, forms, problem))?;
  build(&numbers).ok_or_else(|| error(text, type_name, forms, GeometryProblem::Count(numbers.len())))
}

fn read_numbers(text: &str, sign: Sign) -> Result<Vec<f32>, GeometryProblem> {
  values(text)?.into_iter().map(|value| number(value, sign)).collect()
}

fn values(text: &str) -> Result<Vec<&str>, GeometryProblem> {
  let mut values = Vec::new();
  for segment in text.split(',') {
    let before = values.len();
    values.extend(segment.split_whitespace());
    if values.len() == before {
      return Err(GeometryProblem::Empty);
    }
  }
  Ok(values)
}

fn number(text: &str, sign: Sign) -> Result<f32, GeometryProblem> {
  let number = finite::parse(text)?;
  if sign == Sign::NotNegative && number < 0.0 {
    return Err(GeometryProblem::Negative(text.to_owned()));
  }
  Ok(number)
}

fn read_relative(text: &str) -> Result<(Vec<f32>, Scaling), GeometryProblem> {
  let mut numbers = Vec::new();
  let mut scaling = None;
  for value in values(text)? {
    let (digits, this) = match value.strip_suffix('%') {
      Some(digits) if digits.is_empty() || digits.ends_with('%') => {
        return Err(GeometryProblem::NotAPercent(value.to_owned()));
      }
      Some(digits) => (digits, Scaling::Relative),
      None => (value, Scaling::Fixed),
    };
    let number = number(digits, Sign::Any)?;
    if *scaling.get_or_insert(this) != this {
      return Err(GeometryProblem::MixedUnits);
    }
    numbers.push(match this {
      Scaling::Relative => number / 100.0,
      Scaling::Fixed => number,
    });
  }
  Ok((numbers, scaling.expect("values returns at least one value")))
}

fn error(text: &str, type_name: &'static str, forms: &'static str, problem: GeometryProblem) -> ParseGeometryError {
  ParseGeometryError {
    type_name,
    text: text.to_owned(),
    problem,
    forms,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  /// Returns the problem `text` has as a `T`, panicking if it parses.
  fn problem<T: FromStr<Err = ParseGeometryError> + std::fmt::Debug>(text: &str) -> GeometryProblem {
    text.parse::<T>().expect_err("the text is invalid").problem
  }

  // Every Thickness form parses, with commas, spaces or both.
  #[test]
  fn thickness_accepts_one_two_or_four_values() {
    for text in ["4", " 4 "] {
      assert_eq!(text.parse(), Ok(Thickness::uniform(4.0)), "{text}");
    }
    for text in ["4,8", "4 8", "4, 8"] {
      assert_eq!(text.parse(), Ok(Thickness::symmetric(4.0, 8.0)), "{text}");
    }
    for text in ["1,2,3,4", "1 2 3 4", "1, 2 3,4"] {
      assert_eq!(text.parse(), Ok(Thickness::new(1.0, 2.0, 3.0, 4.0)), "{text}");
    }
    assert_eq!("-4,0.5".parse(), Ok(Thickness::symmetric(-4.0, 0.5)));
  }

  // Every CornerRadius form parses, with commas or spaces.
  #[test]
  fn corner_radius_accepts_one_or_four_values() {
    assert_eq!("4".parse(), Ok(CornerRadius::uniform(4.0)));
    for text in ["1,2,3,4", "1 2 3 4"] {
      assert_eq!(text.parse(), Ok(CornerRadius::new(1.0, 2.0, 3.0, 4.0)), "{text}");
    }
  }

  // Point, Size and Rect parse their single form.
  #[test]
  fn point_size_and_rect_parse() {
    assert_eq!("-1.5, 2".parse(), Ok(Point::new(-1.5, 2.0)));
    assert_eq!("30 40".parse(), Ok(Size::new(30.0, 40.0)));
    assert_eq!("-1,2,30,40".parse(), Ok(Rect::new(-1.0, 2.0, 30.0, 40.0)));
  }

  // Empty text and empty values between separators are rejected.
  #[test]
  fn empty_values_are_rejected() {
    for text in ["", "   ", "4,,8", ",4", "4,"] {
      assert_eq!(problem::<Thickness>(text), GeometryProblem::Empty, "{text:?}");
    }
  }

  // A count of values the type doesn't accept is rejected.
  #[test]
  fn wrong_counts_are_rejected() {
    assert_eq!(problem::<Thickness>("1,2,3"), GeometryProblem::Count(3));
    assert_eq!(problem::<CornerRadius>("1,2"), GeometryProblem::Count(2));
    assert_eq!(problem::<Point>("1"), GeometryProblem::Count(1));
  }

  // Non-numbers, NaN and infinity are rejected.
  #[test]
  fn non_numbers_and_non_finite_values_are_rejected() {
    assert_eq!(
      problem::<Thickness>("abc"),
      GeometryProblem::Number(NumberProblem::NotANumber("abc".to_owned()))
    );
    assert_eq!(
      problem::<Thickness>("4,NaN"),
      GeometryProblem::Number(NumberProblem::NotFinite("NaN".to_owned()))
    );
    assert_eq!(
      problem::<Size>("inf,4"),
      GeometryProblem::Number(NumberProblem::NotFinite("inf".to_owned()))
    );
    assert_eq!(
      problem::<Point>("50%,50%"),
      GeometryProblem::Number(NumberProblem::NotANumber("50%".to_owned()))
    );
    assert_eq!(
      "4,NaN".parse::<Thickness>().unwrap_err().to_string(),
      r#"invalid Thickness '4,NaN': 'NaN' is not a finite number; expected "4", "4,8" or "1,2,3,4""#
    );
  }

  // Sizes, radii and a rectangle's extent can't be negative.
  #[test]
  fn negative_extents_are_rejected() {
    assert_eq!(problem::<Size>("-1,4"), GeometryProblem::Negative("-1".to_owned()));
    assert_eq!(problem::<CornerRadius>("-2"), GeometryProblem::Negative("-2".to_owned()));
    assert_eq!(problem::<Rect>("0,0,-3,4"), GeometryProblem::Negative("-3".to_owned()));
  }

  // The message repeats the text, says what is wrong, and lists the accepted forms.
  #[test]
  fn error_message_names_the_expected_forms() {
    let error = "1,2,3".parse::<Thickness>().expect_err("three values");

    assert_eq!(
      error.to_string(),
      r#"invalid Thickness '1,2,3': it has 3 values; expected "4", "4,8" or "1,2,3,4""#
    );
  }

  // A point is all percentages, read as fractions of the box, or all logical units.
  #[test]
  fn relative_point_parses_percent_and_fixed() {
    assert_eq!("50%,25%".parse(), Ok(RelativePoint::relative(0.5, 0.25)));
    assert_eq!(" 100% 0% ".parse(), Ok(RelativePoint::relative(1.0, 0.0)));
    assert_eq!("10, -20".parse(), Ok(RelativePoint::fixed(10.0, -20.0)));
  }

  // A point can't mix percentages with logical units, and the message says so.
  #[test]
  fn relative_point_rejects_mixed_units() {
    assert_eq!(problem::<RelativePoint>("50%,20"), GeometryProblem::MixedUnits);
    assert_eq!(problem::<RelativePoint>("10 25%"), GeometryProblem::MixedUnits);
    assert_eq!(
      "50%,20".parse::<RelativePoint>().expect_err("mixed").to_string(),
      r#"invalid RelativePoint '50%,20': it mixes percentages and fixed values; expected "50%,25%" or "10,20""#
    );
  }

  // A scalar is a percentage, read as a fraction, or logical units.
  #[test]
  fn relative_scalar_parses_percent_and_fixed() {
    assert_eq!("50%".parse(), Ok(RelativeScalar::relative(0.5)));
    assert_eq!(" 12.5 ".parse(), Ok(RelativeScalar::fixed(12.5)));
  }

  // Empty text, a second value, and a `%` that doesn't follow a number are rejected.
  #[test]
  fn relative_scalar_rejects_invalid() {
    assert_eq!(problem::<RelativeScalar>(""), GeometryProblem::Empty);
    assert_eq!(problem::<RelativeScalar>("10 20"), GeometryProblem::Count(2));
    assert_eq!(problem::<RelativeScalar>("%"), GeometryProblem::NotAPercent("%".to_owned()));
    assert_eq!(
      problem::<RelativeScalar>("50%%"),
      GeometryProblem::NotAPercent("50%%".to_owned())
    );
    assert_eq!(
      problem::<RelativeScalar>("abc%"),
      GeometryProblem::Number(NumberProblem::NotANumber("abc".to_owned()))
    );
  }

  /// Where `transform` sends `point`, rounded to hundredths to absorb `f32` error.
  fn at(transform: Transform, x: f32, y: f32) -> (f32, f32) {
    let point = transform.apply(Point::new(x, y));
    ((point.x * 100.0).round() / 100.0, (point.y * 100.0).round() / 100.0)
  }

  // Each function parses, with one or two numbers where CSS allows.
  #[test]
  fn transform_functions_parse() {
    assert_eq!(at("translate(10, 20)".parse().unwrap(), 1.0, 1.0), (11.0, 21.0));
    assert_eq!(at("translate(10)".parse().unwrap(), 1.0, 1.0), (11.0, 1.0));
    assert_eq!(at("scale(2)".parse().unwrap(), 3.0, 4.0), (6.0, 8.0));
    assert_eq!(at("scale(2 3)".parse().unwrap(), 3.0, 4.0), (6.0, 12.0));
    assert_eq!(at("rotate(90)".parse().unwrap(), 1.0, 0.0), (0.0, 1.0));
  }

  // As in CSS, whitespace between functions is optional, but none may come between a function's
  // name and its parenthesis.
  #[test]
  fn transform_whitespace_follows_css() {
    let spaced: Transform = "translate(100, 0) rotate(90)".parse().unwrap();
    let packed: Transform = "translate(100, 0)rotate(90)".parse().unwrap();

    assert_eq!(at(packed, 1.0, 0.0), at(spaced, 1.0, 0.0));
    assert_eq!(
      "rotate (90)".parse::<Transform>().unwrap_err().problem,
      GeometryProblem::UnknownFunction("rotate ".to_owned())
    );
  }

  // As in CSS, the rightmost function applies to the element first: this rotates, then moves.
  #[test]
  fn transform_functions_compose_like_css() {
    let transform: Transform = "translate(100, 0) rotate(90)".parse().unwrap();

    assert_eq!(at(transform, 1.0, 0.0), (100.0, 1.0));
  }

  // Unknown functions, wrong argument counts, bad numbers and empty text are errors.
  #[test]
  fn invalid_transforms_are_errors() {
    let problem = |text: &str| text.parse::<Transform>().unwrap_err().problem;

    assert_eq!(problem("skew(10)"), GeometryProblem::UnknownFunction("skew".to_owned()));
    assert_eq!(problem("rotate(1, 2)"), GeometryProblem::Count(2));
    assert_eq!(
      problem("translate(a, 2)"),
      GeometryProblem::Number(NumberProblem::NotANumber("a".to_owned()))
    );
    assert_eq!(
      problem("translate(1, 2"),
      GeometryProblem::UnknownFunction("translate(1, 2".to_owned())
    );
    assert_eq!(problem("  "), GeometryProblem::Empty);
  }

  // A stray parenthesis is an error naming the text before the next `(`, or the whole rest when
  // no `(` or `)` follows.
  #[test]
  fn stray_parentheses_are_errors() {
    let problem = |text: &str| text.parse::<Transform>().unwrap_err().problem;

    assert_eq!(problem(")rotate(45)"), GeometryProblem::UnknownFunction(")rotate".to_owned()));
    assert_eq!(
      problem("rotate)45("),
      GeometryProblem::UnknownFunction("rotate)45(".to_owned())
    );
    assert_eq!(problem("rotate(45"), GeometryProblem::UnknownFunction("rotate(45".to_owned()));
    assert_eq!(
      problem("translate(1)) scale(2)"),
      GeometryProblem::UnknownFunction(") scale".to_owned())
    );
  }
}
