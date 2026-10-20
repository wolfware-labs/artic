//! Turning a document's infoset into a [`View`]: every type resolved, every literal written, and
//! the elements ordered children first.

use crate::resolve::{parse_path, resolve};
use crate::unsupported;
use artic_registry::{PropertyInfo, PropertyRef, Registry, RustExpr, TypeInfo};
use artic_xaml::diagnostics::{Diagnostic, Diagnostics, Span};
use artic_xaml::infoset::{Directive, Member, MemberKey, XamlDocument, XamlNode};
use artic_xaml::namespaces::NamespaceMap;
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use syn::{Expr, Ident, Path};

/// A view ready to be written.
#[derive(Default)]
pub(crate) struct View {
  /// Every element, children before their parent, so element `i` is the local `__e{i}` and the
  /// root is the last.
  pub elements: Vec<Element>,
  /// The `x:Name` fields, in the order their elements are built, each with its element's index.
  pub names: Vec<(Ident, usize)>,
  /// The methods traits the setters call through, keyed by their text so they're written sorted.
  pub traits: BTreeMap<String, Path>,
}

/// One element: its type, the builder methods that set its properties, and its children.
pub(crate) struct Element {
  /// The type's path in generated code.
  pub ty: Path,
  /// The property methods, in the order the document sets them.
  pub setters: Vec<Setter>,
  /// The children its content method takes.
  pub content: Content,
}

/// A builder method call: `.method(value)`.
pub(crate) struct Setter {
  /// The method, named after the property's field.
  pub method: Ident,
  /// The expression that builds the value.
  pub value: Expr,
}

/// The children an element's content method takes.
#[derive(Default)]
pub(crate) enum Content {
  /// No content call.
  #[default]
  None,
  /// `.child(…)` with the child's index.
  Child {
    /// The method, `child`.
    method: Ident,
    /// The child's index.
    child: usize,
  },
  /// `.children([…])` with the children's indices, never empty.
  Children {
    /// The method, `children`.
    method: Ident,
    /// The children's indices, in document order.
    children: Vec<usize>,
  },
}

/// Lowers `document`, collecting every problem.
pub(crate) fn lower(document: &XamlDocument, registry: &Registry, namespaces: &NamespaceMap) -> Result<View, Diagnostics> {
  let mut paths = RustExpr::new();
  for krate in namespaces.crates_named() {
    paths.path(krate, &namespaces.path_of(krate));
  }
  let mut lower = Lower {
    registry,
    namespaces,
    paths,
    view: View::default(),
    diagnostics: Diagnostics::new(),
  };
  lower.element(&document.root);
  lower.diagnostics.into_result(lower.view)
}

/// The walk's state.
struct Lower<'a> {
  registry: &'a Registry,
  namespaces: &'a NamespaceMap,
  /// An empty expression that knows the map's paths, cloned for each literal.
  paths: RustExpr,
  view: View,
  diagnostics: Diagnostics,
}

/// What lowering one element's members gathers.
#[derive(Default)]
struct Members {
  setters: Vec<Setter>,
  /// Where each property was set, by field, to report one set twice.
  set: HashMap<&'static str, Span>,
  content: Content,
  name: Option<Ident>,
}

