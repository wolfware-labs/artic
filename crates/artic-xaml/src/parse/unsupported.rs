//! The MS-XAML and WPF constructs artic rejects, each with why and what to write instead.

use super::namespace::UI_URI;
use crate::diagnostics::{Diagnostic, Span};
use crate::infoset::{TypeRef, XamlNamespace};

/// A construct artic rejects.
pub(crate) struct Unsupported {
  /// The construct as written, which is also how it's looked up: `x:Shared`, `Trigger`,
  /// `clr-namespace:`.
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
  Unsupported {
    construct: "Trigger",
    reason: "styles react to state with selectors",
    suggestion: "use a selector with a pseudo-class, like `Button:pointerover`",
  },
  Unsupported {
    construct: "MultiTrigger",
    reason: "styles react to state with selectors",
    suggestion: "use a selector with a pseudo-class, like `Button:pointerover`",
  },
  Unsupported {
    construct: "DataTrigger",
    reason: "styles react to state with selectors",
    suggestion: "use a selector with a pseudo-class, like `Button:pointerover`",
  },
  Unsupported {
    construct: "MultiDataTrigger",
    reason: "styles react to state with selectors",
    suggestion: "use a selector with a pseudo-class, like `Button:pointerover`",
  },
  Unsupported {
    construct: "EventTrigger",
    reason: "styles react to state with selectors",
    suggestion: "use a selector with a pseudo-class, like `Button:pointerover`",
  },
  Unsupported {
    construct: "http://schemas.microsoft.com/winfx/2006/xaml",
    reason: "artic's XAML language namespace has its own URI",
    suggestion: "`xmlns:x=\"https://artic.rs/markup\"`",
  },
  Unsupported {
    construct: "http://schemas.microsoft.com/winfx/2006/xaml/presentation",
    reason: "artic's controls are their own vocabulary",
    suggestion: "`xmlns=\"https://artic.rs/ui\"`",
  },
  Unsupported {
    construct: "clr-namespace:",
    reason: "prefixes map to Rust modules, not .NET namespaces",
    suggestion: "`using:` with the module path, like `using:my_app::views`",
  },
];

/// The types of the markup namespace artic supports: `x:Type`, `x:Static`, `x:Null` and the XAML
/// 2009 primitives.
const MARKUP_TYPES: &[&str] = &["Type", "Static", "Null", "String", "Int32", "Double", "Boolean"];

/// Everything the markup namespace supports, for the help of an unknown name.
const MARKUP_NAMES: &str = "directives `x:Name`, `x:Key`, `x:Class`, `x:DataType`, `x:Uid` and `x:FieldModifier`; \
  types `x:Type`, `x:Static`, `x:Null`, `x:String`, `x:Int32`, `x:Double` and `x:Boolean`";

/// The error for `construct`, written at `span`, if the table rejects it.
pub(super) fn rejection(construct: &str, span: Span) -> Option<Diagnostic> {
  let row = UNSUPPORTED.iter().find(|row| row.construct == construct)?;
  Some(Diagnostic::error(span, format!("`{}` isn't supported: {}", row.construct, row.reason)).with_help(row.suggestion))
}

/// The error for a type artic doesn't support: a rejected or unknown name in the markup namespace,
/// or a WPF trigger in the artic UI vocabulary.
pub(super) fn type_problem(ty: &TypeRef) -> Option<Diagnostic> {
  match &ty.namespace {
    XamlNamespace::Markup if !MARKUP_TYPES.contains(&ty.name.as_str()) => {
      Some(markup_problem(ty.prefix.as_deref(), &ty.name, ty.span))
    }
    XamlNamespace::Uri(uri) if uri == UI_URI => rejection(&ty.name, ty.span),
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
      .with_help(format!("it has {MARKUP_NAMES}"))
  })
}
