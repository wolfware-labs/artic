//! Checking a document's members against its types: [`check`] and the [`CheckedView`] it
//! returns.

use crate::document::{Content, Element, Member, Problem, Text, TypeUse};
use crate::{Document, field_name, unsupported};
use artic_diagnostics::{Diagnostic, Diagnostics, Span};
use artic_registry::{ContentInfo, ContentMethod, PropertyRef, TypeInfo, Whitespace};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

/// A view whose members are all found, ready for a back end.
#[derive(Debug, Clone)]
pub struct CheckedView {
  /// Every element whose type was found, children before their parent.
  pub elements: Vec<CheckedElement>,
  /// The root's index, or `None` when its type wasn't found.
  pub root: Option<usize>,
  /// Where the root element is written, for problems with the whole file.
  pub span: Span,
  /// Each valid name, its field and its element's index, in the order the elements are built.
  pub names: Vec<CheckedName>,
}

/// A valid name: what the view writes and the field a back end gives the element.
#[derive(Debug, Clone)]
pub struct CheckedName {
  /// The name as the view writes it.
  pub name: Text,
  /// The field it gives, unique in the view.
  pub field: String,
  /// The named element's index.
  pub element: usize,
}

/// An element ready to build: its type, its properties and its children.
#[derive(Debug, Clone)]
pub struct CheckedElement {
  /// The element's type.
  pub ty: TypeUse,
  /// Each property with the text it's set to, in member order.
  pub properties: Vec<(PropertyRef, Text)>,
  /// The children its content method takes.
  pub content: CheckedContent,
}

/// The children an element's content method takes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CheckedContent {
  /// No content call.
  #[default]
  None,
  /// `child`, with the child's index.
  Child(usize),
  /// `children`, with the children's indices in document order; never empty.
  Children(Vec<usize>),
}

/// Checks `document`'s members: each property against its type's descriptor, the content against
/// the type's content rule, and the names. Returns the view and every problem the walk reaches,
/// in the order it reaches them.
#[must_use]
pub fn check(document: &Document) -> (CheckedView, Diagnostics) {
  let mut check = Check {
    elements: Vec::new(),
    names: Vec::new(),
    diagnostics: Diagnostics::new(),
    named: HashMap::new(),
    reported: HashSet::new(),
  };
  let root = check.element(&document.root);
  let view = CheckedView {
    elements: check.elements,
    root,
    span: document.root.span,
    names: check.names,
  };
  (view, check.diagnostics)
}

/// The walk's state.
struct Check {
  elements: Vec<CheckedElement>,
  names: Vec<CheckedName>,
  diagnostics: Diagnostics,
  /// The name that first gave each field, where it's written.
  named: HashMap<String, Text>,
  /// The `once` keys already reported.
  reported: HashSet<String>,
}

/// What checking one element's members gathers.
#[derive(Default)]
struct Members {
  properties: Vec<(PropertyRef, Text)>,
  /// Where each property was set, by field, to report one set twice.
  set: HashMap<&'static str, Span>,
  content: CheckedContent,
  name: Option<(Text, String)>,
}

impl Check {
  /// Checks an element and the elements in its content, returning its index, or `None` when its
  /// type wasn't found.
  fn element(&mut self, element: &Element) -> Option<usize> {
    let ty = match &element.ty {
      Ok(ty) => Some(ty),
      Err(problem) => {
        self.report(problem);
        None
      }
    };
    let info = ty.map(|ty| ty.info);
    let mut gathered = Members::default();
    for member in &element.members {
      match member {
        Member::Property { name, span, value } => {
          if let Some(info) = info {
            self.property(info, name, *span, value, &mut gathered);
          }
        }
        Member::Name(name) => gathered.name = self.name(name),
        Member::Content { span, nodes } => self.content(info, *span, nodes, &mut gathered),
        Member::Unsupported(problem) => self.report(problem),
      }
    }
    let ty: &TypeUse = ty?;
    self.elements.push(CheckedElement {
      ty: ty.clone(),
      properties: gathered.properties,
      content: gathered.content,
    });
    let index = self.elements.len() - 1;
    if let Some((name, field)) = gathered.name {
      self.names.push(CheckedName {
        name,
        field,
        element: index,
      });
    }
    Some(index)
  }

  /// Reports `problem`, unless its `once` key was reported before.
  fn report(&mut self, problem: &Problem) {
    if let Some(key) = &problem.once
      && !self.reported.insert(key.clone())
    {
      return;
    }
    self.diagnostics.push((*problem.diagnostic).clone());
  }

  /// Sets the property `name` of `info` named at `span`.
  fn property(
    &mut self,
    info: &'static TypeInfo,
    name: &str,
    span: Span,
    value: &Result<Text, Problem>,
    members: &mut Members,
  ) {
    let Some(property) = info.property(name) else {
      let diagnostic = if info.events.iter().any(|event| event.name == name) {
        unsupported::event(span)
      } else {
        Diagnostic::error(span, format!("`{}` has no property `{name}`", info.name)).with_label("unknown property")
      };
      self.diagnostics.push(diagnostic);
      return;
    };
    match value {
      Ok(text) => self.set(property, text.clone(), members),
      Err(problem) => self.report(problem),
    }
  }

