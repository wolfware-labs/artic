use crate::{Diagnostic, Diagnostics, SourceMap, Span};
use std::fmt::Write;

impl Diagnostic {
  /// Renders the diagnostic as plain text, with positions and source lines from `sources`.
  ///
  /// The first line is `path:line:col: severity: message`, the format build output and editors
  /// recognize. Then come the source line with the main place underlined by `^`, each secondary
  /// place underlined by `-` after a `:::` line naming its position, and the help text. A span
  /// that covers no bytes gets one `^`, and a span over several lines is underlined to the end of
  /// its first line.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_diagnostics::{Diagnostic, SourceMap, Span};
  ///
  /// let mut sources = SourceMap::new();
  /// let file = sources.add("view.ui", "<Button Foo=\"1\"/>");
  /// let diagnostic = Diagnostic::error(Span::new(file, 8, 11), "unknown property 'Foo' on Button");
  /// assert_eq!(
  ///   diagnostic.render(&sources),
  ///   "view.ui:1:9: error: unknown property 'Foo' on Button\n\
  ///    1 | <Button Foo=\"1\"/>\n  |         ^^^"
  /// );
  /// ```
  #[must_use]
  pub fn render(&self, sources: &SourceMap) -> String {
    let width = self
      .labels
      .iter()
      .map(|label| label.span)
      .chain([self.span])
      .map(|span| sources.get(span.source).line_col(span.start).line.to_string().len())
      .max()
      .unwrap_or(1);
    let mut output = String::new();
    let _ = write!(
      output,
      "{}: {}: {}",
      position(sources, self.span),
      self.severity,
      self.message
    );
    snippet(&mut output, sources, self.span, '^', self.label.as_deref(), width);
    for label in &self.labels {
      let _ = write!(output, "\n{:>width$} ::: {}", "", position(sources, label.span));
      snippet(&mut output, sources, label.span, '-', Some(&label.message), width);
    }
    if let Some(help) = &self.help {
      let _ = write!(output, "\nhelp: {help}");
    }
    output
  }
}

impl Diagnostics {
  /// Renders every diagnostic as [`Diagnostic::render`] does, separated by blank lines.
  #[must_use]
  pub fn render(&self, sources: &SourceMap) -> String {
    self
      .iter()
      .map(|diagnostic| diagnostic.render(sources))
      .collect::<Vec<_>>()
      .join("\n\n")
  }
}

/// `path:line:col` for the start of `span`.
fn position(sources: &SourceMap, span: Span) -> String {
  let file = sources.get(span.source);
  let start = file.line_col(span.start);
  format!("{}:{}:{}", file.path().display(), start.line, start.column)
}

/// Appends the source line `span` starts on and a line underlining the span with `mark`.
fn snippet(output: &mut String, sources: &SourceMap, span: Span, mark: char, label: Option<&str>, width: usize) {
  let file = sources.get(span.source);
  let start = file.line_col(span.start);
  let end = file.line_col(span.end);
  let text = file.line_text(start.line);
  let end_column = if end.line == start.line {
    end.column
  } else {
    text.chars().count() + 1
  };
  let indent: String = text
    .chars()
    .take(start.column - 1)
    .map(|character| if character == '\t' { '\t' } else { ' ' })
    .collect();
  let marks = mark.to_string().repeat(end_column.saturating_sub(start.column).max(1));
  let _ = write!(output, "\n{:>width$} | {text}", start.line);
  let _ = write!(output, "\n{:>width$} | {indent}{marks}", "");
  if let Some(label) = label {
    let _ = write!(output, " {label}");
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  /// Returns a source map with `text` as `views/login.ui`, and that file's id.
  fn login(text: &str) -> (SourceMap, crate::SourceId) {
    let mut sources = SourceMap::new();
    let file = sources.add("views/login.ui", text);
    (sources, file)
  }

  // The first line is `path:line:col: severity: message`.
  #[test]
  fn renders_path_line_col() {
    let text = format!("{}<Button Foo=\"1\"/>\n", "\n".repeat(11));
    let (sources, file) = login(&text);
    let start = text.find("Foo").expect("has Foo");

    let rendered = Diagnostic::error(Span::new(file, start, start + 3), "unknown property 'Foo' on Button").render(&sources);

    assert_eq!(
      rendered.lines().next(),
      Some("views/login.ui:12:9: error: unknown property 'Foo' on Button")
    );
  }

  // An empty span at the very end of the file renders one caret after the last character.
  #[test]
  fn span_at_end_of_file() {
    let (sources, file) = login("<A>");

    let rendered = Diagnostic::error(Span::new(file, 3, 3), "expected '</A>'").render(&sources);

    assert_eq!(rendered, "views/login.ui:1:4: error: expected '</A>'\n1 | <A>\n  |    ^");
  }

  // An empty file renders line 1, column 1, with an empty source line.
  #[test]
  fn empty_file() {
    let (sources, file) = login("");

    let rendered = Diagnostic::error(Span::new(file, 0, 0), "expected a root element").render(&sources);

    assert_eq!(rendered, "views/login.ui:1:1: error: expected a root element\n1 | \n  | ^");
  }

  // A span over several lines is underlined to the end of its first line.
  #[test]
  fn multi_line_span_underlines_its_first_line() {
    let (sources, file) = login("<A b=\"x\ny\"/>");

    let rendered = Diagnostic::error(Span::new(file, 3, 10), "bad value").render(&sources);

    assert_eq!(rendered, "views/login.ui:1:4: error: bad value\n1 | <A b=\"x\n  |    ^^^^");
  }

  // Underlines line up under text that has tabs and multi-byte characters before them.
  #[test]
  fn underline_aligns_after_tabs_and_wide_characters() {
    let (sources, file) = login("\t<A é=\"1\" b=\"2\"/>");
    let start = "\t<A é=\"1\" ".len();

    let rendered = Diagnostic::error(Span::new(file, start, start + 1), "x").render(&sources);

    assert_eq!(rendered.lines().nth(2), Some("  | \t         ^"));
  }

  // A diagnostic with a primary label, a secondary label on another line and help text.
  #[test]
  fn renders_labels_and_help() {
    let text = "<StackPanel>\n  <TextBox name=\"email\"/>\n  <TextBox name=\"email\"/>\n</StackPanel>\n";
    let (sources, file) = login(text);
    let first = text.find("name=").expect("first name");
    let second = text.rfind("name=").expect("second name");
    let length = "name=\"email\"".len();

    let rendered = Diagnostic::error(Span::new(file, second, second + length), "duplicate name 'email'")
      .with_label("duplicate")
      .with_secondary(Span::new(file, first, first + length), "first defined here")
      .with_help("rename one of the two elements")
      .render(&sources);

    insta::assert_snapshot!(rendered);
  }
}
