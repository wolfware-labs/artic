//! Shaped text: lines and positioned glyphs, and the fonts they're drawn from.

use crate::geometry::Size;
use std::fmt::{self, Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Text shaped and broken into lines: its size, its lines and its glyph runs.
///
/// A [`TextEngine`](super::TextEngine) builds it, layout keeps it, and a renderer draws its runs.
/// It's cheap to clone: its lines and runs are shared. Positions are in logical pixels, relative to
/// the paragraph's top-left corner, and aren't snapped to device pixels. Two paragraphs are equal
/// when their content is, with fonts compared by [`FontData`]'s identity.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Paragraph {
  size: Size,
  lines: Arc<[Line]>,
  runs: Arc<[GlyphRun]>,
  right_to_left: bool,
  limited_width: bool,
  limited_height: bool,
}

impl Paragraph {
  /// A left-to-right paragraph of `size` that no limit shaped, with `lines` from top to bottom and
  /// the glyph `runs` that draw it. [`with_right_to_left`](Paragraph::with_right_to_left),
  /// [`with_limited_width`](Paragraph::with_limited_width) and
  /// [`with_limited_height`](Paragraph::with_limited_height) set the rest.
  #[must_use]
  pub fn new(size: Size, lines: impl Into<Arc<[Line]>>, runs: impl Into<Arc<[GlyphRun]>>) -> Paragraph {
    Paragraph {
      size,
      lines: lines.into(),
      runs: runs.into(),
      right_to_left: false,
      limited_width: false,
      limited_height: false,
    }
  }

  /// This paragraph, running right to left when `right_to_left` is true.
  #[must_use]
  pub fn with_right_to_left(mut self, right_to_left: bool) -> Paragraph {
    self.right_to_left = right_to_left;
    self
  }

  /// This paragraph, shaped by its width limit when `limited` is true.
  #[must_use]
  pub fn with_limited_width(mut self, limited: bool) -> Paragraph {
    self.limited_width = limited;
    self
  }

  /// This paragraph, shaped by its height limit when `limited` is true.
  #[must_use]
  pub fn with_limited_height(mut self, limited: bool) -> Paragraph {
    self.limited_height = limited;
    self
  }

  /// The widest line's width by the sum of the lines' heights. A paragraph with a justified line is
  /// as wide as the width it was justified to, which can be more than its widest natural line.
  #[must_use]
  pub fn size(&self) -> Size {
    self.size
  }

  /// The lines, from top to bottom.
  #[must_use]
  pub fn lines(&self) -> &[Line] {
    &self.lines
  }

  /// The glyph runs, in drawing order.
  #[must_use]
  pub fn runs(&self) -> &[GlyphRun] {
    &self.runs
  }

  /// Whether it draws anything: one of its runs has a glyph. Shaped empty text has a line and no
  /// glyph; the paragraph an engine gives for text it can't shape has neither.
  #[must_use]
  pub fn has_glyphs(&self) -> bool {
    self.runs.iter().any(|run| !run.glyphs().is_empty())
  }

  /// Whether the paragraph runs right to left, as its first strong character does.
  #[must_use]
  pub fn right_to_left(&self) -> bool {
    self.right_to_left
  }

  /// Whether its width limit shaped it: a line broke other than at a hard break, was stretched to
  /// justify it, or was cut to fit and lost some of its own text. Another width can give another
  /// paragraph.
  #[must_use]
  pub fn limited_width(&self) -> bool {
    self.limited_width
  }

  /// Whether its height limit dropped lines.
  #[must_use]
  pub fn limited_height(&self) -> bool {
    self.limited_height
  }
}

/// One line of a [`Paragraph`], in the paragraph's coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Line {
  /// The line's top edge.
  pub top: f32,
  /// The distance from the line's top to the next line's.
  pub height: f32,
  /// The baseline's distance from the paragraph's top.
  pub baseline: f32,
  /// The width of the line's glyphs, without trailing whitespace.
  pub width: f32,
}

/// Glyphs drawn from one font face, at one size, with one set of variations.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
  font: FontData,
  size: f32,
  coords: Arc<[i16]>,
  synthesis: Synthesis,
  glyphs: Arc<[Glyph]>,
}

impl GlyphRun {
  /// `glyphs` drawn from `font` at `size`, at the variable-font axis positions `coords`, with what
  /// `synthesis` says the renderer fakes.
  #[must_use]
  pub fn new(
    font: FontData,
    size: f32,
    coords: impl Into<Arc<[i16]>>,
    synthesis: Synthesis,
    glyphs: impl Into<Arc<[Glyph]>>,
  ) -> GlyphRun {
    GlyphRun {
      font,
      size,
      coords: coords.into(),
      synthesis,
      glyphs: glyphs.into(),
    }
  }

  /// The face the glyphs come from.
  #[must_use]
  pub fn font(&self) -> &FontData {
    &self.font
  }

  /// The font size, in logical pixels.
  #[must_use]
  pub fn size(&self) -> f32 {
    self.size
  }

