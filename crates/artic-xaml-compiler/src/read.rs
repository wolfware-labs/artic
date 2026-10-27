//! The XAML front end: reading a file into a [`Document`], each type looked up and each member
//! as written.

use crate::resolve::resolve;
use crate::unsupported;
use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, SourceMap};
use artic_registry::Registry;
use artic_view::Language;
use artic_view::document::{Content, Document, Element, Member, Problem, Text};
use artic_xaml::infoset::{Directive, Member as XamlMember, MemberKey, XamlNode};
use artic_xaml::namespaces::NamespaceMap;

/// XAML as a [`Language`]: reads `.xaml` files, finding types through the URIs `namespaces`
/// declares or a `using:` module.
#[derive(Debug, Clone)]
pub struct Xaml {
  namespaces: NamespaceMap,
}

impl Xaml {
  /// XAML with the URIs of `namespaces`, such as `artic::xaml::namespaces()`.
  #[must_use]
  pub fn new(namespaces: NamespaceMap) -> Xaml {
    Xaml { namespaces }
  }
}

impl Language for Xaml {
  fn extension(&self) -> &str {
    "xaml"
  }

  fn read(&self, sources: &SourceMap, file: SourceId, registry: &Registry) -> (Option<Document>, Diagnostics) {
    read(sources, file, registry, &self.namespaces)
  }

  fn crates(&self) -> Vec<&str> {
    self.namespaces.crates_named().collect()
  }
}

/// Reads `file` into a document, or returns the parser's errors. Every other problem is the
/// document's, for the checker to report where it reaches it.
fn read(
  sources: &SourceMap,
  file: SourceId,
  registry: &Registry,
  namespaces: &NamespaceMap,
) -> (Option<Document>, Diagnostics) {
  match artic_xaml::parse(sources, file) {
    Ok(document) => {
      let reader = Reader { registry, namespaces };
      let root = reader.element(&document.root);
      (Some(Document { root }), Diagnostics::new())
    }
    Err(diagnostics) => (None, diagnostics),
  }
}

/// What reading looks types up in.
struct Reader<'a> {
  registry: &'a Registry,
  namespaces: &'a NamespaceMap,
}

impl Reader<'_> {
  /// Reads an object element and the elements in its content.
  fn element(&self, node: &XamlNode) -> Element {
    let XamlNode::Object { ty, members, span, .. } = node else {
      unreachable!("only object elements are read as elements")
    };
    Element {
      ty: resolve(ty, self.registry, self.namespaces),
      span: *span,
      members: members.iter().filter_map(|member| self.member(member)).collect(),
    }
  }

  /// Reads one member, or `None` for a directive the document doesn't hold yet: `x:Class` and
  /// `x:FieldModifier` (#52), `x:DataType` (#94) and `x:Uid` (#168).
  fn member(&self, member: &XamlMember) -> Option<Member> {
    match &member.key {
      MemberKey::Property(name) => Some(Member::Property {
        name: name.clone(),
        span: member.span,
        value: value(member),
      }),
      MemberKey::Attached { .. } => Some(Member::Unsupported(unsupported::attached(member.span).into())),
      MemberKey::Directive(Directive::Name) => Some(Member::Name(name(member))),
      MemberKey::Directive(Directive::Key) => Some(Member::Unsupported(unsupported::resource_key(member.span).into())),
      MemberKey::Directive(Directive::Class | Directive::DataType | Directive::FieldModifier | Directive::Uid) => None,
      MemberKey::Content | MemberKey::Items => Some(Member::Content {
        span: member.span,
        nodes: member.value.iter().map(|node| self.content(node)).collect(),
      }),
    }
  }

  /// Reads one node written inside an element.
  fn content(&self, node: &XamlNode) -> Content {
    match node {
      XamlNode::Value { text, raw, span } => Content::Text {
        text: text.clone(),
        raw: raw.clone(),
        span: *span,
      },
      XamlNode::Object { .. } => Content::Element(self.element(node)),
      XamlNode::MarkupExtension { ty, span, .. } => Content::Unsupported(unsupported::markup_extension(ty, *span).into()),
    }
  }
}

/// The text a property member holds, or the problem with its first node that isn't text. The
/// span is the only node's, or the member's for several nodes or none.
fn value(member: &XamlMember) -> Result<Text, Problem> {
  let mut text = String::new();
  for node in &member.value {
    match node {
      XamlNode::Value { text: value, .. } => text.push_str(value),
      XamlNode::MarkupExtension { ty, span, .. } => return Err(unsupported::markup_extension(ty, *span).into()),
      XamlNode::Object { span, .. } => return Err(artic_view::unsupported::value_object(*span).into()),
    }
  }
  let span = match member.value.as_slice() {
    [only] => only.span(),
    _ => member.span,
  };
  Ok(Text { text, span })
}

/// The name `x:Name` gives, or the problem when it isn't written as text.
fn name(member: &XamlMember) -> Result<Text, Problem> {
  match member.value.as_slice() {
    [XamlNode::Value { text, span, .. }] => Ok(Text {
      text: text.clone(),
      span: *span,
    }),
    _ => Err(Diagnostic::error(member.span, "`x:Name` takes a name written as text").into()),
  }
}
