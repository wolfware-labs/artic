//! The MS-XAML and WPF constructs artic rejects, each with why and what to write instead.

use crate::infoset::{Directive, TypeRef, XamlNamespace};
use crate::namespaces::ARTIC_UI;
use artic_diagnostics::{Diagnostic, Span};

/// A construct artic rejects.
pub(crate) struct Unsupported {
  /// The construct as written, which is also how it's looked up: `x:Shared`, `Trigger`.
  pub(crate) construct: &'static str,
  /// Why artic doesn't support it.
  pub(crate) reason: &'static str,
  /// What to write instead.
  pub(crate) suggestion: &'static str,
}

/// Every construct the parser rejects by name: the spec's "Not supported" list (§4).
pub(crate) const UNSUPPORTED: &[Unsupported] = &[
  Unsupported {
    construct: "x:Code",
    reason: "views' logic is Rust, not inline code",
    suggestion: "put the code in the view's `#[ui_class]` struct",
  },
  Unsupported {
    construct: "x:Subclass",
    reason: "the `x:Class` struct is the view's type",
    suggestion: "remove `x:Subclass`",
  },
  Unsupported {
    construct: "x:Shared",
    reason: "each resource is created once and shared",
    suggestion: "create one instance per use with a template",
  },
  Unsupported {
    construct: "x:Arguments",
    reason: "objects are created with `Default`, then their properties are set",
    suggestion: "set properties instead of passing constructor arguments",
  },
  Unsupported {
    construct: "x:FactoryMethod",
    reason: "objects are created with `Default`, then their properties are set",
    suggestion: "set properties instead of calling a factory method",
  },
  Unsupported {
    construct: "x:TypeArguments",
    reason: "no generics from XAML in v1",
    suggestion: "use a concrete type, or wrap the generic type in a non-generic one in Rust",
  },
  Unsupported {
    construct: "x:Reference",
    reason: "elements refer to each other through bindings",
    suggestion: "use `{Binding ElementName=name}`",
  },
  trigger("Trigger"),
  trigger("MultiTrigger"),
  trigger("DataTrigger"),
  trigger("MultiDataTrigger"),
  trigger("EventTrigger"),
];

/// The types of the markup namespace artic supports: `x:Type`, `x:Static`, `x:Null` and the XAML
/// 2009 primitives.
const MARKUP_TYPES: &[&str] = &["Type", "Static", "Null", "String", "Int32", "Double", "Boolean"];

/// A WPF trigger, which artic's styles replace with selectors.
const fn trigger(construct: &'static str) -> Unsupported {
  Unsupported {
    construct,
    reason: "styles react to state with selectors",
    suggestion: "use a selector with a pseudo-class, like `Button:pointerover`",
  }
}

/// Everything the markup namespace supports, for the help of an unknown name: its directives,
/// then its types.
fn markup_names() -> String {
  format!(
    "directives {}; types {}",
    listed(Directive::ALL.map(Directive::name)),
    listed(MARKUP_TYPES.iter().copied())
  )
}

/// `names` as `x:` names in a sentence: `` `x:A`, `x:B` and `x:C` ``.
fn listed(names: impl IntoIterator<Item = &'static str>) -> String {
  let names: Vec<String> = names.into_iter().map(|name| format!("`x:{name}`")).collect();
  match names.split_last() {
    Some((last, [])) => last.clone(),
    Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    None => String::new(),
  }
}

/// The error for `construct`, written at `span`, if the table rejects it.
pub(super) fn rejection(construct: &str, span: Span) -> Option<Diagnostic> {
  let row = UNSUPPORTED.iter().find(|row| row.construct == construct)?;
  Some(not_supported(row.construct, row.reason, span).with_help(row.suggestion))
}

/// The error for `construct`, written at `span`, which artic doesn't support for `reason`.
pub(super) fn not_supported(construct: &str, reason: &str, span: Span) -> Diagnostic {
  Diagnostic::error(span, format!("`{construct}` isn't supported: {reason}"))
}

/// The error for a type artic doesn't support: a rejected or unknown name in the markup namespace,
/// or a WPF trigger in the artic UI vocabulary.
pub(super) fn type_problem(ty: &TypeRef) -> Option<Diagnostic> {
  match &ty.namespace {
    XamlNamespace::Markup if !MARKUP_TYPES.contains(&ty.name.as_str()) => {
      Some(markup_problem(ty.prefix.as_deref(), &ty.name, ty.span))
    }
    XamlNamespace::Uri(uri) if uri == ARTIC_UI => rejection(&ty.name, ty.span),
    _ => None,
  }
}

/// The error for `name` in the markup namespace, written with `prefix` at `span`, that is neither
/// a supported directive nor a supported type.
pub(super) fn markup_problem(prefix: Option<&str>, name: &str, span: Span) -> Diagnostic {
  rejection(&format!("x:{name}"), span).unwrap_or_else(|| {
    let written = match prefix {
      Some(prefix) => format!("{prefix}:{name}"),
      None => name.to_owned(),
    };
    Diagnostic::error(span, format!("`{written}` isn't part of the XAML language namespace"))
      .with_help(format!("it has {}", markup_names()))
  })
}
