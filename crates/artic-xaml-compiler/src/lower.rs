//! Turning a document's infoset into a [`View`]: every type resolved, every literal written, and
//! the elements ordered children first.

use crate::names::field_name;
use crate::resolve::{parse_path, resolve};
use crate::unsupported;
use artic_diagnostics::{Diagnostic, Diagnostics, Span};
use artic_registry::{ContentInfo, ContentMethod, PropertyInfo, PropertyRef, Registry, RustExpr, TypeInfo, Whitespace};
use artic_xaml::infoset::{Directive, Member, MemberKey, TypeRef, XamlDocument, XamlNamespace, XamlNode};
use artic_xaml::namespaces::NamespaceMap;
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};
use syn::{Expr, Ident, Path};

/// A view ready to be written.
pub(crate) struct View {
  /// The path of the facade's prelude, from the map's path for `artic`.
  pub prelude: Path,
  /// Every element, children before their parent, so element `i` is the local `__e{i}` and the
  /// root is the last.
  pub elements: Vec<Element>,
  /// The `x:Name` fields, in the order their elements are built, each with its element's index.
  pub names: Vec<(Ident, usize)>,
  /// The methods traits the setters call through, keyed by their text so they're written sorted.
  pub traits: BTreeMap<String, Path>,
  /// Every crate the map names, by its path, so the app links them.
  pub links: Vec<Path>,
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
    view: View {
      prelude: syn::parse_quote!(::artic::prelude),
      elements: Vec::new(),
      names: Vec::new(),
      traits: BTreeMap::new(),
      links: Vec::new(),
    },
    diagnostics: Diagnostics::new(),
    named: HashMap::new(),
    undeclared: HashSet::new(),
  };
  lower.element(&document.root);
  let prelude = format!("{}::prelude", namespaces.path_of("artic"));
  if let Some(prelude) = parse_path(&prelude, document.root.span(), &mut lower.diagnostics) {
    lower.view.prelude = prelude;
  }
  lower.view.links = namespaces
    .crates_named()
    .filter_map(|krate| parse_path(&namespaces.path_of(krate), document.root.span(), &mut lower.diagnostics))
    .collect();
  let mut diagnostics: Vec<Diagnostic> = lower.diagnostics.into_iter().collect();
  diagnostics.sort_by_key(|diagnostic| diagnostic.span.start);
  let mut sorted = Diagnostics::new();
  sorted.extend(diagnostics);
  sorted.into_result(lower.view)
}

