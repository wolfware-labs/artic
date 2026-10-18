//! The values text is shaped from and into, and the engine that shapes it.
//!
//! A control describes its text with a [`TextStyle`] and [`TextLimits`], asks a [`TextEngine`] for
//! the [`Paragraph`] they make while it's measured, and draws that paragraph when it renders.

mod font;
mod paragraph;
mod paragraph_style;

pub use font::{
  FamilyName, FamilyProblem, FontFamily, FontStyle, FontWeight, GenericFamily, ParseFontFamilyError, ParseFontWeightError,
  WeightProblem,
};
pub use paragraph::{FontData, Glyph, GlyphRun, Line, Paragraph, Synthesis};
pub use paragraph_style::{ParagraphStyle, TextAlignment, TextTrimming, TextWrapping};
use std::num::NonZeroU32;

/// What text is shaped in: its font and its line spacing.
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
  /// The distance from one line's top to the next one's, in logical pixels, or `None` for the
  /// font's own line spacing.
  pub line_height: Option<f32>,
}

impl TextStyle {
  /// `family` at `size`, with normal weight and style and the font's own line spacing.
  #[must_use]
  pub fn new(family: FontFamily, size: f32) -> TextStyle {
    TextStyle {
      family,
      size,
      weight: FontWeight::NORMAL,
      style: FontStyle::Normal,
      line_height: None,
    }
  }
}

/// How much room text may take: a width to wrap at and a number of lines to keep.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextLimits {
  /// The width lines break at, or `None` to break only at hard line breaks.
  pub max_width: Option<f32>,
  /// The number of lines kept, or `None` to keep every line.
  pub max_lines: Option<NonZeroU32>,
}

impl TextLimits {
  /// No limits: lines break only at hard line breaks, and every line is kept.
  pub const NONE: TextLimits = TextLimits {
    max_width: None,
    max_lines: None,
  };

  /// Lines break to fit `width`, and every line is kept.
  #[must_use]
  pub const fn wrap(width: f32) -> TextLimits {
    TextLimits {
      max_width: Some(width),
      max_lines: None,
    }
  }
}

/// Shapes text into paragraphs: finds fonts, places glyphs and breaks lines.
///
/// Layout holds it as `&mut dyn TextEngine` and lends it to the controls it measures and
/// arranges. It runs on the UI thread, so it needn't be `Send`.
pub trait TextEngine {
  /// Shapes `text` in `style` and breaks it into lines within `limits`.
  ///
  /// It never fails: text it can't shape, such as a size that isn't above 0, gives an empty
  /// paragraph.
  fn paragraph(&mut self, text: &str, style: &TextStyle, limits: TextLimits) -> Paragraph;
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
    fn paragraph(&mut self, text: &str, style: &TextStyle, limits: TextLimits) -> Paragraph {
      self.asked.push((text.to_owned(), limits));
      Paragraph::new(Size::new(text.chars().count() as f32 * style.size, style.size), [], [])
    }
  }

  // `wrap` sets only the width, and `NONE`, the default, sets nothing.
  #[test]
  fn text_limits_wrap_and_none() {
    assert_eq!(
      TextLimits::wrap(200.0),
      TextLimits {
        max_width: Some(200.0),
        max_lines: None,
      }
    );
    assert_eq!(
      TextLimits::NONE,
      TextLimits {
        max_width: None,
        max_lines: None,
      }
    );
    assert_eq!(TextLimits::default(), TextLimits::NONE);
  }

  // `TextStyle::new` takes the family and size, with normal weight and style and the font's own
  // line spacing.
  #[test]
  fn text_style_new_has_normal_weight_and_style() {
    let style = TextStyle::new(FontFamily::default(), 14.0);

    assert_eq!(style.size, 14.0);
    assert_eq!(style.weight, FontWeight::NORMAL);
    assert_eq!(style.style, FontStyle::Normal);
    assert_eq!(style.line_height, None);
  }

  // An engine is reached through `&mut dyn TextEngine`, as layout holds it, and gets the text and
  // limits it was asked for.
  #[test]
  fn text_engine_serves_as_a_trait_object() {
    let mut squares = Squares::default();
    let engine: &mut dyn TextEngine = &mut squares;

    let paragraph = engine.paragraph("abc", &TextStyle::new(FontFamily::default(), 10.0), TextLimits::wrap(25.0));

    assert_eq!(paragraph.size(), Size::new(30.0, 10.0));
    assert_eq!(squares.asked, [(String::from("abc"), TextLimits::wrap(25.0))]);
  }
}
