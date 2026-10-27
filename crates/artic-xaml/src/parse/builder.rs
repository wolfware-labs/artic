//! The builder that turns tokens into infoset nodes.

use super::text::{Decoded, Kind, decode, normalize_line_endings};
use super::whitespace::{self, Space};
use super::{Fatal, duplicate_of, markup, namespace, unresolved, unsupported};
use crate::infoset::{Directive, Member, MemberKey, NamespaceDecl, TypeRef, XamlDocument, XamlNamespace, XamlNode};
use crate::namespaces::ARTIC_UI;
use artic_diagnostics::{Diagnostic, Diagnostics, LineCol, SourceFile, SourceId, Span};
use std::collections::HashSet;
use xmlparser::{ElementEnd, StrSpan, Token, Tokenizer};

/// A prefix bound by an `xmlns` attribute, `None` for the default namespace, and what it stands for.
type Binding = (Option<String>, XamlNamespace);

/// An element whose end tag hasn't been reached yet.
struct Open {
  /// The tag name as written, to match the end tag.
  tag: String,
  /// Where the element starts.
  start: usize,
  /// The start tag's name, for "opened here" labels.
  tag_span: Span,
  /// The prefixes this element's `xmlns` attributes bind.
  scope: Vec<Binding>,
  /// How whitespace in its text is treated: its own `xml:space`, or its parent's.
  space: Space,
  /// Text and CDATA read since the last element boundary: the decoded text and its span.
  pending_text: Option<(String, usize, usize)>,
  kind: OpenKind,
}

enum OpenKind {
  Object {
    ty: TypeRef,
    members: Vec<Member>,
    namespaces: Vec<NamespaceDecl>,
    content: Vec<XamlNode>,
  },
  PropertyElement {
    key: MemberKey,
    value: Vec<XamlNode>,
  },
}

/// A start tag being read. Its attributes are kept as written until the tag ends: only then are all
/// its `xmlns` declarations known, and they apply to the whole element.
struct StartTag<'a> {
  prefix: StrSpan<'a>,
  local: StrSpan<'a>,
  span: StrSpan<'a>,
  attributes: Vec<Attribute<'a>>,
}

struct Attribute<'a> {
  prefix: StrSpan<'a>,
  local: StrSpan<'a>,
  value: StrSpan<'a>,
}

pub(super) struct Builder<'a> {
  file: SourceId,
  source: &'a SourceFile,
  diagnostics: Diagnostics,
  stack: Vec<Open>,
  root: Option<XamlNode>,
  start_tag: Option<StartTag<'a>>,
  /// The prefixes already reported as undeclared, `None` for the default namespace, so that each is
  /// reported once.
  undeclared: HashSet<Option<String>>,
}

