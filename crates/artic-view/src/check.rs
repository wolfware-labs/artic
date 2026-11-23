//! Checking a document's members against its types: [`check`] and the [`CheckedView`] it
//! returns.

use crate::document::{Content, Element, Member, Problem, Text, TypeUse};
use crate::{Document, field_name, unsupported};
use artic_diagnostics::{Diagnostic, Diagnostics, Span};
use artic_registry::{
  AttachedPropertyInfo, AttachedTarget, ContentInfo, ContentMethod, PropertyRef, TypeInfo, Whitespace, type_names,
};
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
  /// The Rust type the view's code is, which takes the root's place, as the view writes it.
  pub code_type: Option<Text>,
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
  /// The visibility the view writes for the field, or `None` for the back end's default.
  pub visibility: Option<FieldVisibility>,
}

/// The visibility a view writes for the field a name gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldVisibility {
  /// `pub(crate)`: visible in the crate.
  Crate,
  /// `pub`: visible wherever the code type is.
  Public,
}

/// An element ready to build: its type, its properties and its children.
#[derive(Debug, Clone)]
pub struct CheckedElement {
  /// The element's type.
  pub ty: TypeUse,
  /// Each property with the text it's set to, in member order.
  pub properties: Vec<(PropertyRef, Text)>,
  /// Each attached property set on it, in member order.
  pub attached: Vec<CheckedAttached>,
  /// The children its content method takes.
  pub content: CheckedContent,
}

/// An attached property set on an element: its owner, the property and the text it's set to.
#[derive(Debug, Clone)]
pub struct CheckedAttached {
  /// The type that owns the property, as the document names it.
  pub owner: TypeUse,
  /// The property.
  pub info: &'static AttachedPropertyInfo,
  /// The text it's set to.
  pub value: Text,
}

/// Where an element is written, which an attached property's target rule checks.
#[derive(Debug, Clone, Copy)]
enum Parent {
  /// It's the view's root.
  Root,
  /// It's inside an element of this type.
  Of(&'static TypeInfo),
  /// It's inside an element whose type wasn't found.
  Unknown,
}

/// The children an element's content method takes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CheckedContent {
  /// No content call.
  #[default]
  None,
  /// One child, for a `Child` field, with the child's index.
  Child(usize),
  /// Any number of children, with their indices in document order; never empty.
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
    code_type: None,
  };
  let root = check.element(&document.root, Parent::Root);
  let view = CheckedView {
    elements: check.elements,
    root,
    span: document.root.span,
    names: check.names,
    code_type: check.code_type,
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
  /// The code type the root names.
  code_type: Option<Text>,
}

/// What checking one element's members gathers.
#[derive(Default)]
struct Members {
  properties: Vec<(PropertyRef, Text)>,
  /// Where each property was set, by field, to report one set twice.
  set: HashMap<&'static str, Span>,
  attached: Vec<CheckedAttached>,
  content: CheckedContent,
  name: Option<(Text, String)>,
  /// Whether the element is named, validly or not.
  named: bool,
  /// Where the field visibility is written, and what it is if valid.
  visibility: Option<(Span, Option<FieldVisibility>)>,
}

