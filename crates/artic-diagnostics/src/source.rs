use std::fmt;
use std::path::{Path, PathBuf};

/// Which source file a [`Span`] belongs to: an index into a [`SourceMap`].
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct SourceId(u32);

/// A byte range in one source file.
///
/// Positions are byte offsets into the file's UTF-8 text, cheap to store on every parsed node.
/// They become lines and columns only when a diagnostic is rendered.
///
/// Its `Debug` output is compact, `Span(0, 4..9)` for bytes `4..9` of file 0, since spans appear on
/// every node of a printed infoset.
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub struct Span {
  /// The file the range is in.
  pub source: SourceId,
  /// The first byte.
  pub start: usize,
  /// The byte after the last one. Equal to `start` for an empty span, such as "expected `>` here".
  pub end: usize,
}

impl Span {
  /// Creates the span of bytes `start..end` in `source`.
  #[must_use]
  pub const fn new(source: SourceId, start: usize, end: usize) -> Span {
    Span { source, start, end }
  }

  /// Whether the span covers no bytes.
  #[must_use]
  pub const fn is_empty(&self) -> bool {
    self.start == self.end
  }
}

impl fmt::Debug for Span {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Span({}, {}..{})", self.source.0, self.start, self.end)
  }
}

/// A line and a column, both counted from 1.
///
/// The column counts Unicode scalar values (Rust `char`s), not bytes: `é` and `😀` are one column
/// each. Tools that count differently, such as the language server with UTF-16, convert from this.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LineCol {
  /// The line, from 1.
  pub line: usize,
  /// The column, from 1.
  pub column: usize,
}

/// A source file's path and text, with an index of where its lines start.
#[derive(Debug)]
pub struct SourceFile {
  path: PathBuf,
  text: String,
  line_starts: Vec<usize>,
}

impl SourceFile {
  /// Creates a source file and indexes its lines.
  #[must_use]
  pub fn new(path: impl Into<PathBuf>, text: impl Into<String>) -> SourceFile {
    let text = text.into();
    let line_starts = std::iter::once(0)
      .chain(text.match_indices('\n').map(|(index, _)| index + 1))
      .collect();
    SourceFile {
      path: path.into(),
      text,
      line_starts,
    }
  }

  /// The file's path, as given.
  #[must_use]
  pub fn path(&self) -> &Path {
    &self.path
  }

  /// The file's text.
  #[must_use]
  pub fn text(&self) -> &str {
    &self.text
  }

  /// Returns the line and column of byte offset `byte`.
  ///
  /// An offset past the end of the text is treated as the end, and one inside a multi-byte
  /// character as that character's start. A `\r` before a `\n` belongs to the line ending, so
  /// Windows line endings don't shift columns.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_diagnostics::{LineCol, SourceFile};
  ///
  /// let file = SourceFile::new("view.ui", "<A>\r\n  <B é=\"1\"/>\r\n</A>");
  /// assert_eq!(file.line_col(10), LineCol { line: 2, column: 6 });
  /// ```
  #[must_use]
  pub fn line_col(&self, byte: usize) -> LineCol {
    let byte = self.char_start(byte);
    let line_index = self.line_starts.partition_point(|start| *start <= byte) - 1;
    let line_start = self.line_starts[line_index];
    LineCol {
      line: line_index + 1,
      column: self.text[line_start..byte].chars().count() + 1,
    }
  }

  /// Returns the byte offset of `position`, the inverse of [`line_col`](SourceFile::line_col).
  ///
  /// A line past the last one maps to the end of the text, and a column past the end of its line
  /// to the end of that line, line ending included.
  #[must_use]
  pub fn byte_offset(&self, position: LineCol) -> usize {
    let Some(start) = position.line.checked_sub(1).and_then(|index| self.line_starts.get(index)) else {
      return self.text.len();
    };
    let end = self.line_starts.get(position.line).copied().unwrap_or(self.text.len());
    self.text[*start..end]
      .char_indices()
      .nth(position.column.saturating_sub(1))
      .map_or(end, |(offset, _)| start + offset)
  }

  /// Returns the text of line `line` (from 1), without its line ending, or `""` past the last line.
  #[must_use]
  pub fn line_text(&self, line: usize) -> &str {
    let Some(start) = line.checked_sub(1).and_then(|index| self.line_starts.get(index)) else {
      return "";
    };
    let end = self.line_starts.get(line).copied().unwrap_or(self.text.len());
    self.text[*start..end].trim_end_matches('\n').trim_end_matches('\r')
  }

  fn char_start(&self, byte: usize) -> usize {
    let mut byte = byte.min(self.text.len());
    while !self.text.is_char_boundary(byte) {
      byte -= 1;
    }
    byte
  }
}