impl Lower<'_> {
  /// Lowers an object element and its children, returning its index, or `None` when its type
  /// isn't one the view can build.
  fn element(&mut self, node: &XamlNode) -> Option<usize> {
    let XamlNode::Object { ty, members, .. } = node else {
      unreachable!("only object elements are lowered as elements")
    };
    let resolved = match self.report(resolve(ty, self.registry, self.namespaces)) {
      Some(resolved) if resolved.info.is_value_provider() => {
        self.diagnostics.push(unsupported::provider_as_element(ty));
        None
      }
      resolved => resolved,
    };
    let info = resolved.as_ref().map(|resolved| resolved.info);
    let mut gathered = Members::default();
    for member in members {
      match &member.key {
        MemberKey::Property(name) => {
          if let Some(info) = info {
            self.property(info, name, member, &mut gathered);
          }
        }
        MemberKey::Attached { .. } => self.diagnostics.push(unsupported::attached(member.span)),
        MemberKey::Directive(directive) => self.directive(directive, member, &mut gathered),
        MemberKey::Content | MemberKey::Items => self.content(info, member, &mut gathered),
      }
    }
    let resolved = resolved?;
    self.view.elements.push(Element {
      ty: resolved.path,
      setters: gathered.setters,
      content: gathered.content,
    });
    let index = self.view.elements.len() - 1;
    if let Some(name) = gathered.name {
      self.view.names.push((name, index));
    }
    Some(index)
  }

  /// Sets the property `name` of `info` from an attribute or a property element.
  fn property(&mut self, info: &'static TypeInfo, name: &str, member: &Member, members: &mut Members) {
    let Some(property) = info.property(name) else {
      let diagnostic = if info.events.iter().any(|event| event.name == name) {
        unsupported::event(member.span)
      } else {
        Diagnostic::error(member.span, format!("`{}` has no property `{name}`", info.name)).with_label("unknown property")
      };
      self.diagnostics.push(diagnostic);
      return;
    };
    if let Some((text, span)) = self.member_text(member) {
      self.set(info, property, &text, span, members);
    }
  }

  /// The text a property member holds, or `None` after reporting a value that isn't text.
  fn member_text(&mut self, member: &Member) -> Option<(String, Span)> {
    let mut text = String::new();
    for node in &member.value {
      match node {
        XamlNode::Value { text: value, .. } => text.push_str(value),
        XamlNode::MarkupExtension { ty, span, .. } => {
          self.diagnostics.push(unsupported::markup_extension(ty, *span));
          return None;
        }
        XamlNode::Object { span, .. } => {
          self.diagnostics.push(unsupported::value_object(*span));
          return None;
        }
      }
    }
    let span = match member.value.as_slice() {
      [only] => only.span(),
      _ => member.span,
    };
    Some((text, span))
  }

  /// Writes `text` as `property`'s value and adds its setter, reporting a property set twice.
  fn set(&mut self, info: &TypeInfo, property: PropertyRef, text: &str, span: Span, members: &mut Members) {
    match members.set.entry(property.info.field) {
      Entry::Occupied(first) => {
        self.diagnostics.push(
          Diagnostic::error(span, format!("`{}` is set twice", property.info.name))
            .with_label("set again here")
            .with_secondary(*first.get(), "first set here"),
        );
        return;
      }
      Entry::Vacant(slot) => {
        slot.insert(span);
      }
    }
    if let Some(group) = property.group {
      self.import_group(info, group, span);
    }
    if let Some(value) = self.literal(property.info, text, span) {
      let method = syn::parse_str(property.info.field).expect("a registry field is an identifier");
      members.setters.push(Setter { method, value });
    }
  }

  /// Records the methods trait of `info`'s group `field`, which its properties' setters call.
  fn import_group(&mut self, info: &TypeInfo, field: &str, span: Span) {
    let group = info
      .groups
      .iter()
      .find(|group| group.field == field)
      .expect("a property's group is one of its type's");
    let krate = group.methods.split("::").next().unwrap_or(group.methods);
    let name = group.methods.rsplit("::").next().unwrap_or(group.methods);
    let text = format!("{}::{name}", self.namespaces.path_of(krate));
    if !self.view.traits.contains_key(&text)
      && let Some(path) = self.report(parse_path(&text, span))
    {
      self.view.traits.insert(text, path);
    }
  }

  /// Parses `text` for `property` and returns the expression that builds the value.
  fn literal(&mut self, property: &PropertyInfo, text: &str, span: Span) -> Option<Expr> {
    let Some(write) = property.literal else {
      self.diagnostics.push(
        Diagnostic::error(span, format!("`{}` can't be written as Rust", property.name))
          .with_help("build the registry with `artic-registry`'s `literals` feature"),
      );
      return None;
    };
    let mut out = self.paths.clone();
    if let Err(message) = write(text, &mut out) {
      self
        .diagnostics
        .push(Diagnostic::error(span, message).with_label(format!("for `{}`", property.name)));
      return None;
    }
    let expr = syn::parse_str(out.as_str()).map_err(|_| {
      Diagnostic::error(
        span,
        format!("the value of `{}` was written as invalid Rust: `{}`", property.name, out.as_str()),
      )
      .with_help(format!("`{}`'s `ToRust` impl writes invalid Rust", property.value_type))
    });
    self.report(expr)
  }

  /// Applies a directive: `x:Name` gives a field, `x:Key` is reported, and the others are kept
  /// for the issues that use them (#52, #94, #168).
  fn directive(&mut self, directive: &Directive, member: &Member, _members: &mut Members) {
    match directive {
      Directive::Key => self.diagnostics.push(unsupported::resource_key(member.span)),
      Directive::Name | Directive::Class | Directive::DataType | Directive::FieldModifier | Directive::Uid => {}
    }
  }

  /// Lowers the elements inside content; the content rules come with the next commit.
  fn content(&mut self, _info: Option<&'static TypeInfo>, member: &Member, _members: &mut Members) {
    for child in member.value.iter().filter(|node| matches!(node, XamlNode::Object { .. })) {
      self.element(child);
    }
  }

  /// The value, or `None` after recording the diagnostic.
  fn report<T>(&mut self, result: Result<T, Diagnostic>) -> Option<T> {
    result.map_err(|diagnostic| self.diagnostics.push(diagnostic)).ok()
  }
}
