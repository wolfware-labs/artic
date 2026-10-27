//! The values text is shaped from and into, and the engine that shapes it.
//!
//! A control describes its text with a [`TextStyle`], a [`ParagraphStyle`] and [`TextLimits`], asks a
//! [`TextEngine`] for the [`Paragraph`] they make while it's measured, and draws that paragraph when
//! it renders.

mod font;
mod paragraph;
mod paragraph_style;

pub use font::{
  FamilyName, FamilyProblem, FontFamily, FontStyle, FontWeight, GenericFamily, ParseFontFamilyError, ParseFontWeightError,
  WeightProblem,
};
pub use paragraph::{FontBytes, FontData, Glyph, GlyphRun, Line, Paragraph, Synthesis};
pub use paragraph_style::{ParagraphStyle, TextAlignment, TextTrimming, TextWrapping};

/// How the characters look: their font.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
  /// The families to find the font in, in order.
  pub family: FontFamily,
  /// The font size, in logical pixels.
  pub size: f32,
  /// The font weight.
  pub weight: FontWeight,
  /// Upright or slanted.
  pub style: FontStyle,
}

impl TextStyle {
  /// `family` at `size`, with normal weight and style.
  #[must_use]
  pub fn new(family: FontFamily, size: f32) -> TextStyle {
    TextStyle {
      family,
      size,
      weight: FontWeight::NORMAL,
      style: FontStyle::Normal,
    }
  }
}

/// How much room layout gives a paragraph.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextLimits {
  /// The width lines break at and trimmed lines fit, or `None` for no limit. Whether lines break
  /// at it is [`ParagraphStyle::wrapping`]'s choice.
  pub max_width: Option<f32>,
  /// The height the kept lines fit, or `None` for no limit. The first line is always kept.
  pub max_height: Option<f32>,
}

impl TextLimits {
  /// No limits.
  pub const NONE: TextLimits = TextLimits {
    max_width: None,
    max_height: None,
  };

  /// A limit on the width only.
  #[must_use]
  pub const fn width(width: f32) -> TextLimits {
    TextLimits {
      max_width: Some(width),
      max_height: None,
    }
  }

  /// The width limit as an engine applies it: `None` when there is none or it isn't finite, and
  /// 0 for a negative one.
  #[must_use]
  pub fn usable_width(self) -> Option<f32> {
    usable(self.max_width)
  }

  /// The height limit as an engine applies it: `None` when there is none or it isn't finite, and
  /// 0 for a negative one.
  #[must_use]
  pub fn usable_height(self) -> Option<f32> {
    usable(self.max_height)
  }
}

fn usable(limit: Option<f32>) -> Option<f32> {
  limit.filter(|limit| limit.is_finite()).map(|limit| limit.max(0.0))
}

/// Shapes text into paragraphs: finds fonts, places glyphs and breaks lines.
///
/// Layout holds it as `&mut dyn TextEngine` and lends it to the controls it measures and
/// arranges. It runs on the UI thread, so it needn't be `Send`.
pub trait TextEngine {
  /// Shapes `text` in `style`, and lays it out as `paragraph` says within `limits`.
  ///
  /// It never fails: text it can't shape, such as a size that isn't above 0, gives
  /// `Paragraph::default()`, a paragraph with no lines, of size 0.
  fn paragraph(&mut self, text: &str, style: &TextStyle, paragraph: &ParagraphStyle, limits: TextLimits) -> Paragraph;
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::geometry::Size;
  use pretty_assertions::assert_eq;

  // An engine that makes every character a square of the font size, on one line, and records
  // what it was asked.
  #[derive(Default)]
  struct Squares {
    asked: Vec<(String, TextLimits)>,
  }

  impl TextEngine for Squares {
    fn paragraph(&mut self, text: &str, style: &TextStyle, _paragraph: &ParagraphStyle, limits: TextLimits) -> Paragraph {
      self.asked.push((text.to_owned(), limits));
      Paragraph::new(Size::new(text.chars().count() as f32 * style.size, style.size), [], [])
    }
  }

  // `width` sets only the width, and `NONE`, the default, sets nothing.
  #[test]
  fn text_limits_width_and_none() {
    assert_eq!(
      TextLimits::width(200.0),
      TextLimits {
        max_width: Some(200.0),
        max_height: None,
      }
    );
    assert_eq!(
      TextLimits::NONE,
      TextLimits {
        max_width: None,
        max_height: None,
      }
    );
    assert_eq!(TextLimits::default(), TextLimits::NONE);
  }

  // `TextStyle::new` takes the family and size, with normal weight and style.
  #[test]
  fn text_style_new_has_normal_weight_and_style() {
    let style = TextStyle::new(FontFamily::default(), 14.0);

    assert_eq!(style.size, 14.0);
    assert_eq!(style.weight, FontWeight::NORMAL);
    assert_eq!(style.style, FontStyle::Normal);
  }

  // A limit counts only when it's finite, and a negative one is 0; NaN and infinities mean none.
  #[test]
  fn usable_limits_drop_what_isnt_finite_and_clamp_negatives() {
    let limits = |limit: Option<f32>| TextLimits {
      max_width: limit,
      max_height: limit,
    };

    for (limit, expected) in [
      (Some(10.0), Some(10.0)),
      (Some(-3.0), Some(0.0)),
      (Some(f32::NAN), None),
      (Some(f32::INFINITY), None),
      (None, None),
    ] {
      assert_eq!(limits(limit).usable_width(), expected, "width {limit:?}");
      assert_eq!(limits(limit).usable_height(), expected, "height {limit:?}");
    }
  }

  // An engine is reached through `&mut dyn TextEngine`, as layout holds it, and gets the text and
  // limits it was asked for.
  #[test]
  fn text_engine_serves_as_a_trait_object() {
    let mut squares = Squares::default();
    let engine: &mut dyn TextEngine = &mut squares;

    let paragraph = engine.paragraph(
      "abc",
      &TextStyle::new(FontFamily::default(), 10.0),
      &ParagraphStyle::default(),
      TextLimits::width(25.0),
    );

    assert_eq!(paragraph.size(), Size::new(30.0, 10.0));
    assert_eq!(squares.asked, [(String::from("abc"), TextLimits::width(25.0))]);
  }
}
