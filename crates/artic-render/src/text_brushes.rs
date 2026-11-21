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
pub struct TextBrushes(Held);

/// The brushes as held: one brush, the common case, needs no allocation.
#[derive(Debug, Clone, Default, PartialEq)]
enum Held {
  #[default]
  None,
  One(Brush),
  Many(Vec<Brush>),
}

impl TextBrushes {
  /// The brush for the glyph runs in style `style`: the brush at that index, the last brush past
  /// the end, or `None` when there is no brush.
  #[must_use]
  pub fn brush(&self, style: usize) -> Option<&Brush> {
    self.slot(style).map(|slot| &self.as_slice()[slot])
  }

  /// The index of the brush style `style` takes, as [`brush`](Self::brush) picks it, so a renderer
  /// can reuse what it made of a brush for every style that takes it.
  #[must_use]
  pub fn slot(&self, style: usize) -> Option<usize> {
    self.as_slice().len().checked_sub(1).map(|last| style.min(last))
  }

  /// The brushes, by style index.
  #[must_use]
  pub fn as_slice(&self) -> &[Brush] {
    match &self.0 {
      Held::None => &[],
      Held::One(brush) => std::slice::from_ref(brush),
      Held::Many(brushes) => brushes,
    }
  }

  /// Whether there is no brush, so nothing is drawn.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.as_slice().is_empty()
  }
}

impl<B: Into<Brush>> From<B> for TextBrushes {
  fn from(brush: B) -> TextBrushes {
    TextBrushes(Held::One(brush.into()))
  }
}

impl<B: Into<Brush>, const N: usize> From<[B; N]> for TextBrushes {
  fn from(brushes: [B; N]) -> TextBrushes {
    brushes.into_iter().map(Into::into).collect()
  }
}

impl From<Vec<Brush>> for TextBrushes {
  fn from(mut brushes: Vec<Brush>) -> TextBrushes {
    TextBrushes(match brushes.len() {
      0 => Held::None,
      1 => Held::One(brushes.remove(0)),
      _ => Held::Many(brushes),
    })
  }
}

impl FromIterator<Brush> for TextBrushes {
  fn from_iter<I: IntoIterator<Item = Brush>>(brushes: I) -> TextBrushes {
    brushes.into_iter().collect::<Vec<_>>().into()
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
  // A style takes the slot of its own brush, or the last one past the end, so a renderer can share
  // what it made of a brush; with no brush there's no slot.
  #[test]
  fn each_style_takes_its_brushs_slot() {
    let two = TextBrushes::from([Color::rgb(1, 2, 3), Color::rgb(4, 5, 6)]);

    assert_eq!([0, 1, 7].map(|style| two.slot(style)), [Some(0), Some(1), Some(1)]);
    assert_eq!(TextBrushes::from(Color::rgb(1, 2, 3)).slot(3), Some(0));
    assert_eq!(TextBrushes::default().slot(0), None);
  }

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
