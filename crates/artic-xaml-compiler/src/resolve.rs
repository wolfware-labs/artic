//! Finding the type a markup name stands for, and the path generated code names it by.

use crate::unsupported;
use artic_diagnostics::{Diagnostic, Diagnostics, Span};
use artic_registry::{Lookup, Registry, TypeInfo};
use artic_xaml::infoset::{TypeRef, XamlNamespace};
use artic_xaml::namespaces::NamespaceMap;
use syn::Path;
use syn::parse::Parser;

/// A type named in markup, with the path generated code names it by.
pub(crate) struct Resolved {
  /// The type's descriptor.
  pub info: &'static TypeInfo,
  /// The type's path in generated code, such as `::artic::controls::Border`.
  pub path: Path,
}

/// Finds the type `ty` names: through the crates its URI stands for, or at the module its
/// `using:` namespace gives. Records why in `diagnostics` and returns `None` when there is none.
pub(crate) fn resolve(
  ty: &TypeRef,
  registry: &Registry,
  namespaces: &NamespaceMap,
  diagnostics: &mut Diagnostics,
) -> Option<Resolved> {
  let found = match &ty.namespace {
    XamlNamespace::Markup => Err(Box::new(unsupported::primitive(ty))),
    XamlNamespace::Uri(uri) => by_uri(ty, uri, registry, namespaces),
    XamlNamespace::Using(module) => by_module(ty, module, registry, namespaces),
  };
  let (info, text) = found.map_err(|diagnostic| diagnostics.push(*diagnostic)).ok()?;
  let path = parse_path(&text, ty.span, diagnostics)?;
  Some(Resolved { info, path })
}

/// Parses `text`, built from the namespace map's paths, as the path of an item, or records that
/// it isn't one. The path is read as a module path, without generic arguments, since generated
/// code writes it before `::new()` and in `use` items.
pub(crate) fn parse_path(text: &str, span: Span, diagnostics: &mut Diagnostics) -> Option<Path> {
  let parsed = Path::parse_mod_style.parse_str(text).ok();
  if parsed.is_none() {
    diagnostics.push(
      Diagnostic::error(span, format!("`{text}` isn't a Rust path")).with_help("check the paths set with `NamespaceMap::path`"),
    );
  }
  parsed
}

/// A type found, with the text of its path in generated code; or why none was.
type Found = Result<(&'static TypeInfo, String), Box<Diagnostic>>;

/// The one type named `ty.name` in the crates `uri` stands for, written `<crate path>::<Name>`.
fn by_uri(ty: &TypeRef, uri: &str, registry: &Registry, namespaces: &NamespaceMap) -> Found {
  let crates = namespaces.crates(uri).unwrap_or_default();
  let mut found: Vec<&'static TypeInfo> = crates
    .iter()
    .flat_map(|krate| match registry.type_by_name(krate, &ty.name) {
      Lookup::Found(info) => vec![info],
      Lookup::Ambiguous(infos) => infos,
      Lookup::Missing => Vec::new(),
    })
    .collect();
  found.sort_by_key(|info| info.rust_path);
  match found.as_slice() {
    [info] => Ok((info, format!("{}::{}", namespaces.path_of(info.namespace()), info.name))),
    [] => Err(unknown(ty, &format!("`{uri}`"), crates.iter().map(String::as_str))),
    many => {
      let paths: Vec<String> = many.iter().map(|info| format!("`{}`", info.rust_path)).collect();
      Err(Box::new(
        Diagnostic::error(ty.span, format!("`{}` is ambiguous in `{uri}`", ty.name))
          .with_label("ambiguous type")
          .with_help(format!(
            "it's defined as {}; use a `using:` namespace to pick one",
            paths.join(", ")
          )),
      ))
    }
  }
}

/// The type at `<module>::<Name>`, written with the module path the document gave, its crate
/// translated through the map.
fn by_module(ty: &TypeRef, module: &str, registry: &Registry, namespaces: &NamespaceMap) -> Found {
  let (krate, rest) = module.split_once("::").unwrap_or((module, ""));
  let Some(info) = registry.type_by_rust_path(&format!("{module}::{}", ty.name)) else {
    return Err(unknown(ty, &format!("`using:{module}`"), [krate]));
  };
  let module_path = if rest.is_empty() {
    namespaces.path_of(krate)
  } else {
    format!("{}::{rest}", namespaces.path_of(krate))
  };
  Ok((info, format!("{module_path}::{}", info.name)))
}

/// The unknown-type error, with help when one of `crates` is the crate whose build script runs,
/// whose types don't exist yet.
fn unknown<'a>(ty: &TypeRef, namespace: &str, crates: impl IntoIterator<Item = &'a str>) -> Box<Diagnostic> {
  let diagnostic = Diagnostic::error(ty.span, format!("no type `{}` in {namespace}", ty.name)).with_label("unknown type");
  let Ok(app) = std::env::var("CARGO_PKG_NAME").map(|name| name.replace('-', "_")) else {
    return Box::new(diagnostic);
  };
  Box::new(if crates.into_iter().any(|krate| krate == app) {
    diagnostic.with_help(format!(
      "a type defined in `{app}` doesn't exist yet when its build script runs: move `{}` to a library crate and add that crate to `[build-dependencies]`",
      ty.name
    ))
  } else {
    diagnostic
  })
}
