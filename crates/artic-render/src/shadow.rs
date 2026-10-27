use crate::{Color, ParseColorError, Shape};
use artic_core::{RustExpr, ToRust};
use artic_layout::finite::{self, NumberProblem};
use artic_layout::geometry::{CornerRadius, Point, Rect, RoundedRect};
use std::str::FromStr;

/// A blurred copy of a rounded rectangle, drawn behind it and only outside it: CSS's outer
/// `box-shadow`.
///
/// The rectangle is grown by `spread`, moved by `offset` and blurred by `blur`, in `color`. It's
/// built with [`new`](BoxShadow::new) and the `with_*` methods, so fields added later break no
/// caller.
///
/// It parses from text in CSS's form, `offset-x offset-y [blur [spread]] color`, such as
/// `0 2 8 #40000000`.
///
/// # Examples
///
/// ```
/// use artic_layout::geometry::Point;
/// use artic_render::{BoxShadow, Color};
///
/// let shadow = BoxShadow::new(Point::new(0.0, 2.0), 8.0, Color::rgba(0, 0, 0, 64)).with_spread(2.0);
/// assert_eq!(shadow.spread, 2.0);
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct BoxShadow {
  /// How far the shadow is moved from the rectangle that casts it.
  pub offset: Point,
  /// How far the edges fade: twice the standard deviation of the gaussian blur, as in CSS.
  pub blur: f32,
  /// How far the rectangle grows on each side before it's blurred. A negative spread shrinks it.
  pub spread: f32,
  /// The shadow's color.
  pub color: Color,
}

impl BoxShadow {
  /// A shadow moved by `offset` and blurred by `blur`, in `color`, with no spread.
  #[must_use]
  pub fn new(offset: Point, blur: f32, color: Color) -> BoxShadow {
    BoxShadow {
      offset,
      blur,
      spread: 0.0,
      color,
    }
  }

  /// The same shadow with `spread`.
  #[must_use]
  pub fn with_spread(self, spread: f32) -> BoxShadow {
    BoxShadow { spread, ..self }
  }

  /// The shadow as it's drawn, or `None` when it draws nothing: an offset with a NaN or infinite
  /// coordinate. A blur below `0`, NaN or infinite draws as `0`, a sharp shadow, and a NaN or
  /// infinite spread as `0`.
  #[must_use]
  pub fn drawable(&self) -> Option<BoxShadow> {
    (self.offset.x.is_finite() && self.offset.y.is_finite()).then(|| BoxShadow {
      blur: finite::usable(self.blur),
      spread: finite::or_zero(self.spread),
      ..self.clone()
    })
  }

  /// The rounded rectangle this shadow blurs when `rect` casts it, or `None` when it draws
  /// nothing.
  ///
  /// `rect` is made [drawable](Shape::drawable), grown by the spread on each side and moved by
  /// the offset. Each radius above `0` grows by the spread, and the radii are
  /// [fitted](RoundedRect::fitted) again. A negative spread shrinks the rectangle, down to
  /// nothing. The shadow is made [drawable](BoxShadow::drawable) first.
  #[must_use]
  pub fn shape(&self, rect: RoundedRect) -> Option<RoundedRect> {
    let Some(Shape::RoundedRect(caster)) = Shape::from(rect).drawable() else {
      return None;
    };
    let shadow = self.drawable()?;
    let (spread, offset) = (shadow.spread, shadow.offset);
    let Rect { x, y, width, height } = caster.rect;
    let grown = Rect::new(
      x - spread + offset.x,
      y - spread + offset.y,
      width + 2.0 * spread,
      height + 2.0 * spread,
    );
    let grow = |radius: f32| if radius > 0.0 { radius + spread } else { 0.0 };
    let CornerRadius {
      top_left,
      top_right,
      bottom_right,
      bottom_left,
    } = caster.radius;
    let radius = CornerRadius::new(grow(top_left), grow(top_right), grow(bottom_right), grow(bottom_left));
    match Shape::from(RoundedRect::new(grown, radius)).drawable() {
      Some(Shape::RoundedRect(shadow)) if grown.width > 0.0 && grown.height > 0.0 => Some(shadow),
      _ => None,
    }
  }
}