  /// The variable-font axis positions, normalized to -1 to 1 in `F2Dot14` form, in the face's axis
  /// order. Empty for a face without axes or at its defaults.
  #[must_use]
  pub fn coords(&self) -> &[i16] {
    &self.coords
  }

  /// What the face lacks and the renderer fakes.
  #[must_use]
  pub fn synthesis(&self) -> Synthesis {
    self.synthesis
  }

  /// The glyphs, in drawing order.
  #[must_use]
  pub fn glyphs(&self) -> &[Glyph] {
    &self.glyphs
  }
}

/// What a face lacks and the renderer fakes: a bolder or a slanted look.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Synthesis {
  /// Whether to thicken the strokes, when a bold weight was asked of a family without a bold face.
  pub embolden: bool,
  /// The slant, in degrees, when italic was asked of a family without an italic face; 0 for none.
  pub skew: f32,
}

/// One glyph, placed.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Glyph {
  /// The glyph's id in its face.
  pub id: u32,
  /// The glyph's origin, from the paragraph's left edge.
  pub x: f32,
  /// The glyph's baseline, from the paragraph's top edge.
  pub y: f32,
}

/// The next id [`FontData::new`] hands out.
static NEXT_FONT_ID: AtomicU64 = AtomicU64::new(1);

/// A font file's bytes, shared: the shaper and every renderer read the same copy, and a clone
/// shares them too. It converts from a `&'static [u8]`, a `Vec<u8>`, a `Box<[u8]>` or an
/// `Arc<[u8]>`; [`new`](FontBytes::new) takes any other type that reads as bytes, such as a
/// memory map.
#[derive(Clone)]
pub struct FontBytes(Arc<dyn AsRef<[u8]> + Send + Sync>);

impl FontBytes {
  /// Shares `bytes`.
  #[must_use]
  pub fn new(bytes: impl AsRef<[u8]> + Send + Sync + 'static) -> FontBytes {
    FontBytes(Arc::new(bytes))
  }

  /// The shared allocation, for a font type that keeps its own handle to it, such as a renderer's.
  #[must_use]
  pub fn shared(&self) -> &Arc<dyn AsRef<[u8]> + Send + Sync> {
    &self.0
  }
}

impl AsRef<[u8]> for FontBytes {
  fn as_ref(&self) -> &[u8] {
    (*self.0).as_ref()
  }
}

impl Debug for FontBytes {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    write!(formatter, "FontBytes({} bytes)", self.as_ref().len())
  }
}

impl From<Arc<dyn AsRef<[u8]> + Send + Sync>> for FontBytes {
  fn from(shared: Arc<dyn AsRef<[u8]> + Send + Sync>) -> FontBytes {
    FontBytes(shared)
  }
}

impl From<&'static [u8]> for FontBytes {
  fn from(bytes: &'static [u8]) -> FontBytes {
    FontBytes::new(bytes)
  }
}

impl From<Vec<u8>> for FontBytes {
  fn from(bytes: Vec<u8>) -> FontBytes {
    FontBytes::new(bytes)
  }
}

impl From<Box<[u8]>> for FontBytes {
  fn from(bytes: Box<[u8]>) -> FontBytes {
    FontBytes::new(bytes)
  }
}

impl From<Arc<[u8]>> for FontBytes {
  fn from(bytes: Arc<[u8]>) -> FontBytes {
    FontBytes::new(bytes)
  }
}

/// A font file's bytes and the face in it that a glyph run draws from.
///
/// Each one gets an id when it's created, unique in the process and kept for as long as the bytes
/// live. Two are equal when their id and index are; the bytes are never compared. A renderer may
/// cache what it derives from a font, such as outlines or a typeface, keyed by id and index.
#[derive(Clone)]
pub struct FontData {
  bytes: FontBytes,
  index: u32,
  id: u64,
}

impl FontData {
  /// The face at `index` in the font file `bytes`, with a new id. A file with one face has only
  /// index 0; a collection (`.ttc`) has one index per face.
  #[must_use]
  pub fn new(bytes: impl Into<FontBytes>, index: u32) -> FontData {
    FontData {
      bytes: bytes.into(),
      index,
      id: NEXT_FONT_ID.fetch_add(1, Ordering::Relaxed),
    }
  }

  /// The font file's bytes.
  #[must_use]
  pub fn bytes(&self) -> &FontBytes {
    &self.bytes
  }

  /// The face's index in the font file.
  #[must_use]
  pub fn index(&self) -> u32 {
    self.index
  }

  /// The id this font data got when it was created.
  #[must_use]
  pub fn id(&self) -> u64 {
    self.id
  }
}

impl PartialEq for FontData {
  fn eq(&self, other: &FontData) -> bool {
    (self.id, self.index) == (other.id, other.index)
  }
}

impl Eq for FontData {}

impl Hash for FontData {
  fn hash<H: Hasher>(&self, state: &mut H) {
    (self.id, self.index).hash(state);
  }
}

