//! Turning a document's infoset into a [`View`]: every type resolved, every literal written, and
//! the elements ordered children first.

use crate::resolve::resolve;
use crate::unsupported;
use artic_registry::Registry;
use artic_xaml::diagnostics::{Diagnostic, Diagnostics};
use artic_xaml::infoset::{MemberKey, XamlDocument, XamlNode};
use artic_xaml::namespaces::NamespaceMap;
use std::collections::BTreeMap;
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
  let mut lower = Lower {
    registry,
    namespaces,
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
  view: View,
  diagnostics: Diagnostics,
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
    for member in members.iter().filter(|member| matches!(member.key, MemberKey::Content | MemberKey::Items)) {
      for child in member.value.iter().filter(|node| matches!(node, XamlNode::Object { .. })) {
        self.element(child);
      }
    }
    let resolved = resolved?;
    self.view.elements.push(Element {
      ty: resolved.path,
      setters: Vec::new(),
      content: Content::None,
    });
    Some(self.view.elements.len() - 1)
  }

  /// The value, or `None` after recording the diagnostic.
  fn report<T>(&mut self, result: Result<T, Diagnostic>) -> Option<T> {
    result.map_err(|diagnostic| self.diagnostics.push(diagnostic)).ok()
  }
}