/// The error returned when text isn't a valid box shadow.
///
/// The message repeats the text, says what is wrong, and shows the accepted form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid BoxShadow '{text}': {problem}; expected forms like '0 2 8 #40000000' or '4 4 Black'")]
pub struct ParseBoxShadowError {
  /// The text that failed to parse.
  pub text: String,
  /// What is wrong with it.
  pub problem: ShadowProblem,
}

/// What is wrong with box shadow text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ShadowProblem {
  /// The text is empty, or only whitespace.
  #[error("it is empty")]
  Empty,
  /// It asks for an inset shadow, which isn't supported yet.
  #[error("inset shadows aren't supported yet")]
  Inset,
  /// The last part is a number, so there's no color.
  #[error("it needs a color last")]
  MissingColor,
  /// Fewer than two numbers before the color.
  #[error("it needs at least an x and a y offset")]
  TooFewNumbers,
  /// More than four numbers before the color.
  #[error("it has more than four numbers")]
  TooManyNumbers,
  /// A part before the color isn't a finite number.
  #[error(transparent)]
  Number(#[from] NumberProblem),
  /// The blur is negative.
  #[error("the blur '{0}' is negative")]
  NegativeBlur(String),
  /// The color doesn't parse.
  #[error("its color '{}' is invalid: {}", .0.text, .0.problem)]
  Color(ParseColorError),
}

impl FromStr for BoxShadow {
  type Err = ParseBoxShadowError;

  /// Parses `offset-x offset-y [blur [spread]] color`, as described on [`ShadowProblem`]'s
  /// variants: whitespace between the parts, the color last.
  fn from_str(text: &str) -> Result<BoxShadow, ParseBoxShadowError> {
    parse_shadow(text).map_err(|problem| ParseBoxShadowError {
      text: text.to_owned(),
      problem,
    })
  }
}

impl ToRust for BoxShadow {
  /// Writes `new` and `with_spread`: the struct is `#[non_exhaustive]`, so no literal.
  fn to_rust(&self, out: &mut RustExpr) {
    out.call(
      env!("CARGO_CRATE_NAME"),
      "BoxShadow::new",
      &[&self.offset, &self.blur, &self.color],
    );
    out.raw(".with_spread(");
    self.spread.to_rust(out);
    out.raw(")");
  }
}