impl Debug for FontData {
  /// Shows the id, the index and the length of the bytes, which have no `Debug` of their own.
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("FontData")
      .field("id", &self.id)
      .field("index", &self.index)
      .field("len", &self.bytes.as_ref().len())
      .finish()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use std::collections::HashSet;
  use std::thread;

  fn font() -> FontData {
    FontData::new(vec![0_u8; 16], 2)
  }

  fn run(font: &FontData) -> GlyphRun {
    let glyphs = [Glyph { id: 7, x: 0.0, y: 12.0 }, Glyph { id: 9, x: 6.5, y: 12.0 }];
    GlyphRun::new(font.clone(), 16.0, [100_i16], Synthesis::default(), glyphs)
  }

  fn paragraph(font: &FontData) -> Paragraph {
    let line = Line {
      top: 0.0,
      height: 20.0,
      baseline: 12.0,
      width: 13.0,
    };
    Paragraph::new(Size::new(13.0, 20.0), [line], [run(font)])
  }

  // A new paragraph runs left to right and no limit shaped it.
  #[test]
  fn new_paragraph_is_left_to_right_and_unlimited() {
    let paragraph = paragraph(&font());

    assert!(!paragraph.right_to_left());
    assert!(!paragraph.limited_width());
    assert!(!paragraph.limited_height());
  }

  // The builders set the direction and each limit flag on their own.
  #[test]
  fn builders_set_the_reports() {
    let font = font();

    assert!(paragraph(&font).with_right_to_left(true).right_to_left());
    let width_only = paragraph(&font).with_limited_width(true);
    assert!(width_only.limited_width() && !width_only.limited_height());
    let height_only = paragraph(&font).with_limited_height(true);
    assert!(!height_only.limited_width() && height_only.limited_height());
  }

  // Each new font data gets its own id, even from the same bytes, and a clone keeps it.
  #[test]
  fn font_data_compares_by_id() {
    let bytes = FontBytes::from(vec![1_u8, 2, 3]);
    let first = FontData::new(bytes.clone(), 0);
    let second = FontData::new(bytes, 0);

    assert_ne!(first.id(), second.id());
    assert_ne!(first, second);
    assert_eq!(first.clone(), first);
    assert_eq!(first.clone().id(), first.id());
  }

  // Font data created on several threads at once never shares an id.
  #[test]
  fn font_data_ids_are_unique_across_threads() {
    let workers: Vec<_> = (0..4)
      .map(|_| thread::spawn(|| (0..100).map(|_| font().id()).collect::<Vec<u64>>()))
      .collect();
    let ids: HashSet<u64> = workers.into_iter().flat_map(|worker| worker.join().unwrap()).collect();

    assert_eq!(ids.len(), 400);
  }

  // `Debug` shows the id, the index and the bytes' length, not the bytes.
  #[test]
  fn font_data_debug_shows_id_index_and_length() {
    let font = font();

    assert_eq!(
      format!("{font:?}"),
      format!("FontData {{ id: {}, index: 2, len: 16 }}", font.id())
    );
  }

  // A run hands back what it was built with.
  #[test]
  fn glyph_run_keeps_what_it_was_built_with() {
    let font = font();
    let run = run(&font);

    assert_eq!(run.font(), &font);
    assert_eq!(run.size(), 16.0);
    assert_eq!(run.coords(), [100]);
    assert_eq!(run.synthesis(), Synthesis::default());
    assert_eq!(run.glyphs().len(), 2);
  }

  // Paragraphs built from the same content and font data are equal; the same bytes under another
  // font data aren't.
  #[test]
  fn paragraphs_with_the_same_content_compare_equal() {
    let font = font();

    assert_eq!(paragraph(&font), paragraph(&font));
    assert_ne!(paragraph(&font), paragraph(&FontData::new(font.bytes().clone(), 2)));
  }

  // A clone shares its lines and runs instead of copying them.
  #[test]
  fn paragraph_clones_share_lines_and_runs() {
    let original = paragraph(&font());
    let copy = original.clone();

    assert!(std::ptr::eq(original.lines(), copy.lines()));
    assert!(std::ptr::eq(original.runs(), copy.runs()));
  }

  // The default paragraph has no lines and no glyphs, of size 0; a paragraph with a run has glyphs.
  #[test]
  fn default_paragraph_has_no_lines_and_no_glyphs() {
    let unshaped = Paragraph::default();

    assert!(!unshaped.has_glyphs());
    assert_eq!(unshaped.size(), Size::ZERO);
    assert!(unshaped.lines().is_empty());
    assert!(paragraph(&font()).has_glyphs());
  }

  // A paragraph whose runs hold no glyphs draws nothing, whatever its size.
  #[test]
  fn paragraph_of_glyphless_runs_has_no_glyphs() {
    let run = GlyphRun::new(font(), 16.0, Vec::<i16>::new(), Synthesis::default(), Vec::<Glyph>::new());

    assert!(!Paragraph::new(Size::new(10.0, 20.0), Vec::<Line>::new(), [run]).has_glyphs());
  }
}