impl Check {
  /// Checks an element written in `parent` and the elements in its content, returning its index,
  /// or `None` when its type wasn't found. Only the root may name the view's code type.
  fn element(&mut self, element: &Element, parent: Parent) -> Option<usize> {
    let root = matches!(parent, Parent::Root);
    let ty = match &element.ty {
      Ok(ty) => Some(ty),
      Err(problem) => {
        self.report(problem);
        None
      }
    };
    let info = ty.map(|ty| ty.info);
    if root {
      for member in &element.members {
        if let Member::CodeType(code_type) = member {
          self.code_type(code_type, true);
        }
      }
    }
    let mut gathered = Members::default();
    for member in &element.members {
      match member {
        Member::Property { name, span, value } => {
          if let Some(info) = info {
            self.property(info, name, *span, value, &mut gathered);
          }
        }
        Member::Attached {
          owner,
          name,
          span,
          value,
        } => self.attached(owner, name, *span, value, parent, &mut gathered),
        Member::Name(name) => {
          gathered.named = true;
          if root && self.code_type.is_some() {
            self.root_name(name);
          } else {
            gathered.name = self.name(name);
          }
        }
        Member::CodeType(code_type) => {
          if !root {
            self.code_type(code_type, false);
          }
        }
        Member::FieldVisibility(visibility) => gathered.visibility = self.visibility(visibility),
        Member::Content { span, nodes } => self.content(info, *span, nodes, &mut gathered),
        Member::Unsupported(problem) => self.report(problem),
      }
    }
    if let Some((span, _)) = gathered.visibility
      && !gathered.named
    {
      self
        .diagnostics
        .push(Diagnostic::error(span, "a field visibility needs a name on its element").with_label("no name here"));
    }
    if root
      && self.code_type.is_some()
      && let Some(ty) = ty
    {
      if !ty.info.code_type_root {
        self.diagnostics.push(
          Diagnostic::error(
            element.span,
            format!("`{}` can't be the root of a view with a code type", ty.info.name),
          )
          .with_help("the code type takes the root's place, so the root's type must allow it"),
        );
      } else if matches!(
        ty.info.content,
        Some(ContentInfo::Property(_) | ContentInfo::Collection { method: None, .. })
      ) {
        self.diagnostics.push(
          Diagnostic::error(
            element.span,
            format!(
              "`{}` can't be the root of a view with a code type: its content isn't a child or children",
              ty.info.name
            ),
          )
          .with_help("a code type takes its root's content as a `Child` or `Children` field"),
        );
      }
    }
    if let Some(ty) = ty
      && ty.info.new.is_none()
    {
      self.diagnostics.push(Diagnostic::error(
        element.span,
        format!("`{}` can't be created from markup yet (#59)", ty.info.name),
      ));
    }
    let ty: &TypeUse = ty?;
    self.elements.push(CheckedElement {
      ty: ty.clone(),
      properties: gathered.properties,
      attached: gathered.attached,
      content: gathered.content,
    });
    let index = self.elements.len() - 1;
    if let Some((name, field)) = gathered.name {
      self.names.push(CheckedName {
        name,
        field,
        element: index,
        visibility: gathered.visibility.and_then(|(_, visibility)| visibility),
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

  /// Sets the attached property `name` of `owner` on an element written in `parent`, reporting an
  /// owner that wasn't found, a property it doesn't have, one its target rule doesn't allow there,
  /// and one set twice.
  fn attached(
    &mut self,
    owner: &Result<TypeUse, Problem>,
    name: &str,
    span: Span,
    value: &Result<Text, Problem>,
    parent: Parent,
    members: &mut Members,
  ) {
    let owner = match owner {
      Ok(owner) => owner,
      Err(problem) => {
        self.report(problem);
        return;
      }
    };
    let Some(info) = owner.info.attached_property(name) else {
      self.diagnostics.push(
        Diagnostic::error(span, format!("`{}` has no attached property `{name}`", owner.info.name))
          .with_label("unknown attached property"),
      );
      return;
    };
    let allowed = match (info.target, parent) {
      (AttachedTarget::Any, _) | (AttachedTarget::ChildrenOfOwner, Parent::Unknown) => true,
      (AttachedTarget::ChildrenOfOwner, Parent::Of(parent)) => parent.rust_path == owner.info.rust_path,
      (AttachedTarget::ChildrenOfOwner, Parent::Root) => false,
    };
    if !allowed {
      self.diagnostics.push(
        Diagnostic::error(
          span,
          format!("`{0}.{name}` can only be set on a child of a `{0}`", owner.info.name),
        )
        .with_label("not inside one"),
      );
      return;
    }
    let value = match value {
      Ok(value) => value,
      Err(problem) => {
        self.report(problem);
        return;
      }
    };
    if let Some(first) = members
      .attached
      .iter()
      .find(|set| set.owner.info.rust_path == owner.info.rust_path && set.info.name == name)
    {
      self.diagnostics.push(
        Diagnostic::error(span, format!("`{}.{name}` is set twice", owner.info.name))
          .with_label("set again here")
          .with_secondary(first.value.span, "first set here"),
      );
      return;
    }
    members.attached.push(CheckedAttached {
      owner: owner.clone(),
      info,
      value: value.clone(),
    });
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

  /// Reports the name of the root of a view with a code type, which gives no field.
  fn root_name(&mut self, name: &Result<Text, Problem>) {
    match name {
      Ok(name) => self.diagnostics.push(
        Diagnostic::error(name.span, "the root of a view with a code type can't be named")
          .with_help("the code type is the root: its code reaches it as `self`"),
      ),
      Err(problem) => self.report(problem),
    }
  }

  /// Takes the code type `code_type` names, reporting it anywhere but the `root` or a second time.
  fn code_type(&mut self, code_type: &Result<Text, Problem>, root: bool) {
    let code_type = match code_type {
      Ok(code_type) => code_type,
      Err(problem) => {
        self.report(problem);
        return;
      }
    };
    if !root {
      self.diagnostics.push(
        Diagnostic::error(code_type.span, "only the root element can name the view's code type").with_label("not the root"),
      );
    } else if let Some(first) = &self.code_type {
      self.diagnostics.push(
        Diagnostic::error(code_type.span, "the code type is named twice")
          .with_label("named again here")
          .with_secondary(first.span, "first named here"),
      );
    } else {
      self.code_type = Some(code_type.clone());
    }
  }

  /// Where `visibility` is written and what it is, `None` inside after reporting text that isn't
  /// one; `None` after reporting the front end's problem.
  fn visibility(&mut self, visibility: &Result<Text, Problem>) -> Option<(Span, Option<FieldVisibility>)> {
    let visibility = match visibility {
      Ok(visibility) => visibility,
      Err(problem) => {
        self.report(problem);
        return None;
      }
    };
    let parsed = match visibility.text.as_str() {
      "pub" => Some(FieldVisibility::Public),
      "pub(crate)" => Some(FieldVisibility::Crate),
      text => {
        self.diagnostics.push(
          Diagnostic::error(visibility.span, format!("`{text}` isn't a field visibility"))
            .with_help("write `pub` or `pub(crate)`, or nothing for the back end's default"),
        );
        None
      }
    };
    Some((visibility.span, parsed))
  }

  /// Checks the content of an element of type `info`, or, for a type that wasn't found, only the
  /// elements in it, for their own problems.
  fn content(&mut self, info: Option<&'static TypeInfo>, span: Span, nodes: &[Content], members: &mut Members) {
    let Some(info) = info else {
      for node in nodes {
        if let Content::Element(element) = node {
          self.element(element, Parent::Unknown);
        }
      }
      return;
    };
    match info.content {
      None => self.no_content(info, nodes),
      Some(ContentInfo::Property(property)) => self.content_text(property, span, nodes, members),
      Some(ContentInfo::Collection { method: None, .. }) => {
        if nodes.iter().any(|node| !is_blank(node)) {
          self.diagnostics.push(unsupported::value_collection(span));
        }
      }
      Some(ContentInfo::Collection {
        method: Some(method),
        takes,
        text_wrapper,
        ..
      }) => members.content = self.children(info, method, takes(), text_wrapper(), nodes),
    }
  }

  /// Reports content inside a type that takes none, still checking the elements.
  fn no_content(&mut self, info: &'static TypeInfo, nodes: &[Content]) {
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
          self.element(element, Parent::Of(info));
        }
        Content::Text { .. } | Content::Unsupported(_) => {}
      }
    }
  }

  /// Sets the content property from the text inside the element, after the language's whitespace
  /// rules, and nothing when that is empty. Text set this way counts as setting the property, so
  /// an attribute that sets it too is "set twice".
  fn content_text(&mut self, property: PropertyRef, span: Span, nodes: &[Content], members: &mut Members) {
    let mut value = String::new();
    for node in nodes {
      match node {
        Content::Text { text, .. } => value.push_str(text),
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
  /// takes: `child` one, `children` any number, and none for no children. Text becomes an element
  /// of the collection's `text_wrapper` type, when it has one whose content is a property, that
  /// markup can create and that `takes` accepts; without one, text that isn't blank is reported.
  fn children(
    &mut self,
    info: &'static TypeInfo,
    method: ContentMethod,
    takes: Option<&[&TypeInfo]>,
    text_wrapper: Option<&'static TypeInfo>,
    nodes: &[Content],
  ) -> CheckedContent {
    let wrapper = text_wrapper.and_then(|wrapper| match wrapper.content {
      Some(ContentInfo::Property(property)) if wrapper.new.is_some() && accepts(takes, wrapper) => Some((wrapper, property)),
      _ => None,
    });
    let mut written = Vec::new();
    let mut children = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
      match node {
        Content::Text { text, raw, span } => match wrapper {
          Some((wrapper, property)) => {
            let Some(piece) = text_piece(info.whitespace, text, raw, between_elements(nodes, index)) else {
              continue;
            };
            written.push(*span);
            children.push(self.wrapped(
              wrapper,
              property,
              Text {
                text: piece,
                span: *span,
              },
            ));
          }
          None if !text.is_empty() => self.diagnostics.push(
            Diagnostic::error(*span, format!("`{}` takes no text content", info.name))
              .with_help("put the text in an element, such as a `TextBlock`"),
          ),
          None => {}
        },
        Content::Element(element) => {
          written.push(element.span);
          if let (Some(taken), Ok(ty)) = (takes, &element.ty)
            && !accepts(takes, ty.info)
          {
            self.diagnostics.push(Diagnostic::error(
              element.span,
              format!("`{}` takes only {}", info.name, type_names(taken)),
            ));
          }
          children.extend(self.element(element, Parent::Of(info)));
        }
        Content::Unsupported(_) => {}
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

  /// Adds an element of `wrapper` type whose content `property` is `text`, and returns its index.
  fn wrapped(&mut self, wrapper: &'static TypeInfo, property: PropertyRef, text: Text) -> usize {
    let ty = TypeUse {
      info: wrapper,
      module: None,
      span: text.span,
    };
    self.elements.push(CheckedElement {
      ty,
      properties: vec![(property, text)],
      attached: Vec::new(),
      content: CheckedContent::None,
    });
    self.elements.len() - 1
  }
}

/// A text node of a collection content as it counts: its `text` after the language's rules, or,
/// in a type whose `whitespace` is significant, one space for blank text written
/// `between_elements`, so `<Run>Hello</Run> <Run>world</Run>` keeps its space. `None` when it adds
/// nothing.
fn text_piece(whitespace: Whitespace, text: &str, raw: &str, between_elements: bool) -> Option<String> {
  if !text.is_empty() {
    return Some(text.to_owned());
  }
  (whitespace == Whitespace::Significant && between_elements && !raw.is_empty()).then(|| " ".to_owned())
}

/// Whether the node at `index` of `nodes` has an element on each side.
fn between_elements(nodes: &[Content], index: usize) -> bool {
  let is_element = |node: Option<&Content>| matches!(node, Some(Content::Element(_)));
  index.checked_sub(1).is_some_and(|before| is_element(nodes.get(before))) && is_element(nodes.get(index + 1))
}

/// Whether a collection that takes `takes` accepts a child of type `info`; `None` takes any.
fn accepts(takes: Option<&[&TypeInfo]>, info: &TypeInfo) -> bool {
  takes.is_none_or(|takes| takes.iter().any(|taken| taken.rust_path == info.rust_path))
}

/// Whether `node` is text that adds nothing, such as indentation between elements.
fn is_blank(node: &Content) -> bool {
  matches!(node, Content::Text { text, .. } if text.is_empty())
}