fn parse_shadow(text: &str) -> Result<BoxShadow, ShadowProblem> {
  let tokens: Vec<&str> = text.split_whitespace().collect();
  let Some((color, numbers)) = tokens.split_last() else {
    return Err(ShadowProblem::Empty);
  };
  if tokens.iter().any(|token| token.eq_ignore_ascii_case("inset")) {
    return Err(ShadowProblem::Inset);
  }
  if color.parse::<f32>().is_ok() {
    return Err(ShadowProblem::MissingColor);
  }
  match numbers.len() {
    0 | 1 => return Err(ShadowProblem::TooFewNumbers),
    5.. => return Err(ShadowProblem::TooManyNumbers),
    _ => {}
  }
  let values = numbers
    .iter()
    .map(|token| finite::parse(token))
    .collect::<Result<Vec<f32>, NumberProblem>>()?;
  let blur = values.get(2).copied().unwrap_or(0.0);
  if blur < 0.0 {
    return Err(ShadowProblem::NegativeBlur(numbers[2].to_owned()));
  }
  let color = color.parse::<Color>().map_err(ShadowProblem::Color)?;
  Ok(BoxShadow::new(Point::new(values[0], values[1]), blur, color).with_spread(values.get(3).copied().unwrap_or(0.0)))
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  const BLACK: Color = Color::rgb(0, 0, 0);

  fn parse(text: &str) -> Result<BoxShadow, ShadowProblem> {
    text.parse::<BoxShadow>().map_err(|error| error.problem)
  }

  // A shadow that draws nothing gives no shape, without the caller checking first; a NaN spread
  // counts as 0, as it does when drawn.
  #[test]
  fn shape_makes_the_shadow_drawable() {
    let rect = RoundedRect::new(Rect::new(0.0, 0.0, 10.0, 10.0), CornerRadius::ZERO);
    let broken = BoxShadow::new(Point::new(f32::NAN, 0.0), 0.0, BLACK);
    let nan_spread = BoxShadow::new(Point::ZERO, 0.0, BLACK).with_spread(f32::NAN);

    assert_eq!(broken.shape(rect), None);
    assert_eq!(nan_spread.shape(rect), Some(rect));
  }

  // Two numbers and a color: an offset, no blur, no spread.
  #[test]
  fn two_numbers_and_a_color() {
    assert_eq!(
      parse("4 4 Black"),
      Ok(BoxShadow::new(Point::new(4.0, 4.0), 0.0, Color::rgb(0, 0, 0)))
    );
  }

  // A third number is the blur.
  #[test]
  fn three_numbers_add_blur() {
    assert_eq!(parse("0 2 8 #40000000").map(|shadow| shadow.blur), Ok(8.0));
  }

  // A fourth number is the spread.
  #[test]
  fn four_numbers_add_spread() {
    assert_eq!(parse("0 0 12 3 #000").map(|shadow| shadow.spread), Ok(3.0));
  }

  // A negative spread shrinks the shadow and is allowed.
  #[test]
  fn negative_spread_is_allowed() {
    assert_eq!(parse("0 0 12 -2 #000").map(|shadow| shadow.spread), Ok(-2.0));
  }

  // Tabs and repeated spaces separate tokens like one space.
  #[test]
  fn extra_whitespace_is_ignored() {
    assert_eq!(parse("  4\t4   Black "), parse("4 4 Black"));
  }

  // Each problem is reported by name, and the message quotes the text.
  #[test]
  fn problems_are_named() {
    assert_eq!(parse(""), Err(ShadowProblem::Empty));
    assert_eq!(parse("   "), Err(ShadowProblem::Empty));
    assert_eq!(parse("4 4"), Err(ShadowProblem::MissingColor));
    assert_eq!(parse("4 Black"), Err(ShadowProblem::TooFewNumbers));
    assert_eq!(parse("1 2 3 4 5 Black"), Err(ShadowProblem::TooManyNumbers));
    assert_eq!(
      parse("4 x Black"),
      Err(ShadowProblem::Number(NumberProblem::NotANumber("x".to_owned())))
    );
    assert_eq!(
      parse("4 NaN Black"),
      Err(ShadowProblem::Number(NumberProblem::NotFinite("NaN".to_owned())))
    );
    assert_eq!(
      "4 NaN Black".parse::<BoxShadow>().unwrap_err().to_string(),
      "invalid BoxShadow '4 NaN Black': 'NaN' is not a finite number; expected forms like '0 2 8 #40000000' or '4 4 Black'"
    );
    assert_eq!(parse("4 4 -8 Black"), Err(ShadowProblem::NegativeBlur("-8".to_owned())));
    assert!(matches!(parse("4 4 #4000000"), Err(ShadowProblem::Color(_))));
    let error = "4 4".parse::<BoxShadow>().expect_err("no color");
    assert!(error.to_string().contains("'4 4'"), "{error}");
  }

  // A bad color is reported as the color's problem, quoting the color token.
  #[test]
  fn bad_color_message_quotes_the_color() {
    let error = "4 4 #4000000".parse::<BoxShadow>().expect_err("bad color");

    assert!(error.to_string().contains("its color '#4000000'"), "{error}");
  }

  // `inset` is refused by name wherever it is, before the numbers are read.
  #[test]
  fn inset_is_reported_as_inset() {
    assert_eq!(parse("inset 0 0 4 Black"), Err(ShadowProblem::Inset));
    assert_eq!(parse("0 0 4 Black INSET"), Err(ShadowProblem::Inset));
  }

  // `inf` reads as a number, so a last token of `inf` is a missing color, not a color name.
  #[test]
  fn infinity_as_the_last_token_is_missing_color() {
    assert_eq!(parse("4 4 inf"), Err(ShadowProblem::MissingColor));
  }

  // `new` has no spread, and `with_spread` sets only the spread.
  #[test]
  fn box_shadow_new_has_no_spread() {
    let shadow = BoxShadow::new(Point::new(1.0, 2.0), 3.0, BLACK);

    assert_eq!(
      (shadow.offset, shadow.blur, shadow.spread, shadow.color),
      (Point::new(1.0, 2.0), 3.0, 0.0, BLACK)
    );
    assert_eq!(shadow.clone().with_spread(4.0), BoxShadow { spread: 4.0, ..shadow });
  }

  // An offset that isn't finite draws nothing. A blur below 0, NaN or infinite draws as 0, and a
  // NaN or infinite spread as 0; a negative spread stays, to shrink the rectangle.
  #[test]
  fn box_shadow_values_are_made_drawable() {
    let drawn = |blur, spread| {
      BoxShadow::new(Point::ZERO, blur, BLACK)
        .with_spread(spread)
        .drawable()
        .map(|shadow| (shadow.blur, shadow.spread))
    };

    for offset in [
      Point::new(f32::NAN, 0.0),
      Point::new(0.0, f32::INFINITY),
      Point::new(f32::NEG_INFINITY, 0.0),
    ] {
      assert_eq!(BoxShadow::new(offset, 1.0, BLACK).drawable(), None, "offset {offset}");
    }
    assert_eq!(
      [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 4.0].map(|blur| drawn(blur, 0.0)),
      [
        Some((0.0, 0.0)),
        Some((0.0, 0.0)),
        Some((0.0, 0.0)),
        Some((0.0, 0.0)),
        Some((4.0, 0.0))
      ]
    );
    assert_eq!(
      [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -2.0].map(|spread| drawn(1.0, spread)),
      [Some((1.0, 0.0)), Some((1.0, 0.0)), Some((1.0, 0.0)), Some((1.0, -2.0))]
    );
  }

  // The shadow's rectangle is the caster's, grown by the spread on each side and moved by the
  // offset. Rounded corners grow by the spread, square ones stay square, and a corner shrunk past
  // 0 is square. A negative spread shrinks the rectangle, and nothing is left once it has no width
  // or height, or once a coordinate isn't finite.
  #[test]
  fn box_shadow_shape_grows_and_moves() {
    let card = RoundedRect::new(Rect::new(10.0, 20.0, 100.0, 50.0), CornerRadius::new(2.0, 0.0, 8.0, 0.0));
    let spread = |spread| BoxShadow::new(Point::ZERO, 0.0, BLACK).with_spread(spread);
    let crowded = RoundedRect::new(Rect::new(0.0, 0.0, 10.0, 10.0), CornerRadius::new(5.0, 5.0, 0.0, 0.0));

    assert_eq!(
      BoxShadow::new(Point::new(3.0, 5.0), 8.0, BLACK).with_spread(2.0).shape(card),
      Some(RoundedRect::new(
        Rect::new(11.0, 23.0, 104.0, 54.0),
        CornerRadius::new(4.0, 0.0, 10.0, 0.0)
      ))
    );
    assert_eq!(
      spread(-3.0).shape(card),
      Some(RoundedRect::new(
        Rect::new(13.0, 23.0, 94.0, 44.0),
        CornerRadius::new(0.0, 0.0, 5.0, 0.0)
      ))
    );
    assert_eq!(
      spread(2.0).shape(crowded),
      Some(RoundedRect::new(
        Rect::new(-2.0, -2.0, 14.0, 14.0),
        CornerRadius::new(7.0, 7.0, 0.0, 0.0)
      )),
      "radii that meet keep meeting"
    );
    assert_eq!(spread(-25.0).shape(card), None, "no height left");
    assert_eq!(spread(f32::MAX).shape(card), None, "overflows");
    assert_eq!(
      spread(0.0).shape(Rect::new(0.0, 0.0, f32::NAN, 1.0).into()),
      None,
      "the caster draws nothing"
    );
  }
}