  /// Sets `property` to `text`, reporting a property set twice.
  fn set(&mut self, property: PropertyRef, text: Text, members: &mut Members) {
    match members.set.entry(property.info.field) {
      Entry::Occupied(first) => {
        self.diagnostics.push(
          Diagnostic::error(text.span, format!("`{}` is set twice", property.info.name))
            .with_label("set again here")
            .with_secondary(*first.get(), "first set here"),
        );
      }
      Entry::Vacant(slot) => {
        slot.insert(text.span);
        members.properties.push((property, text));
      }
    }
  }

  /// The name an element is given, or `None` after reporting it.
  fn name(&mut self, name: &Result<Text, Problem>) -> Option<(Text, String)> {
    let name = match name {
      Ok(name) => name,
      Err(problem) => {
        self.report(problem);
        return None;
      }
    };
    let field = match field_name(&name.text) {
      Ok(field) => field,
      Err(reason) => {
        self
          .diagnostics
          .push(Diagnostic::error(name.span, format!("`{}` isn't a valid name", name.text)).with_help(reason));
        return None;
      }
    };
    match self.named.entry(field) {
      Entry::Occupied(first) => {
        let mut diagnostic = Diagnostic::error(name.span, format!("`{}` names two elements", name.text))
          .with_label("named again here")
          .with_secondary(first.get().span, "first named here");
        if first.get().text != name.text {
          diagnostic = diagnostic.with_help(format!(
            "`{}` and `{}` both give the field `{}`",
            first.get().text,
            name.text,
            first.key()
          ));
        }
        self.diagnostics.push(diagnostic);
        None
      }
      Entry::Vacant(slot) => {
        let field = slot.key().clone();
        slot.insert(name.clone());
        Some((name.clone(), field))
      }
    }
  }

  /// Checks the content of an element of type `info`, or, for a type that wasn't found, only the
  /// elements in it, for their own problems.
  fn content(&mut self, info: Option<&'static TypeInfo>, span: Span, nodes: &[Content], members: &mut Members) {
    let Some(info) = info else {
      for node in nodes {
        if let Content::Element(element) = node {
          self.element(element);
        }
      }
      return;
    };
    match info.content {
      None => self.no_content(info, nodes),
      Some(ContentInfo::Property(property)) => self.content_text(info, property, span, nodes, members),
      Some(ContentInfo::Collection { method: None, .. }) => {
        if nodes.iter().any(|node| !is_blank(node)) {
          self.diagnostics.push(unsupported::value_collection(span));
        }
      }
      Some(ContentInfo::Collection {
        method: Some(method), ..
      }) => members.content = self.children(info, method, nodes),
    }
  }

  /// Reports content inside a type that takes none, still checking the elements.
  fn no_content(&mut self, info: &TypeInfo, nodes: &[Content]) {
    for node in nodes {
      match node {
        Content::Text { text, span, .. } if !text.is_empty() => {
          self
            .diagnostics
            .push(Diagnostic::error(*span, format!("`{}` takes no text content", info.name)));
        }
        Content::Element(element) => {
          self.diagnostics.push(Diagnostic::error(
            element.span,
            format!("`{}` takes no child elements", info.name),
          ));
          self.element(element);
        }
        Content::Text { .. } | Content::Unsupported(_) => {}
      }
    }
  }

  /// Sets the content property from the text inside the element: `raw` for a type whose
  /// whitespace is significant, `text` otherwise, and nothing when that is empty. Text set this
  /// way counts as setting the property, so an attribute that sets it too is "set twice".
  fn content_text(&mut self, info: &TypeInfo, property: PropertyRef, span: Span, nodes: &[Content], members: &mut Members) {
    let significant = info.whitespace == Whitespace::Significant;
    let mut value = String::new();
    for node in nodes {
      match node {
        Content::Text { text, raw, .. } => value.push_str(if significant { raw } else { text }),
        Content::Element(element) => {
          self.diagnostics.push(unsupported::value_object(element.span));
          return;
        }
        Content::Unsupported(problem) => {
          self.report(problem);
          return;
        }
      }
    }
    if !value.is_empty() {
      self.set(property, Text { text: value, span }, members);
    }
  }

  /// Checks the child elements of a collection content and returns the children its method
  /// takes: `child` one, `children` any number, and none for no children.
  fn children(&mut self, info: &TypeInfo, method: ContentMethod, nodes: &[Content]) -> CheckedContent {
    let mut written = Vec::new();
    let mut children = Vec::new();
    for node in nodes {
      match node {
        Content::Text { text, span, .. } if !text.is_empty() => {
          self.diagnostics.push(
            Diagnostic::error(*span, format!("`{}` takes no text content", info.name))
              .with_help("put the text in an element, such as a `TextBlock`"),
          );
        }
        Content::Element(element) => {
          written.push(element.span);
          children.extend(self.element(element));
        }
        Content::Text { .. } | Content::Unsupported(_) => {}
      }
    }
    let single = method == ContentMethod::Child;
    if single && let [first, second, ..] = written.as_slice() {
      self.diagnostics.push(
        Diagnostic::error(*second, format!("`{}` takes one child", info.name))
          .with_label("second child")
          .with_secondary(*first, "first child"),
      );
    }
    match children.as_slice() {
      [] => CheckedContent::None,
      [child] if single => CheckedContent::Child(*child),
      _ if single => CheckedContent::None,
      _ => CheckedContent::Children(children),
    }
  }
}

/// Whether `node` is text that adds nothing, such as indentation between elements.
fn is_blank(node: &Content) -> bool {
  matches!(node, Content::Text { text, .. } if text.is_empty())
}
