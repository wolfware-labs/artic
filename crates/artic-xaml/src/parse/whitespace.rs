//! MS-XAML whitespace normalization of text content.

use crate::infoset::XamlNode;

/// How whitespace in an element's text content is treated, set by `xml:space` and inherited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Space {
  /// The default rules apply.
  Default,
  /// Text keeps its whitespace as written.
  Preserve,
}

/// Applies the default rules to one content list, setting each text value's `text` from its `raw`:
/// runs of whitespace collapse to one space, the list is trimmed at its start and end, and
/// whitespace-only text becomes empty.
pub(super) fn normalize(nodes: &mut [XamlNode]) {
  let last = nodes.len().saturating_sub(1);
  for (index, node) in nodes.iter_mut().enumerate() {
    let XamlNode::Value { text, raw, .. } = node else {
      continue;
    };
    if raw.chars().all(is_whitespace) {
      text.clear();
      continue;
    }
    let mut collapsed = collapse(raw);
    if index == last {
      collapsed.truncate(collapsed.trim_end_matches(' ').len());
    }
    if index == 0 {
      collapsed = collapsed.trim_start_matches(' ').to_owned();
    }
    *text = collapsed;
  }
}

/// Whether `character` is whitespace to XML: a space, tab, carriage return or line feed. Other
/// Unicode spaces, such as the non-breaking space, are content.
fn is_whitespace(character: char) -> bool {
  matches!(character, ' ' | '\t' | '\r' | '\n')
}

/// `text` with each run of whitespace replaced by one space.
fn collapse(text: &str) -> String {
  let mut collapsed = String::with_capacity(text.len());
  let mut in_run = false;
  for character in text.chars() {
    if is_whitespace(character) {
      if !in_run {
        collapsed.push(' ');
      }
      in_run = true;
    } else {
      collapsed.push(character);
      in_run = false;
    }
  }
  collapsed
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::infoset::TypeRef;
  use crate::parse::unresolved;
  use artic_diagnostics::{SourceMap, Span};
  use pretty_assertions::assert_eq;

  /// Normalizes a content list of `parts`, where `None` stands for an element, and returns each
  /// text's normalized form.
  fn normalized(parts: &[Option<&str>]) -> Vec<Option<String>> {
    let span = Span::new(SourceMap::new().add("test.xaml", ""), 0, 0);
    let mut nodes: Vec<_> = parts
      .iter()
      .map(|part| match part {
        Some(raw) => XamlNode::Value {
          text: (*raw).to_owned(),
          raw: (*raw).to_owned(),
          span,
        },
        None => XamlNode::Object {
          ty: TypeRef {
            prefix: None,
            name: "Element".to_owned(),
            namespace: unresolved(),
            span,
          },
          members: Vec::new(),
          namespaces: Vec::new(),
          span,
        },
      })
      .collect();
    normalize(&mut nodes);
    nodes
      .iter()
      .map(|node| match node {
        XamlNode::Value { text, .. } => Some(text.clone()),
        _ => None,
      })
      .collect()
  }

  // Runs of whitespace, tabs and line breaks included, collapse to one space.
  #[test]
  fn runs_collapse() {
    assert_eq!(normalized(&[Some("a \n\t b\r\nc")]), [Some("a b c".to_owned())]);
  }

  // The content is trimmed at its start and end only.
  #[test]
  fn content_is_trimmed_at_its_edges() {
    assert_eq!(normalized(&[Some("\n  Hello  world  \n")]), [Some("Hello world".to_owned())]);
  }

  // Text next to an element keeps the space on that side, so words stay apart.
  #[test]
  fn mixed_content_keeps_inner_spaces() {
    assert_eq!(
      normalized(&[Some("\n  Hello "), None, Some(" !\n")]),
      [Some("Hello ".to_owned()), None, Some(" !".to_owned())]
    );
  }

  // Whitespace-only text becomes empty wherever it is.
  #[test]
  fn whitespace_only_text_becomes_empty() {
    assert_eq!(
      normalized(&[Some("\n  "), None, Some(" \t "), None, Some("\n")]),
      [Some(String::new()), None, Some(String::new()), None, Some(String::new())]
    );
  }

  // Only XML's four whitespace characters are whitespace: a non-breaking space is content.
  #[test]
  fn non_breaking_spaces_are_content() {
    assert_eq!(
      normalized(&[Some("a\u{a0}\u{a0}b \u{a0}")]),
      [Some("a\u{a0}\u{a0}b \u{a0}".to_owned())]
    );
  }
}
