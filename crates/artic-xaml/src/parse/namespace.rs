//! What `xmlns` declarations bind prefixes to, and the declarations artic rejects.

use super::text::Decoded;
use super::{unresolved, unsupported};
use crate::infoset::{NamespaceDecl, XamlNamespace};
use crate::namespaces::{ARTIC_UI, MARKUP};
use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, Span};
use artic_ident::{is_identifier, is_keyword, is_path_keyword, snake_case};

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
const CLR_NAMESPACE: &str = "prefixes map to Rust modules, not .NET namespaces";

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
      unsupported::not_supported("clr-namespace:", CLR_NAMESPACE, declaration).with_help(format!(
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
      if let Some(problem) = segment_problem(segment) {
        return Some(Diagnostic::error(span, problem));
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
/// `my_app::views`. A namespace with no segments, or one `using:` would refuse, gives that example.
fn rust_path(clr_namespace: &str) -> String {
  let namespace = clr_namespace.split(';').next().unwrap_or_default();
  let segments: Vec<_> = namespace
    .split('.')
    .filter(|segment| !segment.is_empty())
    .map(snake_case)
    .collect();
  let usable = segments.first().is_some_and(|first| !is_path_keyword(first))
    && segments.iter().all(|segment| segment_problem(segment).is_none());
  if usable {
    segments.join("::")
  } else {
    "my_app::views".to_owned()
  }
}

/// Why `segment` can't be a segment of a `using:` path after the first, if it can't.
fn segment_problem(segment: &str) -> Option<String> {
  if segment.is_empty() {
    Some("empty segment in a `using:` path".to_owned())
  } else if !is_identifier(segment) {
    Some(format!("`{segment}` isn't a Rust identifier"))
  } else if is_keyword(segment) {
    Some(format!("`{segment}` is a Rust keyword"))
  } else {
    None
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

  // A namespace whose module path `using:` would refuse, a keyword or a segment that isn't an
  // identifier, gets the general example instead.
  #[test]
  fn refused_segments_fall_back() {
    assert_eq!(rust_path("MyApp.Type"), "my_app::views");
    assert_eq!(rust_path("My-App.Views"), "my_app::views");
  }
}
