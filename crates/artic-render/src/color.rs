mod names;

use artic_core::{RustExpr, ToRust};
use std::str::FromStr;

/// An sRGB color with straight (not premultiplied) alpha, 8 bits per channel.
///
/// It parses from text, ignoring whitespace around it:
///
/// - `#RGB` and `#ARGB`, each digit doubled: `#3AF` is `#33AAFF`;
/// - `#RRGGBB`, opaque, and `#AARRGGBB`, alpha first as in WPF and Avalonia. CSS writes alpha
///   last (`#RRGGBBAA`), so 8-digit text copied from CSS parses as a different color;
/// - a CSS Color Module Level 4 name, ignoring ASCII case, such as `SteelBlue` or
///   `rebeccapurple`. Each is also a constant, such as [`Color::STEEL_BLUE`], with both the `gray`
///   and `grey` spellings.
///
/// `transparent` is [`Color::TRANSPARENT`], `#00000000`. WPF's `Transparent` is `#00FFFFFF`: on
/// their own the two look the same, but a gradient to `transparent` fades through black.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub struct Color {
  /// Red.
  pub r: u8,
  /// Green.
  pub g: u8,
  /// Blue.
  pub b: u8,
  /// Alpha: 0 is transparent, 255 opaque.
  pub a: u8,
}

impl Color {
  /// Creates the color with these channels.
  #[must_use]
  pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
  }

  /// Creates the opaque color with these channels.
  #[must_use]
  pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::rgba(r, g, b, 255)
  }
}

/// The accepted forms, for error messages.
const FORMS: &str = r##""#RGB", "#ARGB", "#RRGGBB", "#AARRGGBB" or a color name"##;

impl FromStr for Color {
  type Err = ParseColorError;

  /// Parses hex or a color name, as described on [`Color`].
  fn from_str(text: &str) -> Result<Color, ParseColorError> {
    let trimmed = text.trim();
    let color = match trimmed.strip_prefix('#') {
      Some(digits) => hex(digits),
      None if trimmed.is_empty() => Err(ColorProblem::Empty),
      None => names::lookup(trimmed).ok_or_else(|| ColorProblem::UnknownName(trimmed.to_owned())),
    };
    color.map_err(|problem| ParseColorError {
      type_name: "Color",
      text: text.to_owned(),
      problem,
      forms: FORMS,
    })
  }
}

impl ToRust for Color {
  fn to_rust(&self, out: &mut RustExpr) {
    out.type_path(env!("CARGO_CRATE_NAME"), "Color::rgba(");
    for (index, channel) in [self.r, self.g, self.b, self.a].into_iter().enumerate() {
      if index > 0 {
        out.raw(", ");
      }
      channel.to_rust(out);
    }
    out.raw(")");
  }
}

/// Reads the hex digits after a `#`.
fn hex(digits: &str) -> Result<Color, ColorProblem> {
  let mut nibbles = Vec::with_capacity(8);
  for digit in digits.chars() {
    let value = digit.to_digit(16).ok_or(ColorProblem::NotHex(digit))?;
    nibbles.push(u8::try_from(value).expect("a hex digit fits in a byte"));
  }
  let pair = |high: u8, low: u8| (high << 4) | low;
  match *nibbles {
    [r, g, b] => Ok(Color::rgb(r * 17, g * 17, b * 17)),
    [a, r, g, b] => Ok(Color::rgba(r * 17, g * 17, b * 17, a * 17)),
    [r1, r2, g1, g2, b1, b2] => Ok(Color::rgb(pair(r1, r2), pair(g1, g2), pair(b1, b2))),
    [a1, a2, r1, r2, g1, g2, b1, b2] => Ok(Color::rgba(pair(r1, r2), pair(g1, g2), pair(b1, b2), pair(a1, a2))),
    _ => Err(ColorProblem::Length(nibbles.len())),
  }
}

