//! What `xmlns` declarations bind prefixes to, and the declarations artic rejects.

use super::text::Decoded;
use super::{unresolved, unsupported};
use crate::infoset::{NamespaceDecl, XamlNamespace};
use crate::namespaces::{ARTIC_UI, MARKUP};
use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, Span};
use artic_ident::{is_identifier, is_keyword, is_path_keyword, snake_case};
use std::ops::Range;

/// A namespace URI artic rejects: why, and the artic URI it stands for, which its prefix binds so
/// that its uses aren't reported again.
struct Rejected {
  uri: &'static str,
  reason: &'static str,
  meant: &'static str,
}

/// The Microsoft XAML URIs, which artic replaces with its own.
const REJECTED: [Rejected; 2] = [
  Rejected {
    uri: "http://schemas.microsoft.com/winfx/2006/xaml",
    reason: "artic's XAML language namespace has its own URI",
    meant: MARKUP,
  },
  Rejected {
    uri: "http://schemas.microsoft.com/winfx/2006/xaml/presentation",
    reason: "artic's controls are their own vocabulary",
    meant: ARTIC_UI,
  },
];

/// Why a `clr-namespace:` value is rejected.
const CLR_NAMESPACE_REASON: &str = "prefixes map to Rust modules, not .NET namespaces";

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
  let attribute = NamespaceDecl::attribute(prefix);
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
    diagnostics.push(
      unsupported::not_supported("clr-namespace:", CLR_NAMESPACE_REASON, declaration).with_help(format!(
        "artic maps prefixes to Rust modules; for example `{attribute}=\"using:{}\"`",
        rust_path(namespace)
      )),
    );
    return unresolved();
  }
  if text.is_empty() {
    diagnostics.push(Diagnostic::error(declaration, "a namespace can't be empty"));
    return unresolved();
  }
  match REJECTED.iter().find(|rejected| rejected.uri == text) {
    Some(rejected) => {
      diagnostics.push(
        unsupported::not_supported(rejected.uri, rejected.reason, declaration)
          .with_help(format!("use `{attribute}=\"{}\"`", rejected.meant)),
      );
      bound(rejected.meant)
    }
    None => bound(text),
  }
}

/// The namespace `uri` names: the markup namespace, or a vocabulary URI.
fn bound(uri: &str) -> XamlNamespace {
  if uri == MARKUP {
    XamlNamespace::Markup
  } else {
    XamlNamespace::Uri(uri.to_owned())
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
    let (refusal, range) = refusal(self.path)?;
    let span = self.span(range.start, range.end);
    let segment = &self.path[range];
    let from_crate = || format!("write the full path from the crate name, like `{attribute}=\"using:my_app::views\"`");
    Some(match refusal {
      Refusal::Empty => Diagnostic::error(span, "`using:` needs a Rust module path").with_help(from_crate()),
      Refusal::NoCrate => Diagnostic::error(span, "a `using:` path starts with a crate name").with_help(from_crate()),
      Refusal::EmptySegment => Diagnostic::error(span, "empty segment in a `using:` path"),
      Refusal::NotIdentifier => Diagnostic::error(span, format!("`{segment}` isn't a Rust identifier")),
      Refusal::Keyword => Diagnostic::error(span, format!("`{segment}` is a Rust keyword")),
    })
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
/// `my_app::views`. A namespace with no segments, or one `using:` would refuse, gives that example.
fn rust_path(clr_namespace: &str) -> String {
  let namespace = clr_namespace.split(';').next().unwrap_or_default();
  let segments: Vec<_> = namespace
    .split('.')
    .filter(|segment| !segment.is_empty())
    .map(snake_case)
    .collect();
  let path = segments.join("::");
  if refusal(&path).is_none() {
    path
  } else {
    "my_app::views".to_owned()
  }
}

/// Why the parser refuses a `using:` path.
enum Refusal {
  /// `using:` with nothing after it.
  Empty,
  /// The first segment isn't a crate name: empty, or `crate`, `self`, `super` or `Self`.
  NoCrate,
  /// A segment after the first is empty.
  EmptySegment,
  /// A segment has a character no Rust identifier has, such as `-`, or starts with a digit.
  NotIdentifier,
  /// A segment is a Rust keyword, such as `type`, which `using:` can't write raw.
  Keyword,
}

/// The first reason the parser refuses `path` as a `using:` path, with the bytes of `path` it's at.
fn refusal(path: &str) -> Option<(Refusal, Range<usize>)> {
  if path.is_empty() {
    return Some((Refusal::Empty, 0..0));
  }
  let mut start = 0;
  for (index, segment) in path.split("::").enumerate() {
    let range = start..start + segment.len();
    start = range.end + "::".len();
    let refusal = if index == 0 && (segment.is_empty() || is_path_keyword(segment)) {
      Refusal::NoCrate
    } else if segment.is_empty() {
      Refusal::EmptySegment
    } else if !is_identifier(segment) {
      Refusal::NotIdentifier
    } else if is_keyword(segment) {
      Refusal::Keyword
    } else {
      continue;
    };
    return Some((refusal, range));
  }
  None
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

  // A namespace whose module path `using:` would refuse, a keyword or a segment that isn't an
  // identifier, gets the general example instead.
  #[test]
  fn refused_segments_fall_back() {
    assert_eq!(rust_path("MyApp.Type"), "my_app::views");
    assert_eq!(rust_path("My-App.Views"), "my_app::views");
  }
}
