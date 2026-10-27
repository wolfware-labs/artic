//! The XAML infoset: the tree both XAML back ends read, with no trace of XML syntax.
//!
//! An infoset keeps what the markup means and forgets how it was written: `Content="Hi"`,
//! `<Button.Content>Hi</Button.Content>` and `<Button>Hi</Button>` all become an object with one
//! member holding the text. Every piece keeps the [`Span`] it came from, so later stages can point
//! at it. Type references carry the namespace their prefix stands for, but turning them into Rust
//! types needs the registry, which this crate doesn't depend on.

use artic_diagnostics::{SourceId, Span};

/// A parsed XAML file.
#[derive(Debug, Clone, PartialEq)]
pub struct XamlDocument {
  /// The file, in the [`SourceMap`](artic_diagnostics::SourceMap) it was parsed from.
  pub source: SourceId,
  /// The root element, always an [`XamlNode::Object`].
  pub root: XamlNode,
}

/// One node of the infoset.
#[derive(Debug, Clone, PartialEq)]
pub enum XamlNode {
  /// An object element, such as `<Button Width="100"/>`: "create a `Button`".
  Object {
    /// The element's type, unresolved.
    ty: TypeRef,
    /// The object's members: attributes in order, then property elements in order, then the
    /// content, if any, as one member.
    members: Vec<Member>,
    /// The namespace prefixes this element declares with `xmlns` attributes.
    namespaces: Vec<NamespaceDecl>,
    /// The whole element, from `<` to its end tag.
    span: Span,
  },
  /// Text: an attribute value, or text and CDATA inside an element, with entities decoded.
  Value {
    /// The text. For text inside an element, after whitespace normalization: the default rules,
    /// unless `xml:space="preserve"` is in effect; whitespace-only text is then empty, which adds
    /// nothing. For an attribute value, the same as `raw`.
    text: String,
    /// The text with its whitespace as written, for types whose whitespace is significant.
    raw: String,
    /// Where it was written; its length can differ from `text`'s when entities were decoded.
    span: Span,
  },
  /// A markup extension, such as `{Binding Name}`: an attribute value, or an argument, that starts
  /// with `{`. A value that starts with the `{}` escape is a [`XamlNode::Value`] instead.
  MarkupExtension {
    /// The extension's type, unresolved.
    ty: TypeRef,
    /// Arguments given by position.
    positional: Vec<XamlNode>,
    /// Arguments given by name, such as `Mode=TwoWay`. Each sets a property of the extension, so
    /// it's a [`Member`] like an attribute: a [`MemberKey::Property`] key spanning the name, and
    /// one value.
    named: Vec<Member>,
    /// The whole extension, braces included.
    span: Span,
  },
}

impl XamlNode {
  /// A value whose whitespace isn't normalized, such as an attribute value: `text` and `raw` are
  /// the same.
  pub(crate) fn value(text: String, span: Span) -> XamlNode {
    XamlNode::Value {
      raw: text.clone(),
      text,
      span,
    }
  }

  /// Where the node was written.
  #[must_use]
  pub fn span(&self) -> Span {
    match self {
      XamlNode::Object { span, .. } | XamlNode::Value { span, .. } | XamlNode::MarkupExtension { span, .. } => *span,
    }
  }
}

/// One member of an object: which member, and the nodes it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
  /// Which member.
  pub key: MemberKey,
  /// What it holds, in document order: one value for an attribute, any number of nodes for a
  /// property element or for content.
  pub value: Vec<XamlNode>,
  /// Where the member is named: the attribute's name, or the property element's start tag. For
  /// content, the content itself.
  pub span: Span,
}

/// Which member of an object a [`Member`] sets.
#[derive(Debug, Clone, PartialEq)]
pub enum MemberKey {
  /// A property of the object's own type, such as `Width`.
  Property(String),
  /// A property that another type owns, set on this object, such as `Grid.Row`.
  Attached {
    /// The type that declares the property, unresolved.
    owner: TypeRef,
    /// The property's name.
    name: String,
  },
  /// An instruction to the XAML processor rather than a property, such as `x:Name`.
  Directive(Directive),
  /// Text and child objects written directly inside the element, in document order.
  Content,
  /// Children of a collection. The parser never produces it: whether content goes into a
  /// collection depends on the registry, so the back ends decide.
  Items,
}

/// An `x:` directive.
///
/// An attribute is a directive when its prefix stands for [`XamlNamespace::Markup`], whatever the
/// prefix is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Directive {
  /// `x:Name`
  Name,
  /// `x:Key`
  Key,
  /// `x:Class`
  Class,
  /// `x:DataType`
  DataType,
  /// `x:Uid`
  Uid,
  /// `x:FieldModifier`
  FieldModifier,
}

impl Directive {
  /// Every directive, in the order the spec lists them.
  pub const ALL: [Directive; 6] = [
    Directive::Name,
    Directive::Key,
    Directive::Class,
    Directive::DataType,
    Directive::Uid,
    Directive::FieldModifier,
  ];

  /// The name it's written with after the markup prefix, such as `Name` for `x:Name`.
  #[must_use]
  pub fn name(self) -> &'static str {
    match self {
      Directive::Name => "Name",
      Directive::Key => "Key",
      Directive::Class => "Class",
      Directive::DataType => "DataType",
      Directive::Uid => "Uid",
      Directive::FieldModifier => "FieldModifier",
    }
  }

  /// The directive written as `name` after the markup prefix, if `name` is one.
  #[must_use]
  pub fn from_name(name: &str) -> Option<Directive> {
    Directive::ALL.into_iter().find(|directive| directive.name() == name)
  }
}

/// A type named in markup, with its namespace resolved but not yet resolved to a Rust type.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeRef {
  /// The namespace prefix as written, such as `local` in `local:Gauge`, or `None` for the default
  /// namespace.
  pub prefix: Option<String>,
  /// The type's name.
  pub name: String,
  /// The namespace the prefix stands for where the name is written.
  pub namespace: XamlNamespace,
  /// Where the name was written.
  pub span: Span,
}

/// What a namespace prefix stands for: the vocabulary a type name comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum XamlNamespace {
  /// `https://artic.rs/markup`, the XAML language itself: `x:Name`, `{x:Null}`, `<x:String>`.
  Markup,
  /// A vocabulary URI declared by XAML types, such as `https://artic.rs/ui`.
  Uri(String),
  /// A Rust module path written as `using:my_app::views`, without the `using:`.
  Using(String),
}

/// A namespace prefix declared with `xmlns` or `xmlns:prefix`.
#[derive(Debug, Clone, PartialEq)]
pub struct NamespaceDecl {
  /// The declared prefix, or `None` for the default namespace (`xmlns="…"`).
  pub prefix: Option<String>,
  /// The namespace it maps to.
  pub uri: String,
  /// The whole `xmlns…="…"` attribute.
  pub span: Span,
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // Each directive is found by the name it's written with after `x:`, and every one is listed.
  #[test]
  fn directive_names_round_trip() {
    assert_eq!(Directive::ALL.len(), 6);
    for directive in Directive::ALL {
      assert_eq!(Directive::from_name(directive.name()), Some(directive));
    }
    assert_eq!(Directive::from_name("Shared"), None);
  }
}