/// The walk's state.
struct Lower<'a> {
  registry: &'a Registry,
  namespaces: &'a NamespaceMap,
  /// An empty expression that knows the map's paths, cloned for each literal.
  paths: RustExpr,
  view: View,
  diagnostics: Diagnostics,
  /// Where each `x:Name` field was first given.
  named: HashMap<String, Span>,
  /// The undeclared URIs already reported.
  undeclared: HashSet<String>,
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
    let resolved = if self.declared(ty) {
      match resolve(ty, self.registry, self.namespaces, &mut self.diagnostics) {
        Some(resolved) if resolved.info.is_value_provider() => {
          self.diagnostics.push(unsupported::provider_as_element(ty));
          None
        }
        resolved => resolved,
      }
    } else {
      None
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

  /// Whether `ty`'s namespace is one the map declares, reporting an undeclared URI the first
  /// time it's used.
  fn declared(&mut self, ty: &TypeRef) -> bool {
    let XamlNamespace::Uri(uri) = &ty.namespace else {
      return true;
    };
    if self.namespaces.crates(uri).is_some() {
      return true;
    }
    if self.undeclared.insert(uri.clone()) {
      self.diagnostics.push(
        Diagnostic::error(ty.span, format!("no namespace `{uri}`"))
          .with_label("undeclared namespace")
          .with_help("declare it with `NamespaceMap::alias`, or check its spelling"),
      );
    }
    false
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
      && let Some(path) = parse_path(&text, span, &mut self.diagnostics)
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
    let expr = syn::parse_str(out.as_str()).ok();
    if expr.is_none() {
      self.diagnostics.push(
        Diagnostic::error(
          span,
          format!(
            "the value of `{}` was written as invalid Rust: `{}`",
            property.name,
            out.as_str()
          ),
        )
        .with_help(format!("`{}`'s `ToRust` impl writes invalid Rust", property.value_type)),
      );
    }
    expr
  }

  /// Applies a directive: `x:Name` gives a field, `x:Key` is reported, and the others are kept
  /// for the issues that use them (#52, #94, #168).
  fn directive(&mut self, directive: &Directive, member: &Member, members: &mut Members) {
    match directive {
      Directive::Name => members.name = self.name(member),
      Directive::Key => self.diagnostics.push(unsupported::resource_key(member.span)),
      Directive::Class | Directive::DataType | Directive::FieldModifier | Directive::Uid => {}
    }
  }

  /// The `View` field an `x:Name` gives, or `None` after reporting it.
  fn name(&mut self, member: &Member) -> Option<Ident> {
    let [XamlNode::Value { text, span, .. }] = member.value.as_slice() else {
      self
        .diagnostics
        .push(Diagnostic::error(member.span, "`x:Name` takes a name written as text"));
      return None;
    };
    let field = match field_name(text) {
      Ok(field) => field,
      Err(reason) => {
        self
          .diagnostics
          .push(Diagnostic::error(*span, format!("`{text}` isn't a valid name")).with_help(reason));
        return None;
      }
    };
    match self.named.entry(field.to_string()) {
      Entry::Occupied(first) => {
        self.diagnostics.push(
          Diagnostic::error(*span, format!("`{text}` names two elements"))
            .with_label("named again here")
            .with_secondary(*first.get(), "first named here"),
        );
        None
      }
      Entry::Vacant(slot) => {
        slot.insert(*span);
        Some(field)
      }
    }
  }

  /// Lowers the content written directly inside an element of type `info`, or, for a type that
  /// didn't resolve, only the elements in it, for their own diagnostics.
  fn content(&mut self, info: Option<&'static TypeInfo>, member: &Member, members: &mut Members) {
    let Some(info) = info else {
      for child in member.value.iter().filter(|node| matches!(node, XamlNode::Object { .. })) {
        self.element(child);
      }
      return;
    };
    match info.content {
      None => self.no_content(info, member),
      Some(ContentInfo::Property(property)) => self.content_text(info, property, member, members),
      Some(ContentInfo::Collection { method: None, .. }) => {
        if member.value.iter().any(|node| !is_blank(node)) {
          self.diagnostics.push(unsupported::value_collection(member.span));
        }
      }
      Some(ContentInfo::Collection {
        method: Some(method), ..
      }) => members.content = self.children(info, method, member),
    }
  }

  /// Reports content inside a type that takes none.
  fn no_content(&mut self, info: &TypeInfo, member: &Member) {
    for node in &member.value {
      match node {
        XamlNode::Value { text, span, .. } if !text.is_empty() => {
          self
            .diagnostics
            .push(Diagnostic::error(*span, format!("`{}` takes no text content", info.name)));
        }
        XamlNode::Object { span, .. } => {
          self
            .diagnostics
            .push(Diagnostic::error(*span, format!("`{}` takes no child elements", info.name)));
          self.element(node);
        }
        XamlNode::Value { .. } | XamlNode::MarkupExtension { .. } => {}
      }
    }
  }

  /// Sets the content property from the text inside the element: `raw` for a type whose
  /// whitespace is significant, `text` otherwise, and nothing when that is empty.
  fn content_text(&mut self, info: &TypeInfo, property: PropertyRef, member: &Member, members: &mut Members) {
    let significant = info.whitespace == Whitespace::Significant;
    let mut value = String::new();
    for node in &member.value {
      match node {
        XamlNode::Value { text, raw, .. } => value.push_str(if significant { raw } else { text }),
        XamlNode::Object { span, .. } => {
          self.diagnostics.push(unsupported::value_object(*span));
          return;
        }
        XamlNode::MarkupExtension { ty, span, .. } => {
          self.diagnostics.push(unsupported::markup_extension(ty, *span));
          return;
        }
      }
    }
    if !value.is_empty() {
      self.set(info, property, &value, member.span, members);
    }
  }

  /// Lowers the child elements of a collection content and returns its content call: `child`
  /// takes one, `children` any number, and none is written for no children.
  fn children(&mut self, info: &TypeInfo, method: ContentMethod, member: &Member) -> Content {
    let mut objects = Vec::new();
    let mut children = Vec::new();
    for node in &member.value {
      match node {
        XamlNode::Value { text, span, .. } if !text.is_empty() => {
          self.diagnostics.push(
            Diagnostic::error(*span, format!("`{}` takes no text content", info.name))
              .with_help("put the text in an element, such as a `TextBlock`"),
          );
        }
        XamlNode::Object { span, .. } => {
          objects.push(*span);
          children.extend(self.element(node));
        }
        XamlNode::Value { .. } | XamlNode::MarkupExtension { .. } => {}
      }
    }
    let single = method == ContentMethod::Child;
    if single && let [first, second, ..] = objects.as_slice() {
      self.diagnostics.push(
        Diagnostic::error(*second, format!("`{}` takes one child", info.name))
          .with_label("second child")
          .with_secondary(*first, "first child"),
      );
    }
    let method = syn::parse_str(method.name()).expect("a content method is an identifier");
    match children.as_slice() {
      [] => Content::None,
      [child] if single => Content::Child { method, child: *child },
      _ if single => Content::None,
      _ => Content::Children { method, children },
    }
  }
}

/// Whether `node` is text that adds nothing, such as indentation between elements.
fn is_blank(node: &XamlNode) -> bool {
  matches!(node, XamlNode::Value { text, .. } if text.is_empty())
}
