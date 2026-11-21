//! Text whose byte ranges each have their own [`TextStyle`].

use super::TextStyle;
use std::ops::Range;

/// Text whose byte ranges each have their own [`TextStyle`]: what a [`TextEngine`](super::TextEngine)
/// shapes.
///
/// It's built by pushing pieces of text, each in its own style. A style's index is its place among
/// the pushes, and each [`GlyphRun`](super::GlyphRun) the engine shapes reports the index of the
/// style it came from, so a renderer can paint each piece with its own brush. The spans are sorted,
/// adjacent, on char boundaries, and cover the text.
///
/// # Examples
///
/// ```
/// use artic_layout::text::{FontFamily, FontWeight, StyledText, TextStyle};
///
/// let regular = TextStyle::new(FontFamily::default(), 14.0);
/// let bold = TextStyle {
///   weight: FontWeight::BOLD,
///   ..regular.clone()
/// };
/// let mut text = StyledText::new();
/// text.push("Hello, ", regular);
/// let name = text.push("Ada", bold);
///
/// assert_eq!(text.text(), "Hello, Ada");
/// assert_eq!(text.style_at(7), Some(name));
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyledText {
  text: String,
  styles: Vec<TextStyle>,
  spans: Vec<(Range<usize>, usize)>,
}

impl StyledText {
  /// No text and no style.
  #[must_use]
  pub fn new() -> StyledText {
    StyledText::default()
  }

  /// `text` in `style`, style 0, as one [`push`](StyledText::push) makes it, keeping a `String`
  /// it's given rather than copying it.
  #[must_use]
  pub fn plain(text: impl Into<String>, style: TextStyle) -> StyledText {
    let text = text.into();
    let mut spans = Vec::new();
    join(&mut spans, (0..text.len(), 0));
    StyledText {
      text,
      styles: vec![style],
      spans,
    }
  }

  /// Appends `text` in `style`, and returns the style's index. Empty text still adds its style, so
  /// the indices match the pushes, but no span.
  pub fn push(&mut self, text: &str, style: TextStyle) -> usize {
    let index = self.styles.len();
    self.styles.push(style);
    let start = self.text.len();
    self.text.push_str(text);
    join(&mut self.spans, (start..self.text.len(), index));
    index
  }

  /// The whole text.
  #[must_use]
  pub fn text(&self) -> &str {
    &self.text
  }

  /// The styles, by index.
  #[must_use]
  pub fn styles(&self) -> &[TextStyle] {
    &self.styles
  }

  /// Each span's byte range and style index, in text order.
  pub fn spans(&self) -> impl ExactSizeIterator<Item = (Range<usize>, usize)> + '_ {
    self.spans.iter().cloned()
  }

  /// The style the text is shaped in where no span says otherwise: the first span's, or style 0
  /// when there's no text; `None` with no style. A style no text is in plays no other part, so an
  /// empty push doesn't change what the text looks like.
  #[must_use]
  pub fn base_style(&self) -> Option<usize> {
    self
      .spans
      .first()
      .map(|&(_, style)| style)
      .or((!self.styles.is_empty()).then_some(0))
  }

  /// The index of the style the byte at `byte` is in, or `None` past the text's end.
  #[must_use]
  pub fn style_at(&self, byte: usize) -> Option<usize> {
    let at = self.spans.partition_point(|(range, _)| range.end <= byte);
    self.spans.get(at).map(|&(_, style)| style)
  }

  /// The text in `range`, with every style and its index kept, for an engine that shapes part of
  /// what it was given.
  ///
  /// # Panics
  ///
  /// When `range` isn't on char boundaries of the text, as slicing a `str` does.
  #[must_use]
  pub fn slice(&self, range: Range<usize>) -> StyledText {
    StyledText {
      text: self.text[range.clone()].to_owned(),
      styles: self.styles.clone(),
      spans: self.clipped(range, 0).collect(),
    }
  }

  /// Replaces the text in `range` with `text` in the existing style `style`, as
  /// [`String::replace_range`] does, for an engine that adds text of its own, such as an ellipsis.
  ///
  /// # Panics
  ///
  /// When `range` isn't on char boundaries of the text, or `style` isn't the index of a style.
  pub fn replace_range(&mut self, range: Range<usize>, text: &str, style: usize) {
    assert!(style < self.styles.len(), "style {style} of {}", self.styles.len());
    let inserted_end = range.start + text.len();
    let mut spans = Vec::with_capacity(self.spans.len() + 2);
    for span in self.clipped(0..range.start, 0) {
      join(&mut spans, span);
    }
    join(&mut spans, (range.start..inserted_end, style));
    for span in self.clipped(range.end..self.text.len(), inserted_end) {
      join(&mut spans, span);
    }
    self.text.replace_range(range, text);
    self.spans = spans;
  }

  /// The spans' parts inside `range`, moved so `range` starts at `to`.
  fn clipped(&self, range: Range<usize>, to: usize) -> impl Iterator<Item = (Range<usize>, usize)> + '_ {
    self.spans.iter().filter_map(move |(span, style)| {
      let start = span.start.max(range.start);
      let end = span.end.min(range.end);
      (start < end).then(|| (start - range.start + to..end - range.start + to, *style))
    })
  }
}

