//! How a paragraph is laid out: its line spacing, alignment, wrapping, trimming and line count.

use artic_macros::XamlEnum;
use std::num::NonZeroU32;

/// How a paragraph is laid out, from the control's properties: one per paragraph.
///
/// The default matches `TextBlock`'s: the font's own spacing, aligned to the start, no wrapping,
/// no trimming and every line kept.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ParagraphStyle {
  /// The distance from one line's top to the next one's, or `None` for the font's own spacing.
  pub line_height: Option<f32>,
  /// Where each line sits in the paragraph's width.
  pub alignment: TextAlignment,
  /// Whether and how lines break to fit `max_width`.
  pub wrapping: TextWrapping,
  /// How a line that doesn't fit is cut.
  pub trimming: TextTrimming,
  /// The number of lines kept, or `None` to keep every line.
  pub max_lines: Option<NonZeroU32>,
}

/// Whether and how lines break to fit the width layout gives them.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, XamlEnum)]
pub enum TextWrapping {
  /// Lines break only at hard line breaks.
  #[default]
  NoWrap,
  /// Lines break between words, and inside a word wider than the width, so nothing sticks out.
  Wrap,
  /// Lines break between words only, so a word wider than the width sticks out.
  WrapWithOverflow,
}

/// How a line that doesn't fit is cut, and where its ellipsis goes.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, XamlEnum)]
pub enum TextTrimming {
  /// Lines aren't cut: what doesn't fit draws past the edge.
  #[default]
  None,
  /// The text's start, cut between characters, then "…".
  CharacterEllipsis,
  /// The text's start, cut between words, then "…".
  WordEllipsis,
  /// The text's first 8 characters, "…", then as much of its end as fits.
  PrefixCharacterEllipsis,
  /// "…", then as much of the text's end as fits.
  LeadingCharacterEllipsis,
  /// The text with path segments near its middle replaced by "…", keeping the first and last.
  PathSegmentEllipsis,
}

/// Where each line sits in the paragraph's width.
///
/// `Start` and `End` follow the paragraph's direction; `Left` and `Right` don't.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, XamlEnum)]
pub enum TextAlignment {
  /// The side the paragraph's text starts on: left in left-to-right text, right in right-to-left.
  #[default]
  Start,
  /// The middle.
  Center,
  /// The side the paragraph's text ends on.
  End,
  /// The left edge, in either direction.
  Left,
  /// The right edge, in either direction.
  Right,
  /// Placed as `Start`, then stretched to the paragraph's width at its spaces, except the last
  /// line and lines that end at a hard break.
  Justify,
}

impl TextAlignment {
  /// The share of a line's free width that goes before it: 0, ½ or 1, in a paragraph that runs
  /// right to left when `right_to_left` is true. The same share places a paragraph in its box.
  #[must_use]
  pub fn factor(self, right_to_left: bool) -> f32 {
    match (self, right_to_left) {
      (TextAlignment::Center, _) => 0.5,
      (TextAlignment::Left, _) | (TextAlignment::Start | TextAlignment::Justify, false) | (TextAlignment::End, true) => 0.0,
      (TextAlignment::Right, _) | (TextAlignment::Start | TextAlignment::Justify, true) | (TextAlignment::End, false) => 1.0,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // `TextWrapping` parses its three names ignoring case, and doesn't wrap by default.
  #[test]
  fn text_wrapping_parses_ignoring_case() {
    assert_eq!("nowrap".parse::<TextWrapping>(), Ok(TextWrapping::NoWrap));
    assert_eq!("WRAP".parse::<TextWrapping>(), Ok(TextWrapping::Wrap));
    assert_eq!("wrapWithOverflow".parse::<TextWrapping>(), Ok(TextWrapping::WrapWithOverflow));
    assert_eq!(TextWrapping::default(), TextWrapping::NoWrap);
    assert_eq!(
      "Both".parse::<TextWrapping>().unwrap_err().expected,
      ["NoWrap", "Wrap", "WrapWithOverflow"]
    );
  }

  // `TextTrimming` parses its six names ignoring case, and doesn't trim by default.
  #[test]
  fn text_trimming_parses_ignoring_case() {
    assert_eq!("none".parse::<TextTrimming>(), Ok(TextTrimming::None));
    assert_eq!(
      "characterellipsis".parse::<TextTrimming>(),
      Ok(TextTrimming::CharacterEllipsis)
    );
    assert_eq!("WordEllipsis".parse::<TextTrimming>(), Ok(TextTrimming::WordEllipsis));
    assert_eq!(
      "PREFIXCHARACTERELLIPSIS".parse::<TextTrimming>(),
      Ok(TextTrimming::PrefixCharacterEllipsis)
    );
    assert_eq!(
      "leadingCharacterEllipsis".parse::<TextTrimming>(),
      Ok(TextTrimming::LeadingCharacterEllipsis)
    );
    assert_eq!(
      "PathSegmentEllipsis".parse::<TextTrimming>(),
      Ok(TextTrimming::PathSegmentEllipsis)
    );
    assert_eq!(TextTrimming::default(), TextTrimming::None);
    assert_eq!("Ellipsis".parse::<TextTrimming>().unwrap_err().expected.len(), 6);
  }

  // `TextAlignment` parses its six names ignoring case, and starts at the start by default.
  #[test]
  fn text_alignment_parses_ignoring_case() {
    assert_eq!("start".parse::<TextAlignment>(), Ok(TextAlignment::Start));
    assert_eq!("CENTER".parse::<TextAlignment>(), Ok(TextAlignment::Center));
    assert_eq!("End".parse::<TextAlignment>(), Ok(TextAlignment::End));
    assert_eq!("left".parse::<TextAlignment>(), Ok(TextAlignment::Left));
    assert_eq!("right".parse::<TextAlignment>(), Ok(TextAlignment::Right));
    assert_eq!("justify".parse::<TextAlignment>(), Ok(TextAlignment::Justify));
    assert_eq!(TextAlignment::default(), TextAlignment::Start);
    assert_eq!(
      "Middle".parse::<TextAlignment>().unwrap_err().expected,
      ["Start", "Center", "End", "Left", "Right", "Justify"]
    );
  }

  // Every row of the alignment table: Start and Justify follow the direction, End mirrors it,
  // Center is half either way, and Left and Right ignore it.
  #[test]
  fn alignment_factor_follows_direction() {
    let rows = [
      (TextAlignment::Start, 0.0, 1.0),
      (TextAlignment::Justify, 0.0, 1.0),
      (TextAlignment::End, 1.0, 0.0),
      (TextAlignment::Center, 0.5, 0.5),
      (TextAlignment::Left, 0.0, 0.0),
      (TextAlignment::Right, 1.0, 1.0),
    ];

    for (alignment, left_to_right, right_to_left) in rows {
      assert_eq!(alignment.factor(false), left_to_right, "{alignment:?}");
      assert_eq!(alignment.factor(true), right_to_left, "{alignment:?}");
    }
  }

  // The default paragraph style is TextBlock's: font spacing, start, no wrapping, no trimming,
  // every line.
  #[test]
  fn paragraph_style_defaults_match_textblock() {
    assert_eq!(
      ParagraphStyle::default(),
      ParagraphStyle {
        line_height: None,
        alignment: TextAlignment::Start,
        wrapping: TextWrapping::NoWrap,
        trimming: TextTrimming::None,
        max_lines: None,
      }
    );
  }
}