/// Every source file one run reads, addressed by [`SourceId`].
///
/// The parser works with a map holding one file; the compiler and the language server hold many.
#[derive(Debug, Default)]
pub struct SourceMap {
  files: Vec<SourceFile>,
}

impl SourceMap {
  /// Creates an empty map.
  #[must_use]
  pub fn new() -> SourceMap {
    SourceMap::default()
  }

  /// Adds a file and returns its id.
  ///
  /// # Panics
  ///
  /// Panics if the map would hold more than `u32::MAX` files.
  pub fn add(&mut self, path: impl Into<PathBuf>, text: impl Into<String>) -> SourceId {
    let id = SourceId(u32::try_from(self.files.len()).expect("fewer than u32::MAX source files"));
    self.files.push(SourceFile::new(path, text));
    id
  }

  /// Returns the file `id` refers to.
  ///
  /// # Panics
  ///
  /// Panics if `id` came from another map.
  #[must_use]
  pub fn get(&self, id: SourceId) -> &SourceFile {
    &self.files[id.0 as usize]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  /// Returns the line and column of `byte` in `text`.
  fn line_col(text: &str, byte: usize) -> (usize, usize) {
    let position = SourceFile::new("test.ui", text).line_col(byte);
    (position.line, position.column)
  }

  // A span prints as its file and byte range, on one line.
  #[test]
  fn debug_is_compact() {
    assert_eq!(format!("{:#?}", Span::new(SourceId(2), 4, 9)), "Span(2, 4..9)");
  }

  // ASCII offsets map to lines and columns from 1.
  #[test]
  fn ascii_positions() {
    let text = "<A>\n  <B/>\n</A>";

    assert_eq!(line_col(text, 0), (1, 1));
    assert_eq!(line_col(text, 6), (2, 3));
    assert_eq!(line_col(text, 11), (3, 1));
  }

  // Columns count characters, so multi-byte characters before a position count once each.
  #[test]
  fn multi_byte_characters_count_once() {
    let text = "<A é=\"😀\" b=\"1\"/>";
    let b = text.find(" b").expect("has b") + 1;

    assert_eq!(line_col(text, b), (1, 10));
  }

  // A byte inside a multi-byte character maps to that character.
  #[test]
  fn offset_inside_a_character_maps_to_its_start() {
    let text = "é";

    assert_eq!(line_col(text, 1), (1, 1));
  }

  // With CRLF line endings, the `\r` neither counts as a column nor shifts line numbers.
  #[test]
  fn crlf_line_endings() {
    let file = SourceFile::new("test.ui", "<A>\r\n  <B/>\r\n</A>");

    assert_eq!(file.line_col(7), LineCol { line: 2, column: 3 });
    assert_eq!(file.line_col(13), LineCol { line: 3, column: 1 });
    assert_eq!(file.line_text(2), "  <B/>");
  }

  // The end of the text, and offsets past it, map to just after the last character.
  #[test]
  fn end_of_file() {
    let text = "<A/>\n";

    assert_eq!(line_col(text, 5), (2, 1));
    assert_eq!(line_col(text, 99), (2, 1));
    assert_eq!(line_col("", 0), (1, 1));
  }

  // Line text has no line ending, and lines past the last one are empty.
  #[test]
  fn line_text() {
    let file = SourceFile::new("test.ui", "one\ntwo");

    assert_eq!(file.line_text(1), "one");
    assert_eq!(file.line_text(2), "two");
    assert_eq!(file.line_text(3), "");
    assert_eq!(file.line_text(0), "");
  }

  // Each added file gets its own id.
  #[test]
  fn source_map_hands_out_ids() {
    let mut sources = SourceMap::new();

    let first = sources.add("a.ui", "a");
    let second = sources.add("b.ui", "b");

    assert_eq!(sources.get(first).text(), "a");
    assert_eq!(sources.get(second).path(), Path::new("b.ui"));
  }

  // `byte_offset` undoes `line_col`, across multi-byte characters and CRLF line endings, and clamps
  // positions past the text.
  #[test]
  fn byte_offset_inverts_line_col() {
    let file = SourceFile::new("test.ui", "<A é=\"1\">\r\n  <B/>\r\n</A>");

    for byte in [0, 3, 5, 6, 12, 14, 18] {
      assert_eq!(file.byte_offset(file.line_col(byte)), byte, "byte {byte}");
    }
    assert_eq!(file.byte_offset(LineCol { line: 9, column: 1 }), file.text().len());
    assert_eq!(file.byte_offset(LineCol { line: 1, column: 99 }), 12);
  }
}