/// Adds `span` after `spans`: dropped when empty, merged into the last span when they touch and
/// share a style.
fn join(spans: &mut Vec<(Range<usize>, usize)>, span: (Range<usize>, usize)) {
  if span.0.is_empty() {
    return;
  }
  match spans.last_mut() {
    Some((last, style)) if last.end == span.0.start && *style == span.1 => last.end = span.0.end,
    _ => spans.push(span),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::text::FontFamily;
  use pretty_assertions::assert_eq;

  fn style(size: f32) -> TextStyle {
    TextStyle::new(FontFamily::default(), size)
  }

  // The base style is the first span's, style 0 for text with no span, and none without a style.
  #[test]
  fn base_style_is_the_first_spans() {
    let mut text = StyledText::new();
    assert_eq!(text.base_style(), None);
    text.push("", style(1.0));
    assert_eq!(text.base_style(), Some(0), "no text");
    text.push("a", style(2.0));
    assert_eq!(text.base_style(), Some(1), "the first span");
  }

  // "ab" in style 0, "€" (three bytes) in style 1 and "cd" in style 2.
  fn three() -> StyledText {
    let mut text = StyledText::new();
    text.push("ab", style(10.0));
    text.push("€", style(11.0));
    text.push("cd", style(12.0));
    text
  }

  fn spans(text: &StyledText) -> Vec<(Range<usize>, usize)> {
    text.spans().collect()
  }

  // Each push appends its text and returns the index of its style, and the spans cover the text in
  // order.
  #[test]
  fn pushes_index_their_styles() {
    let mut text = StyledText::new();

    assert_eq!(text.push("ab", style(10.0)), 0);
    assert_eq!(text.push("€", style(11.0)), 1);

    assert_eq!(text.text(), "ab€");
    assert_eq!(text.styles(), [style(10.0), style(11.0)]);
    assert_eq!(spans(&text), [(0..2, 0), (2..5, 1)]);
  }

  // An empty push records its style, keeping later indices aligned with the pushes, but no span.
  #[test]
  fn empty_push_keeps_its_style_without_a_span() {
    let mut text = StyledText::new();

    text.push("", style(10.0));
    let next = text.push("a", style(11.0));

    assert_eq!(next, 1);
    assert_eq!(text.styles().len(), 2);
    assert_eq!(spans(&text), [(0..1, 1)]);
  }

  // Plain text is one push: one style, one span over all of it, and none when it's empty.
  #[test]
  fn plain_is_one_push() {
    let text = StyledText::plain("Hello", style(14.0));

    assert_eq!(text.styles(), [style(14.0)]);
    assert_eq!(spans(&text), [(0..5, 0)]);
    assert_eq!(StyledText::plain("", style(14.0)).spans().len(), 0);
  }

  // A byte's style is its span's, inside a multi-byte char too, and there is none past the end.
  #[test]
  fn style_at_finds_the_span() {
    let text = three();

    assert_eq!(text.style_at(0), Some(0));
    assert_eq!(text.style_at(2), Some(1));
    assert_eq!(text.style_at(4), Some(1));
    assert_eq!(text.style_at(6), Some(2));
    assert_eq!(text.style_at(7), None);
  }

  // A slice keeps the whole style table and each clipped span's original index, from offset 0.
  #[test]
  fn slice_keeps_style_indices() {
    let text = three();

    let slice = text.slice(1..6);

    assert_eq!(slice.text(), "b€c");
    assert_eq!(slice.styles(), text.styles());
    assert_eq!(spans(&slice), [(0..1, 0), (1..4, 1), (4..5, 2)]);
    assert_eq!(text.slice(5..7).spans().collect::<Vec<_>>(), [(0..2, 2)]);
  }

  // Slicing inside a char panics, as slicing a `str` does.
  #[test]
  #[should_panic(expected = "char boundary")]
  fn slice_inside_a_char_panics() {
    let _ = three().slice(0..3);
  }

  // A replacement takes the given style, and the spans after it move by the change in length.
  #[test]
  fn replace_range_inserts_in_a_style() {
    let mut text = three();

    text.replace_range(2..5, "…", 2);

    assert_eq!(text.text(), "ab…cd");
    assert_eq!(spans(&text), [(0..2, 0), (2..7, 2)]);
  }

  // Inserting at the start splits nothing, and text in the style beside it merges into its span.
  #[test]
  fn replace_range_inserts_at_the_start_and_merges() {
    let mut text = three();

    text.replace_range(0..0, "\u{200E}", 0);

    assert_eq!(text.text(), "\u{200E}ab€cd");
    assert_eq!(spans(&text), [(0..5, 0), (5..8, 1), (8..10, 2)]);
  }

  // A replacement inside one span splits it around the new text.
  #[test]
  fn replace_range_splits_a_span() {
    let mut text = StyledText::plain("abcd", style(10.0));
    text.push("", style(11.0));

    text.replace_range(1..3, "x", 1);

    assert_eq!(text.text(), "axd");
    assert_eq!(spans(&text), [(0..1, 0), (1..2, 1), (2..3, 0)]);
  }

  // A style index the text doesn't have panics.
  #[test]
  #[should_panic(expected = "style 3 of 3")]
  fn replace_range_with_an_unknown_style_panics() {
    three().replace_range(0..0, "x", 3);
  }
}
