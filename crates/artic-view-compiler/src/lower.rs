//! Turning a [`CheckedView`] into Rust: each type's path, each value as an expression, and the
//! traits the setters call through.

use crate::CratePaths;
use artic_diagnostics::{Diagnostic, Diagnostics, Span};
use artic_registry::{PropertyInfo, PropertyRef, RustExpr, TypeInfo};
use artic_view::document::{Text, TypeUse};
use artic_view::{CheckedContent, CheckedElement, CheckedView};
use std::collections::{BTreeMap, BTreeSet};
use syn::parse::Parser;
use syn::{Expr, Ident, Path};

/// A view ready to be written.
pub(crate) struct View {
  /// The path of `artic_core`'s tree module.
  pub tree: Path,
  /// Every element, children before their parent, so element `i` is the local `__e{i}`.
  pub elements: Vec<Element>,
  /// The root's index.
  pub root: usize,
  /// The `View` fields the names give, in the order their elements are built, each with its
  /// element's index.
  pub names: Vec<(Ident, usize)>,
  /// The methods traits the setters call through, keyed by their text so they're written sorted.
  pub traits: BTreeMap<String, Path>,
  /// Every crate the language names or has a path, by its path, so the app links them.
  pub links: Vec<Path>,
}

/// One element: its type, the builder methods that set its properties, and its children.
pub(crate) struct Element {
  /// The type's path in generated code.
  pub ty: Path,
  /// The property methods, in the order the document sets them.
  pub setters: Vec<Setter>,
  /// The children its content method takes.
  pub content: CheckedContent,
}

/// A builder method call: `.method(value)`.
pub(crate) struct Setter {
  /// The method, named after the property's field.
  pub method: Ident,
  /// The expression that builds the value.
  pub value: Expr,
}

/// Lowers `view`, collecting every problem. The view is `None` when it has no root or a path
/// isn't a Rust path; every value is still written, so each of its problems is reported.
pub(crate) fn lower(view: &CheckedView, paths: &CratePaths, crates: &[&str]) -> (Option<View>, Diagnostics) {
  let mut expr = RustExpr::new();
  for krate in paths.crates_named() {
    expr.path(krate, &paths.path_of(krate));
  }
  let mut lower = Lower {
    paths,
    expr,
    traits: BTreeMap::new(),
    diagnostics: Diagnostics::new(),
  };
  let elements: Vec<Option<Element>> = view.elements.iter().map(|element| lower.element(element)).collect();
  let tree = parse_path(&paths.module("artic_core::tree"), view.span, &mut lower.diagnostics);
  let links: Vec<Option<Path>> = crates
    .iter()
    .copied()
    .chain(paths.crates_named())
    .collect::<BTreeSet<_>>()
    .into_iter()
    .map(|krate| parse_path(&paths.path_of(krate), view.span, &mut lower.diagnostics))
    .collect();
  let names = view
    .names
    .iter()
    .map(|name| {
      (
        syn::parse_str(&name.field).expect("a field name is an identifier"),
        name.element,
      )
    })
    .collect();
  let elements: Option<Vec<Element>> = elements.into_iter().collect();
  let links: Option<Vec<Path>> = links.into_iter().collect();
  let lowered = match (view.root, tree, elements, links) {
    (Some(root), Some(tree), Some(elements), Some(links)) => Some(View {
      tree,
      elements,
      root,
      names,
      traits: lower.traits,
      links,
    }),
    _ => None,
  };
  (lowered, lower.diagnostics)
}

/// Parses `text`, built from the crates' paths, as the path of an item, or records that
/// it isn't one. The path is read as a module path, without generic arguments, since generated
/// code writes it before `::new()` and in `use` items.
fn parse_path(text: &str, span: Span, diagnostics: &mut Diagnostics) -> Option<Path> {
  let parsed = Path::parse_mod_style.parse_str(text).ok();
  if parsed.is_none() {
    diagnostics.push(
      Diagnostic::error(span, format!("`{text}` isn't a Rust path")).with_help("check the paths set with `CratePaths::path`"),
    );
  }
  parsed
}

/// The walk's state.
struct Lower<'a> {
  paths: &'a CratePaths,
  /// An empty expression that knows the crates' paths, cloned for each literal.
  expr: RustExpr,
  /// The methods traits recorded so far.
  traits: BTreeMap<String, Path>,
  diagnostics: Diagnostics,
}

impl Lower<'_> {
  /// Lowers one element, or `None` when its type's path isn't a Rust path.
  fn element(&mut self, element: &CheckedElement) -> Option<Element> {
    let ty = self.type_path(&element.ty);
    let setters = element
      .properties
      .iter()
      .filter_map(|(property, value)| self.setter(element.ty.info, *property, value))
      .collect();
    Some(Element {
      ty: ty?,
      setters,
      content: element.content.clone(),
    })
  }

  /// The path generated code names `ty` by: at its crate's root, or in the module the document
  /// named, its crate translated through the paths.
  fn type_path(&mut self, ty: &TypeUse) -> Option<Path> {
    let text = match &ty.module {
      None => self.paths.item(ty.info.rust_path),
      Some(module) => format!("{}::{}", self.paths.module(module), ty.info.name),
    };
    parse_path(&text, ty.span, &mut self.diagnostics)
  }

  /// The setter for `property` of `info`, set to `value`, or `None` after reporting a value that
  /// can't be written.
  fn setter(&mut self, info: &TypeInfo, property: PropertyRef, value: &Text) -> Option<Setter> {
    if let Some(group) = property.group {
      self.import_group(info, group, value.span);
    }
    let value = self.literal(property.info, &value.text, value.span)?;
    let method = syn::parse_str(property.info.field).expect("a registry field is an identifier");
    Some(Setter { method, value })
  }

  /// Records the methods trait of `info`'s group `field`, which its properties' setters call.
  fn import_group(&mut self, info: &TypeInfo, field: &str, span: Span) {
    let group = info
      .groups
      .iter()
      .find(|group| group.field == field)
      .expect("a property's group is one of its type's");
    let text = self.paths.item(group.methods);
    if !self.traits.contains_key(&text)
      && let Some(path) = parse_path(&text, span, &mut self.diagnostics)
    {
      self.traits.insert(text, path);
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
    let mut out = self.expr.clone();
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
}
