use crate::Span;
use std::error::Error;
use std::fmt::{Display, Formatter, Result};

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Severity {
  /// The file can't be used as written.
  Error,
  /// The file works, but something is likely wrong.
  Warning,
  /// Extra information.
  Note,
}

impl Display for Severity {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    formatter.write_str(match self {
      Severity::Error => "error",
      Severity::Warning => "warning",
      Severity::Note => "note",
    })
  }
}

/// A secondary place a diagnostic points at, with what to say about it.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Label {
  /// Where.
  pub span: Span,
  /// What to say there, such as "first defined here".
  pub message: String,
}

/// One problem found in a source file: what is wrong, where, and what to do about it.
///
/// # Examples
///
/// ```
/// use artic_diagnostics::{Diagnostic, SourceMap, Span};
///
/// let mut sources = SourceMap::new();
/// let file = sources.add("view.ui", "<Button Foo=\"1\"/>");
/// let diagnostic = Diagnostic::error(Span::new(file, 8, 11), "unknown property 'Foo' on Button")
///   .with_label("not a property of Button")
///   .with_help("remove the attribute, or check its spelling");
/// assert_eq!(diagnostic.message, "unknown property 'Foo' on Button");
/// ```
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Diagnostic {
  /// How serious it is.
  pub severity: Severity,
  /// What is wrong, as one sentence without a final period.
  pub message: String,
  /// The main place it refers to.
  pub span: Span,
  /// What to say under the main place, if anything.
  pub label: Option<String>,
  /// Other places it refers to.
  pub labels: Vec<Label>,
  /// What to do instead.
  pub help: Option<String>,
}

impl Diagnostic {
  /// Creates an error at `span`.
  pub fn error(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, span, message)
  }

  /// Creates a warning at `span`.
  pub fn warning(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Warning, span, message)
  }

  /// Creates a note at `span`.
  pub fn note(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Note, span, message)
  }

  /// Sets the text shown under the main place.
  #[must_use]
  pub fn with_label(mut self, label: impl Into<String>) -> Diagnostic {
    self.label = Some(label.into());
    self
  }

  /// Adds another place, with what to say about it.
  #[must_use]
  pub fn with_secondary(mut self, span: Span, message: impl Into<String>) -> Diagnostic {
    self.labels.push(Label {
      span,
      message: message.into(),
    });
    self
  }

  /// Sets what to do instead.
  #[must_use]
  pub fn with_help(mut self, help: impl Into<String>) -> Diagnostic {
    self.help = Some(help.into());
    self
  }

  fn new(severity: Severity, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
      severity,
      message: message.into(),
      span,
      label: None,
      labels: Vec::new(),
      help: None,
    }
  }
}

/// The diagnostics a run produced, used as the error type of `Result<_, Diagnostics>`.
///
/// It can hold warnings and notes next to errors. Its `Display` lists the messages without
/// positions; [`render`](Diagnostics::render) shows each one with its file, line and source text.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct Diagnostics(Vec<Diagnostic>);

impl Diagnostics {
  /// Creates an empty collection.
  #[must_use]
  pub fn new() -> Diagnostics {
    Diagnostics::default()
  }

  /// Adds a diagnostic.
  pub fn push(&mut self, diagnostic: Diagnostic) {
    self.0.push(diagnostic);
  }

  /// Whether there are no diagnostics at all.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }

  /// How many diagnostics there are.
  #[must_use]
  pub fn len(&self) -> usize {
    self.0.len()
  }

  /// Whether any diagnostic is an error.
  #[must_use]
  pub fn has_errors(&self) -> bool {
    self.0.iter().any(|diagnostic| diagnostic.severity == Severity::Error)
  }

  /// The diagnostics, in the order they were added.
  pub fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
    self.0.iter()
  }

  /// Returns `Ok(value)` if there is no error, and `Err(self)` otherwise.
  ///
  /// # Errors
  ///
  /// Returns the collection itself when it holds at least one error.
  pub fn into_result<T>(self, value: T) -> std::result::Result<T, Diagnostics> {
    if self.has_errors() { Err(self) } else { Ok(value) }
  }
}

impl From<Diagnostic> for Diagnostics {
  fn from(diagnostic: Diagnostic) -> Diagnostics {
    Diagnostics(vec![diagnostic])
  }
}

impl Extend<Diagnostic> for Diagnostics {
  fn extend<I: IntoIterator<Item = Diagnostic>>(&mut self, diagnostics: I) {
    self.0.extend(diagnostics);
  }
}

impl IntoIterator for Diagnostics {
  type Item = Diagnostic;
  type IntoIter = std::vec::IntoIter<Diagnostic>;

  fn into_iter(self) -> Self::IntoIter {
    self.0.into_iter()
  }
}

impl<'a> IntoIterator for &'a Diagnostics {
  type Item = &'a Diagnostic;
  type IntoIter = std::slice::Iter<'a, Diagnostic>;

  fn into_iter(self) -> Self::IntoIter {
    self.0.iter()
  }
}

impl Display for Diagnostics {
  /// Writes `severity: message`, one diagnostic per line.
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    for (index, diagnostic) in self.0.iter().enumerate() {
      if index > 0 {
        writeln!(formatter)?;
      }
      write!(formatter, "{}: {}", diagnostic.severity, diagnostic.message)?;
    }
    Ok(())
  }
}

impl Error for Diagnostics {}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::SourceMap;
  use pretty_assertions::assert_eq;

  /// Returns a span in a throwaway source map.
  fn span() -> Span {
    let mut sources = SourceMap::new();
    Span::new(sources.add("test.ui", "<A/>"), 0, 1)
  }

  // Warnings alone are not failures; one error is.
  #[test]
  fn into_result_fails_only_on_errors() {
    let mut diagnostics = Diagnostics::new();
    diagnostics.push(Diagnostic::warning(span(), "unused namespace"));
    assert_eq!(diagnostics.clone().into_result(7), Ok(7));

    diagnostics.push(Diagnostic::error(span(), "unknown element"));
    assert!(diagnostics.into_result(7).is_err());
  }

  // The builder methods fill in the label, the secondary places and the help text.
  #[test]
  fn builder_methods() {
    let diagnostic = Diagnostic::error(span(), "duplicate name 'email'")
      .with_label("duplicate")
      .with_secondary(span(), "first defined here")
      .with_help("rename one of the two elements");

    assert_eq!(diagnostic.label.as_deref(), Some("duplicate"));
    assert_eq!(diagnostic.labels[0].message, "first defined here");
    assert_eq!(diagnostic.help.as_deref(), Some("rename one of the two elements"));
  }

  // Display lists severity and message per diagnostic, one per line.
  #[test]
  fn display_lists_messages() {
    let mut diagnostics = Diagnostics::from(Diagnostic::error(span(), "unknown element 'Buton'"));
    diagnostics.extend([Diagnostic::warning(span(), "unused namespace 'x'")]);

    assert_eq!(
      diagnostics.to_string(),
      "error: unknown element 'Buton'\nwarning: unused namespace 'x'"
    );
  }
}
