//! The brushes a paragraph is painted with, one per text style.

use crate::Brush;

/// The brushes a paragraph is painted with: one per style of the
/// [`StyledText`](artic_layout::text::StyledText) it was shaped from, by the style's index.
///
/// Each glyph run takes the brush at its [`style`](artic_layout::text::GlyphRun::style) index. A run
/// whose index is past the last brush takes the last one, so one brush paints all the text, and a
/// short list paints the styles after it with its last brush. With no brush, nothing is drawn.
///
/// Anything that converts into a [`Brush`], such as a [`Color`](crate::Color), converts into one
/// brush, and an array or a `Vec` of brushes into a list.
///
/// # Examples
///
/// ```
/// use artic_render::{Color, TextBrushes};
///
/// let one = TextBrushes::from(Color::rgb(15, 23, 42));
/// let two = TextBrushes::from([Color::rgb(15, 23, 42), Color::rgb(220, 38, 38)]);
///
/// assert_eq!(one.brush(1), one.brush(0));
/// assert_eq!(two.brush(1), Some(&Color::rgb(220, 38, 38).into()));
/// assert_eq!(two.brush(5), two.brush(1));
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextBrushes(Vec<Brush>);

impl TextBrushes {
  /// The brush for the glyph runs in style `style`: the brush at that index, the last brush past
  /// the end, or `None` when there is no brush.
  #[must_use]
  pub fn brush(&self, style: usize) -> Option<&Brush> {
    self.0.get(style).or_else(|| self.0.last())
  }

  /// The brushes, by style index.
  #[must_use]
  pub fn as_slice(&self) -> &[Brush] {
    &self.0
  }

  /// Whether there is no brush, so nothing is drawn.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }
}

impl<B: Into<Brush>> From<B> for TextBrushes {
  fn from(brush: B) -> TextBrushes {
    TextBrushes(vec![brush.into()])
  }
}

impl<B: Into<Brush>, const N: usize> From<[B; N]> for TextBrushes {
  fn from(brushes: [B; N]) -> TextBrushes {
    TextBrushes(brushes.into_iter().map(Into::into).collect())
  }
}

impl From<Vec<Brush>> for TextBrushes {
  fn from(brushes: Vec<Brush>) -> TextBrushes {
    TextBrushes(brushes)
  }
}

impl FromIterator<Brush> for TextBrushes {
  fn from_iter<I: IntoIterator<Item = Brush>>(brushes: I) -> TextBrushes {
    TextBrushes(brushes.into_iter().collect())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::Color;
  use pretty_assertions::assert_eq;

  const RED: Color = Color::rgb(255, 0, 0);
  const BLUE: Color = Color::rgb(0, 0, 255);

  // One brush paints every style.
  #[test]
  fn one_brush_paints_every_style() {
    let brushes = TextBrushes::from(RED);

    for style in [0, 1, 7] {
      assert_eq!(brushes.brush(style), Some(&RED.into()), "{style}");
    }
  }

  // A list gives each style its own brush, and the last one to the styles past its end.
  #[test]
  fn a_list_gives_each_style_its_brush_and_the_last_past_the_end() {
    let brushes = TextBrushes::from([RED, BLUE]);

    assert_eq!(brushes.brush(0), Some(&RED.into()));
    assert_eq!(brushes.brush(1), Some(&BLUE.into()));
    assert_eq!(brushes.brush(2), Some(&BLUE.into()));
    assert_eq!(brushes.as_slice().len(), 2);
  }

  // With no brush, no style has one.
  #[test]
  fn no_brush_paints_nothing() {
    let brushes = TextBrushes::from(Vec::new());

    assert!(brushes.is_empty());
    assert_eq!(brushes.brush(0), None);
    assert_eq!(TextBrushes::default(), brushes);
  }

  // A `Vec` of brushes and an iterator of them convert into the same list.
  #[test]
  fn vec_and_iterator_convert_alike() {
    let brushes = vec![Brush::from(RED), Brush::from(BLUE)];

    assert_eq!(TextBrushes::from(brushes.clone()), brushes.into_iter().collect());
  }
}
