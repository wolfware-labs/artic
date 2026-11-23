//! The XAML front end: reading a file into a [`Document`], each type looked up and each member
//! as written.

use crate::resolve::resolve;
use crate::unsupported;
use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, SourceMap};
use artic_registry::Registry;
use artic_view::Language;
use artic_view::document::{Content, Document, Element, Member, Problem, Text, TypeUse};
use artic_xaml::infoset::{Directive, Member as XamlMember, MemberKey, TypeRef, XamlNode};
use artic_xaml::namespaces::NamespaceMap;

/// XAML as a [`Language`]: reads `.xaml` files, finding types through the URIs `namespaces`
/// declares or a `using:` module.
#[derive(Debug, Clone)]
pub struct Xaml {
  namespaces: NamespaceMap,
  app_crate: Option<String>,
}

impl Xaml {
  /// XAML with the URIs of `namespaces`, such as `artic::xaml::namespaces()`.
  #[must_use]
  pub fn new(namespaces: NamespaceMap) -> Xaml {
    Xaml {
      namespaces,
      app_crate: None,
    }
  }

  /// XAML read for the package whose build script runs, such as `env!("CARGO_PKG_NAME")`, a `-`
  /// read as `_`: a type its views look up in that crate gets help saying it doesn't exist yet.
  #[must_use]
  pub fn with_app_crate(self, name: &str) -> Xaml {
    Xaml {
      app_crate: Some(name.replace('-', "_")),
      ..self
    }
  }
}

impl Language for Xaml {
  fn extension(&self) -> &str {
    "xaml"
  }

  fn read(&self, sources: &SourceMap, file: SourceId, registry: &Registry) -> (Option<Document>, Diagnostics) {
    read(sources, file, registry, &self.namespaces, self.app_crate.as_deref())
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
  app_crate: Option<&str>,
) -> (Option<Document>, Diagnostics) {
  match artic_xaml::parse(sources, file) {
    Ok(document) => {
      let reader = Reader {
        registry,
        namespaces,
        app_crate,
      };
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
  app_crate: Option<&'a str>,
}

impl Reader<'_> {
  /// Reads an object element and the elements in its content.
  fn element(&self, node: &XamlNode) -> Element {
    let XamlNode::Object { ty, members, span, .. } = node else {
      unreachable!("only object elements are read as elements")
    };
    let resolved = resolve(ty, self.registry, self.namespaces, self.app_crate);
    let members = members
      .iter()
      .filter_map(|member| self.member(member, ty, &resolved))
      .collect();
    Element {
      ty: resolved,
      span: *span,
      members,
    }
  }

  /// Reads one member of an element written as `written` and resolved to `ty`, or `None` for a
  /// directive the document doesn't hold yet: `x:DataType` (#94) and `x:Uid` (#168). A dotted
  /// member is the element's own when its owner is the element's type, compared as resolved, or as
  /// written when either isn't found.
  fn member(&self, member: &XamlMember, written: &TypeRef, ty: &Result<TypeUse, Problem>) -> Option<Member> {
    match &member.key {
      MemberKey::Property(name) => Some(property(name, member)),
      MemberKey::Attached { owner, name } => {
        let resolved = resolve(owner, self.registry, self.namespaces, self.app_crate);
        let own_type = match (&resolved, ty) {
          (Ok(owner), Ok(ty)) => owner.info.rust_path == ty.info.rust_path,
          _ => owner.is_same_type(written),
        };
        Some(if own_type && !is_attached(ty, name) {
          property(name, member)
        } else {
          Member::Attached {
            owner: resolved,
            name: name.clone(),
            span: member.span,
            value: value(member),
          }
        })
      }
      MemberKey::Directive(Directive::Name) => Some(Member::Name(text(member, "`x:Name` takes a name written as text"))),
      MemberKey::Directive(Directive::Class) => Some(Member::CodeType(text(
        member,
        "`x:Class` takes a type's path written as text",
      ))),
      MemberKey::Directive(Directive::FieldModifier) => Some(Member::FieldVisibility(text(
        member,
        "`x:FieldModifier` takes a visibility written as text",
      ))),
      MemberKey::Directive(Directive::Key) => Some(Member::Unsupported(unsupported::resource_key(member.span).into())),
      MemberKey::Directive(Directive::DataType | Directive::Uid) => None,
      MemberKey::Content | MemberKey::Items => Some(Member::Content {
        span: member.span,
        nodes: member.value.iter().map(|node| self.content(node)).collect(),
      }),
    }
  }

  /// Reads one node written inside an element.
  fn content(&self, node: &XamlNode) -> Content {
    match node {
      XamlNode::Value { text, raw, trimmed, .. } => Content::Text {
        text: text.clone(),
        raw: raw.clone(),
        span: *trimmed,
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

/// The text a directive holds, or `problem` when it isn't written as text.
fn text(member: &XamlMember, problem: &str) -> Result<Text, Problem> {
  match member.value.as_slice() {
    [XamlNode::Value { text, span, .. }] => Ok(Text {
      text: text.clone(),
      span: *span,
    }),
    _ => Err(Diagnostic::error(member.span, problem).into()),
  }
}

/// The property `name` of the element `member` is written on, set to what `member` holds.
fn property(name: &str, member: &XamlMember) -> Member {
  Member::Property {
    name: name.to_owned(),
    span: member.span,
    value: value(member),
  }
}

/// Whether `ty`, when found, has an attached property `name` and no property of its own by that
/// name: `Owner.Name` on an element of the owner's own type sets that attached property, as a grid
/// in a grid sets its `Grid.Row`, and the element's own property otherwise.
fn is_attached(ty: &Result<TypeUse, Problem>, name: &str) -> bool {
  ty.as_ref()
    .is_ok_and(|ty| ty.info.property(name).is_none() && ty.info.attached_property(name).is_some())
}