impl<'a> Builder<'a> {
  pub(super) fn new(file: SourceId, source: &'a SourceFile) -> Builder<'a> {
    Builder {
      file,
      source,
      diagnostics: Diagnostics::new(),
      stack: Vec::new(),
      root: None,
      start_tag: None,
      undeclared: HashSet::new(),
    }
  }

  /// Reads the whole file into its infoset, or returns every problem found.
  pub(super) fn run(mut self) -> Result<XamlDocument, Diagnostics> {
    let text: &'a str = self.source.text();
    for token in Tokenizer::from(text) {
      let step = match token {
        Ok(token) => self.token(token),
        Err(error) => Err(self.tokenizer_error(&error)),
      };
      if step.is_err() {
        return Err(self.diagnostics);
      }
    }
    self.finish()
  }

  /// Records a tokenizer error, at the position the tokenizer reports.
  fn tokenizer_error(&mut self, error: &xmlparser::Error) -> Fatal {
    let position = error.pos();
    let byte = self.source.byte_offset(LineCol {
      line: position.row as usize,
      column: position.col as usize,
    });
    let span = self.span(byte, byte);
    let diagnostic = if self.stack.is_empty() && self.root.is_some() {
      Diagnostic::error(span, "expected one root element, found more content after it")
        .with_help("only comments and processing instructions may follow the root element")
    } else {
      Diagnostic::error(span, format!("malformed XML: {error}"))
    };
    self.fatal(diagnostic)
  }

  /// Handles one token. A fatal error stops parsing: the document's structure can't be trusted after
  /// it.
  fn token(&mut self, token: Token<'a>) -> Result<(), Fatal> {
    match token {
      Token::ElementStart { prefix, local, span } => self.element_start(prefix, local, span),
      Token::Attribute {
        prefix, local, value, ..
      } => {
        let start_tag = self
          .start_tag
          .as_mut()
          .expect("the tokenizer reads attributes only in a start tag");
        start_tag.attributes.push(Attribute { prefix, local, value });
        Ok(())
      }
      Token::ElementEnd { end, span } => match end {
        ElementEnd::Open => {
          self.end_start_tag();
          Ok(())
        }
        ElementEnd::Empty => {
          self.end_start_tag();
          self.close(span.end())
        }
        ElementEnd::Close(prefix, local) => self.end_tag(prefix, local, span),
      },
      Token::Text { text } => {
        self.text(text);
        Ok(())
      }
      Token::Cdata { text, span } => {
        self.add_text(normalize_line_endings(text.as_str()), span.start(), span.end());
        Ok(())
      }
      Token::DtdStart { span, .. } | Token::EmptyDtd { span, .. } => Err(
        self.fatal(
          Diagnostic::error(self.span_of(span), "DTDs aren't supported in XAML")
            .with_help("remove the `<!DOCTYPE …>` declaration"),
        ),
      ),
      Token::Declaration { .. }
      | Token::ProcessingInstruction { .. }
      | Token::Comment { .. }
      | Token::EntityDeclaration { .. }
      | Token::DtdEnd { .. } => Ok(()),
    }
  }

  fn element_start(&mut self, prefix: StrSpan<'a>, local: StrSpan<'a>, span: StrSpan<'a>) -> Result<(), Fatal> {
    self.flush_text();
    if local.as_str().contains('.') {
      let tag_span = self.span(span.start() + 1, local.end());
      let Some(parent) = self.stack.last() else {
        return Err(self.fatal(Diagnostic::error(tag_span, "the root element can't be a property element")));
      };
      if let OpenKind::PropertyElement { .. } = parent.kind {
        return Err(
          self.fatal(
            Diagnostic::error(tag_span, "a property element can't contain a property element")
              .with_secondary(parent.tag_span, "inside this property element"),
          ),
        );
      }
    }
    self.start_tag = Some(StartTag {
      prefix,
      local,
      span,
      attributes: Vec::new(),
    });
    Ok(())
  }

  /// Opens the element whose start tag just ended: binds its `xmlns` declarations, then resolves its
  /// type and attributes against them.
  fn end_start_tag(&mut self) {
    let StartTag {
      prefix,
      local,
      span,
      attributes,
    } = self.start_tag.take().expect("an element starts before its start tag ends");
    let tag_span = self.span(span.start() + 1, local.end());
    let type_prefix = (!prefix.as_str().is_empty()).then(|| prefix.as_str().to_owned());
    let mut space = self.stack.last().map_or(Space::Default, |open| open.space);
    let (scope, kind) = if let Some((owner, name)) = local.as_str().split_once('.') {
      for attribute in &attributes {
        let name_span = self.span(name_start(attribute), attribute.local.end());
        self.diagnostics.push(
          Diagnostic::error(name_span, "a property element can't have attributes")
            .with_secondary(tag_span, "on this property element"),
        );
      }
      let mut owner_type = TypeRef {
        prefix: type_prefix,
        name: owner.to_owned(),
        namespace: unresolved(),
        span: self.span(tag_span.start, local.start() + owner.len()),
      };
      self.resolve_type(&[], &mut owner_type);
      let Some(Open {
        kind: OpenKind::Object { ty: parent, .. },
        ..
      }) = self.stack.last()
      else {
        unreachable!("a property element is always opened inside an object element");
      };
      let key = if same_type(parent, &owner_type) {
        MemberKey::Property(name.to_owned())
      } else {
        MemberKey::Attached {
          owner: owner_type,
          name: name.to_owned(),
        }
      };
      (Vec::new(), OpenKind::PropertyElement { key, value: Vec::new() })
    } else {
      let (scope, namespaces, attributes) = self.declarations(attributes);
      let mut ty = TypeRef {
        prefix: type_prefix,
        name: local.as_str().to_owned(),
        namespace: unresolved(),
        span: tag_span,
      };
      self.resolve_type(&scope, &mut ty);
      let mut members = Vec::new();
      for attribute in attributes {
        if (attribute.prefix.as_str(), attribute.local.as_str()) == ("xml", "space") {
          if let Some(own) = self.xml_space(attribute.value) {
            space = own;
          }
          continue;
        }
        let Some(member) = self.member(&scope, &ty, attribute) else {
          continue;
        };
        match duplicate_of(&members, &member) {
          Some(duplicate) => self.diagnostics.push(duplicate),
          None => members.push(member),
        }
      }
      let kind = OpenKind::Object {
        ty,
        members,
        namespaces,
        content: Vec::new(),
      };
      (scope, kind)
    };
    self.stack.push(Open {
      tag: qualified(prefix, local),
      start: span.start(),
      tag_span,
      scope,
      space,
      pending_text: None,
      kind,
    });
  }

  /// The mode an `xml:space` value sets, or `None` after recording that it's neither `default` nor
  /// `preserve`.
  fn xml_space(&mut self, value: StrSpan) -> Option<Space> {
    match self.decode_value(value)?.text.as_str() {
      "default" => Some(Space::Default),
      "preserve" => Some(Space::Preserve),
      _ => {
        let span = self.span_of(value);
        self
          .diagnostics
          .push(Diagnostic::error(span, "`xml:space` must be `default` or `preserve`"));
        None
      }
    }
  }

  /// Splits an object element's `xmlns` declarations from its other attributes, returning the
  /// prefixes they bind, the declarations as written, and the other attributes.
  fn declarations(&mut self, attributes: Vec<Attribute<'a>>) -> (Vec<Binding>, Vec<NamespaceDecl>, Vec<Attribute<'a>>) {
    let mut scope = Vec::new();
    let mut namespaces: Vec<NamespaceDecl> = Vec::new();
    let mut others = Vec::new();
    for attribute in attributes {
      let prefix = match (attribute.prefix.as_str(), attribute.local.as_str()) {
        ("", "xmlns") => None,
        ("xmlns", prefix) => Some(prefix.to_owned()),
        _ => {
          others.push(attribute);
          continue;
        }
      };
      let span = self.span(name_start(&attribute), attribute.value.end() + 1);
      if let Some(reserved @ ("xml" | "xmlns")) = prefix.as_deref() {
        self
          .diagnostics
          .push(Diagnostic::error(span, format!("the `{reserved}` prefix is reserved")));
        continue;
      }
      if let Some(first) = namespaces.iter().find(|declared| declared.prefix == prefix) {
        let message = match &prefix {
          Some(prefix) => format!("prefix `{prefix}` is declared twice on this element"),
          None => "the default namespace is declared twice on this element".to_owned(),
        };
        self
          .diagnostics
          .push(Diagnostic::error(span, message).with_secondary(first.span, "first declared here"));
        continue;
      }
      let Some(uri) = self.decode_value(attribute.value) else {
        continue;
      };
      let namespace = namespace::classify(prefix.as_deref(), &uri, self.file, span, &mut self.diagnostics);
      scope.push((prefix.clone(), namespace));
      namespaces.push(NamespaceDecl {
        prefix,
        uri: uri.text,
        span,
      });
    }
    (scope, namespaces, others)
  }

  /// Turns an attribute of an object element of type `ty` into a member, with `scope` the
  /// element's own bindings. Returns `None` after recording why it can't be one.
  fn member(&mut self, scope: &[Binding], ty: &TypeRef, attribute: Attribute<'a>) -> Option<Member> {
    let Attribute { prefix, local, value } = attribute;
    let name_span = self.span(name_start(&attribute), local.end());
    let decoded = self.decode_value(value)?;
    let value_node = if decoded.text.starts_with('{') {
      let mut node = markup::parse(&decoded, self.file, &mut self.diagnostics)?;
      self.resolve_node(scope, &mut node);
      node
    } else {
      XamlNode::value(decoded.text, self.span_of(value))
    };
    let key = match (prefix.as_str(), local.as_str()) {
      (prefix, name) if name.contains('.') => {
        let (owner, property) = name.split_once('.').expect("contains a dot");
        let mut owner_type = TypeRef {
          prefix: (!prefix.is_empty()).then(|| prefix.to_owned()),
          name: owner.to_owned(),
          namespace: unresolved(),
          span: self.span(name_span.start, name_span.start + prefix_len(prefix) + owner.len()),
        };
        self.resolve_type(scope, &mut owner_type);
        if same_type(ty, &owner_type) {
          MemberKey::Property(property.to_owned())
        } else {
          MemberKey::Attached {
            owner: owner_type,
            name: property.to_owned(),
          }
        }
      }
      ("", name) => MemberKey::Property(name.to_owned()),
      (prefix, name) => {
        let prefix_span = self.span(name_span.start, name_span.start + prefix.len());
        let is_markup = prefix != "xml" && self.resolve_prefix(scope, Some(prefix), prefix_span)? == XamlNamespace::Markup;
        if !is_markup {
          self.diagnostics.push(
            Diagnostic::error(name_span, format!("`{prefix}:{name}` names no property")).with_help(format!(
              "a prefixed attribute must name an attached property, like `{prefix}:Owner.{name}`"
            )),
          );
          return None;
        }
        let Some(directive) = directive(name) else {
          let problem = unsupported::markup_problem(Some(prefix), name, name_span);
          self.diagnostics.push(problem);
          return None;
        };
        MemberKey::Directive(directive)
      }
    };
    Some(Member {
      key,
      value: vec![value_node],
      span: name_span,
    })
  }

  /// Decodes an attribute value, or records why it can't be decoded.
  fn decode_value(&mut self, value: StrSpan) -> Option<Decoded> {
    match decode(value.as_str(), value.start(), Kind::Attribute) {
      Ok(decoded) => Some(decoded),
      Err(error) => {
        let span = self.span(error.start, error.end);
        self.diagnostics.push(Diagnostic::error(span, error.message));
        None
      }
    }
  }

  /// Resolves the namespaces of the markup extensions in `node`.
  fn resolve_node(&mut self, scope: &[Binding], node: &mut XamlNode) {
    if let XamlNode::MarkupExtension {
      ty, positional, named, ..
    } = node
    {
      self.resolve_type(scope, ty);
      for argument in positional
        .iter_mut()
        .chain(named.iter_mut().flat_map(|member| member.value.iter_mut()))
      {
        self.resolve_node(scope, argument);
      }
    }
  }

  /// Sets the namespace of `ty` to what its prefix stands for, looking in `scope` first, then in the
  /// open elements from the innermost out, and reports it if artic doesn't support that type.
  fn resolve_type(&mut self, scope: &[Binding], ty: &mut TypeRef) {
    let span = match &ty.prefix {
      Some(prefix) => self.span(ty.span.start, ty.span.start + prefix.len()),
      None => ty.span,
    };
    if let Some(namespace) = self.resolve_prefix(scope, ty.prefix.as_deref(), span) {
      ty.namespace = namespace;
    }
    if let Some(problem) = unsupported::type_problem(ty) {
      self.diagnostics.push(problem);
    }
  }

  /// What `prefix` stands for, or `None` after reporting, once per prefix, that it's undeclared.
  fn resolve_prefix(&mut self, scope: &[Binding], prefix: Option<&str>, span: Span) -> Option<XamlNamespace> {
    let bound = scope
      .iter()
      .chain(self.stack.iter().rev().flat_map(|open| open.scope.iter()))
      .find(|(bound, _)| bound.as_deref() == prefix)
      .map(|(_, namespace)| namespace.clone());
    if bound.is_none() && self.undeclared.insert(prefix.map(str::to_owned)) {
      let diagnostic = match prefix {
        Some(prefix) => Diagnostic::error(span, format!("prefix `{prefix}` is not declared")).with_help(format!(
          "declare it here or on an ancestor, like `xmlns:{prefix}=\"using:my_app::views\"`"
        )),
        None => Diagnostic::error(span, "no default namespace is declared")
          .with_help(format!("add `xmlns=\"{ARTIC_UI}\"` to the root element")),
      };
      self.diagnostics.push(diagnostic);
    }
    bound
  }

  fn end_tag(&mut self, prefix: StrSpan, local: StrSpan, span: StrSpan) -> Result<(), Fatal> {
    let tag = qualified(prefix, local);
    let open = self
      .stack
      .last()
      .expect("the tokenizer reads end tags only inside an element");
    if open.tag != tag {
      return Err(
        self.fatal(
          Diagnostic::error(self.span_of(span), format!("expected `</{}>`, found `</{tag}>`", open.tag))
            .with_secondary(open.tag_span, "opened here"),
        ),
      );
    }
    self.close(span.end())
  }

  /// Closes the innermost open element, which ends at byte `end`, and hands it to its parent.
  fn close(&mut self, end: usize) -> Result<(), Fatal> {
    self.flush_text();
    let open = self.stack.pop().expect("an element is open when one closes");
    let span = self.span(open.start, end);
    match open.kind {
      OpenKind::Object {
        ty,
        mut members,
        namespaces,
        mut content,
      } => {
        if open.space == Space::Default {
          whitespace::normalize(&mut content);
        }
        if let (Some(first), Some(last)) = (content.first(), content.last()) {
          let content_span = self.span(first.span().start, last.span().end);
          members.push(Member {
            key: MemberKey::Content,
            value: content,
            span: content_span,
          });
        }
        let node = XamlNode::Object {
          ty,
          members,
          namespaces,
          span,
        };
        match self.stack.last_mut() {
          Some(Open {
            kind: OpenKind::Object { content, .. },
            ..
          }) => content.push(node),
          Some(Open {
            kind: OpenKind::PropertyElement { value, .. },
            ..
          }) => value.push(node),
          None => self.root = Some(node),
        }
      }
      OpenKind::PropertyElement { key, mut value } => {
        if open.space == Space::Default {
          whitespace::normalize(&mut value);
        }
        let member = Member {
          key,
          value,
          span: open.tag_span,
        };
        let Some(Open {
          kind: OpenKind::Object { members, .. },
          ..
        }) = self.stack.last_mut()
        else {
          unreachable!("a property element is always opened inside an object element");
        };
        match duplicate_of(members, &member) {
          Some(duplicate) => self.diagnostics.push(duplicate),
          None => members.push(member),
        }
      }
    }
    Ok(())
  }

  fn text(&mut self, text: StrSpan) {
    match decode(text.as_str(), text.start(), Kind::Text) {
      Ok(decoded) => self.add_text(decoded.text, text.start(), text.end()),
      Err(error) => {
        let span = self.span(error.start, error.end);
        self.diagnostics.push(Diagnostic::error(span, error.message));
      }
    }
  }

  /// Adds decoded text, written at bytes `start..end`, to the innermost element's pending text.
  fn add_text(&mut self, text: String, start: usize, end: usize) {
    let open = self
      .stack
      .last_mut()
      .expect("the tokenizer reads text only inside an element");
    match &mut open.pending_text {
      Some((pending, _, pending_end)) => {
        pending.push_str(&text);
        *pending_end = end;
      }
      None => open.pending_text = Some((text, start, end)),
    }
  }

  /// Moves the innermost element's pending text into its content or value.
  fn flush_text(&mut self) {
    let file = self.file;
    let Some(open) = self.stack.last_mut() else {
      return;
    };
    let Some((text, start, end)) = open.pending_text.take() else {
      return;
    };
    let node = XamlNode::value(text, Span::new(file, start, end));
    match &mut open.kind {
      OpenKind::Object { content, .. } => content.push(node),
      OpenKind::PropertyElement { value, .. } => value.push(node),
    }
  }

  fn finish(mut self) -> Result<XamlDocument, Diagnostics> {
    let end = self.source.text().len();
    if let Some(open) = self.stack.last() {
      let diagnostic = Diagnostic::error(self.span(end, end), format!("`<{}>` is never closed", open.tag))
        .with_secondary(open.tag_span, "opened here");
      self.diagnostics.push(diagnostic);
      return Err(self.diagnostics);
    }
    let Some(root) = self.root else {
      self
        .diagnostics
        .push(Diagnostic::error(self.span(0, end), "expected one root element"));
      return Err(self.diagnostics);
    };
    if !self.diagnostics.is_empty() {
      return Err(self.diagnostics);
    }
    Ok(XamlDocument { source: self.file, root })
  }

  fn fatal(&mut self, diagnostic: Diagnostic) -> Fatal {
    self.diagnostics.push(diagnostic);
    Fatal
  }

  fn span(&self, start: usize, end: usize) -> Span {
    Span::new(self.file, start, end)
  }

  fn span_of(&self, text: StrSpan) -> Span {
    self.span(text.start(), text.end())
  }
}

/// A tag name as written: `prefix:local` or `local`.
fn qualified(prefix: StrSpan, local: StrSpan) -> String {
  if prefix.as_str().is_empty() {
    local.as_str().to_owned()
  } else {
    format!("{}:{}", prefix.as_str(), local.as_str())
  }
}

/// Where an attribute's name starts: at its prefix, if it has one.
fn name_start(attribute: &Attribute) -> usize {
  if attribute.prefix.as_str().is_empty() {
    attribute.local.start()
  } else {
    attribute.prefix.start()
  }
}

/// Whether two type references name the same type, whatever prefixes they're written with.
fn same_type(a: &TypeRef, b: &TypeRef) -> bool {
  a.namespace == b.namespace && a.name == b.name
}

/// The length of `prefix:` in front of a name, or 0 without a prefix.
fn prefix_len(prefix: &str) -> usize {
  if prefix.is_empty() { 0 } else { prefix.len() + 1 }
}

/// The directive `name` names in the markup namespace, if artic supports it.
fn directive(name: &str) -> Option<Directive> {
  match name {
    "Name" => Some(Directive::Name),
    "Key" => Some(Directive::Key),
    "Class" => Some(Directive::Class),
    "DataType" => Some(Directive::DataType),
    "Uid" => Some(Directive::Uid),
    "FieldModifier" => Some(Directive::FieldModifier),
    _ => None,
  }
}