/// The error returned when text isn't a valid color.
///
/// The message repeats the text, says what is wrong, and lists the accepted forms.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {type_name} '{text}': {problem}; expected {forms}")]
pub struct ParseColorError {
  /// The type being parsed: `Color`, or `Brush` for a brush written as a color.
  pub type_name: &'static str,
  /// The text that failed to parse.
  pub text: String,
  /// What is wrong with it.
  pub problem: ColorProblem,
  /// The accepted forms, as examples.
  pub forms: &'static str,
}

/// What is wrong with color text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ColorProblem {
  /// The text is empty, or only whitespace.
  #[error("it is empty")]
  Empty,
  /// A `#` color has a number of hex digits other than 3, 4, 6 or 8.
  #[error("it has {0} hex digits")]
  Length(usize),
  /// A `#` color has a character that isn't a hex digit.
  #[error("'{0}' is not a hex digit")]
  NotHex(char),
  /// The text isn't a color name.
  #[error("'{0}' is not a color name")]
  UnknownName(String),
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  /// Returns the problem `text` has as a color, panicking if it parses.
  fn problem(text: &str) -> ColorProblem {
    text.parse::<Color>().expect_err("the text is invalid").problem
  }

  // Each digit of a short form is doubled, and the 4-digit form puts alpha first.
  #[test]
  fn color_parses_short_hex() {
    assert_eq!("#3AF".parse(), Ok(Color::rgb(0x33, 0xAA, 0xFF)));
    assert_eq!("#83af".parse(), Ok(Color::rgba(0x33, 0xAA, 0xFF, 0x88)));
  }

  // Six digits are opaque, in either case, with whitespace around ignored.
  #[test]
  fn color_parses_rrggbb() {
    assert_eq!("#3B82F6".parse(), Ok(Color::rgb(0x3B, 0x82, 0xF6)));
    assert_eq!(" #3b82f6 ".parse(), Ok(Color::rgb(0x3B, 0x82, 0xF6)));
  }

  // Alpha comes first, as in WPF and Avalonia, not last as in CSS.
  #[test]
  fn color_parses_aarrggbb() {
    assert_eq!("#803B82F6".parse(), Ok(Color::rgba(0x3B, 0x82, 0xF6, 0x80)));
  }

  // Names ignore ASCII case, include CSS's additions to WPF's list, and `transparent` is clear
  // black.
  #[test]
  fn color_parses_named_case_insensitive() {
    for text in ["steelblue", "SteelBlue", "STEELBLUE", " SteelBlue "] {
      assert_eq!(text.parse(), Ok(Color::STEEL_BLUE), "{text}");
    }
    assert_eq!(Color::STEEL_BLUE, Color::rgb(70, 130, 180));
    assert_eq!("RebeccaPurple".parse(), Ok(Color::rgb(102, 51, 153)));
    assert_eq!("grey".parse(), Ok(Color::GRAY));
    assert_eq!("Transparent".parse(), Ok(Color::rgba(0, 0, 0, 0)));
    assert_eq!(names::NAMES.len(), 149, "CSS's 148 names and transparent");
  }

  // Bad lengths, non-hex digits, unknown names and empty text are errors naming the input.
  #[test]
  fn color_rejects_invalid() {
    assert_eq!(problem("#12"), ColorProblem::Length(2));
    assert_eq!(problem("#"), ColorProblem::Length(0));
    assert_eq!(problem("#12345"), ColorProblem::Length(5));
    assert_eq!(problem("#12G"), ColorProblem::NotHex('G'));
    assert_eq!(problem("#ÿÿÿ"), ColorProblem::NotHex('ÿ'));
    assert_eq!(problem("redd"), ColorProblem::UnknownName("redd".to_owned()));
    assert_eq!(problem("  "), ColorProblem::Empty);
    assert_eq!(
      "#12".parse::<Color>().expect_err("two digits").to_string(),
      r##"invalid Color '#12': it has 2 hex digits; expected "#RGB", "#ARGB", "#RRGGBB", "#AARRGGBB" or a color name"##
    );
    assert_eq!(
      "redd".parse::<Color>().expect_err("unknown").to_string(),
      r##"invalid Color 'redd': 'redd' is not a color name; expected "#RGB", "#ARGB", "#RRGGBB", "#AARRGGBB" or a color name"##
    );
  }
}
