//! What `xmlns` declarations bind prefixes to, and the declarations artic rejects.

use super::text::Decoded;
use super::{unresolved, unsupported};
use crate::infoset::XamlNamespace;
use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, Span};
use artic_ident::{is_identifier, is_keyword, is_path_keyword, snake_case};

const MARKUP_URI: &str = "https://artic.rs/markup";
pub(super) const UI_URI: &str = "https://artic.rs/ui";

/// The namespace an `xmlns` or `xmlns:prefix` declaration (`prefix` is `None` for the default
/// namespace) binds, written at `declaration` with the value `uri`.
///
/// A rejected value is recorded in `diagnostics`, and still binds the prefix, so that its uses
/// aren't reported again: to what the author meant when that's clear, such as the markup namespace
/// for the Microsoft XAML URI.
pub(super) fn classify(
  prefix: Option<&str>,
  uri: &Decoded,
  file: SourceId,
  declaration: Span,
  diagnostics: &mut Diagnostics,
) -> XamlNamespace {
  let attribute = match prefix {
    Some(prefix) => format!("xmlns:{prefix}"),
    None => "xmlns".to_owned(),
  };
  // The table gives the message; the help names the author's own prefix.
  let mut reject = |construct: &str, help: String, meant: XamlNamespace| {
    let rejection = unsupported::rejection(construct, declaration).expect("rejected namespaces are in the table");
    diagnostics.push(rejection.with_help(help));
    meant
  };
  let text = uri.text.as_str();
  if let Some(path) = text.strip_prefix("using:") {
    let path = UsingPath {
      path,
      start: "using:".len(),
      uri,
      file,
    };
    return match path.problem(&attribute) {
      Some(diagnostic) => {
        diagnostics.push(diagnostic);
        unresolved()
      }
      None => XamlNamespace::Using(path.path.to_owned()),
    };
  }
  if let Some(namespace) = text.strip_prefix("clr-namespace:") {
    let help = format!(
      "artic maps prefixes to Rust modules; for example `{attribute}=\"using:{}\"`",
      rust_path(namespace)
    );
    return reject("clr-namespace:", help, unresolved());
  }
  match text {
    MARKUP_URI => XamlNamespace::Markup,
    "" => {
      diagnostics.push(Diagnostic::error(declaration, "a namespace can't be empty"));
      unresolved()
    }
    "http://schemas.microsoft.com/winfx/2006/xaml" => {
      reject(text, format!("use `{attribute}=\"{MARKUP_URI}\"`"), XamlNamespace::Markup)
    }
    "http://schemas.microsoft.com/winfx/2006/xaml/presentation" => reject(
      text,
      format!("use `{attribute}=\"{UI_URI}\"`"),
      XamlNamespace::Uri(UI_URI.to_owned()),
    ),
    _ => XamlNamespace::Uri(text.to_owned()),
  }
}

/// The path of a `using:` value, which starts at byte `start` of the decoded value `uri`.
struct UsingPath<'a> {
  path: &'a str,
  start: usize,
  uri: &'a Decoded,
  file: SourceId,
}

impl UsingPath<'_> {
  /// Why the path isn't a module path from a crate name, if it isn't.
  fn problem(&self, attribute: &str) -> Option<Diagnostic> {
    let from_crate = format!("write the full path from the crate name, like `{attribute}=\"using:my_app::views\"`");
    if self.path.is_empty() {
      return Some(Diagnostic::error(self.span(0, 0), "`using:` needs a Rust module path").with_help(from_crate));
    }
    let mut offset = 0;
    for (index, segment) in self.path.split("::").enumerate() {
      let span = self.span(offset, offset + segment.len());
      offset += segment.len() + "::".len();
      if index == 0 && (segment.is_empty() || is_path_keyword(segment)) {
        return Some(Diagnostic::error(span, "a `using:` path starts with a crate name").with_help(from_crate));
      }
      if segment.is_empty() {
        return Some(Diagnostic::error(span, "empty segment in a `using:` path"));
      }
      if !is_identifier(segment) {
        return Some(Diagnostic::error(span, format!("`{segment}` isn't a Rust identifier")));
      }
      if is_keyword(segment) {
        return Some(Diagnostic::error(span, format!("`{segment}` is a Rust keyword")));
      }
    }
    None
  }

  /// The source span of bytes `start..end` of the path.
  fn span(&self, start: usize, end: usize) -> Span {
    let offsets = &self.uri.offsets;
    Span::new(
      self.file,
      offsets.source_offset(self.start + start),
      offsets.source_offset(self.start + end),
    )
  }
}

/// A best-effort Rust module path for a .NET namespace: `MyApp.Views;assembly=MyApp` becomes
/// `my_app::views`.
fn rust_path(clr_namespace: &str) -> String {
  let namespace = clr_namespace.split(';').next().unwrap_or_default();
  let segments: Vec<_> = namespace
    .split('.')
    .filter(|segment| !segment.is_empty())
    .map(snake_case)
    .collect();
  if segments.is_empty() {
    "my_app::views".to_owned()
  } else {
    segments.join("::")
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // A `clr-namespace:` value becomes a module path; the assembly is dropped.
  #[test]
  fn clr_namespaces_become_module_paths() {
    assert_eq!(rust_path("MyApp.ViewModels;assembly=MyApp"), "my_app::view_models");
    assert_eq!(rust_path(";assembly=MyApp"), "my_app::views");
  }
}
