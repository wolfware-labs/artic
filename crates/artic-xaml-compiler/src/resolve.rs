//! Finding the type a markup name stands for.

use crate::unsupported;
use artic_diagnostics::Diagnostic;
use artic_registry::{Lookup, Registry, TypeInfo, crate_of};
use artic_view::document::{Problem, TypeUse};
use artic_xaml::infoset::{TypeRef, XamlNamespace};
use artic_xaml::namespaces::NamespaceMap;

/// Finds the type `ty` names: through the crates its URI stands for, or at the module its
/// `using:` namespace gives. Returns the problem the checker reports when there is none, or when
/// it's a value provider, which an element can't be. A type missing from `app_crate` gets help.
pub(crate) fn resolve(
  ty: &TypeRef,
  registry: &Registry,
  namespaces: &NamespaceMap,
  app_crate: Option<&str>,
) -> Result<TypeUse, Problem> {
  let (info, module) = match &ty.namespace {
    XamlNamespace::Markup => return Err(unsupported::primitive(ty).into()),
    XamlNamespace::Uri(uri) => (by_uri(ty, uri, registry, namespaces, app_crate)?, None),
    XamlNamespace::Using(module) => (by_module(ty, module, registry, app_crate)?, Some(module.clone())),
  };
  if info.is_value_provider() {
    return Err(unsupported::provider_as_element(ty).into());
  }
  Ok(TypeUse {
    info,
    module,
    span: ty.span,
  })
}

/// The one type named `ty.name` in the crates `uri` stands for.
fn by_uri(
  ty: &TypeRef,
  uri: &str,
  registry: &Registry,
  namespaces: &NamespaceMap,
  app_crate: Option<&str>,
) -> Result<&'static TypeInfo, Problem> {
  let Some(crates) = namespaces.crates(uri) else {
    return Err(Problem::once(
      format!("namespace:{uri}"),
      Diagnostic::error(ty.span, format!("no namespace `{uri}`"))
        .with_label("undeclared namespace")
        .with_help("declare it with `NamespaceMap::alias`, or check its spelling"),
    ));
  };
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
    [info] => Ok(info),
    [] => Err(unknown(ty, &format!("`{uri}`"), crates.iter().map(String::as_str), app_crate)),
    many => {
      let paths: Vec<String> = many.iter().map(|info| format!("`{}`", info.rust_path)).collect();
      Err(
        Diagnostic::error(ty.span, format!("`{}` is ambiguous in `{uri}`", ty.name))
          .with_label("ambiguous type")
          .with_help(format!(
            "it's defined as {}; use a `using:` namespace to pick one",
            paths.join(", ")
          ))
          .into(),
      )
    }
  }
}

/// The type at `<module>::<Name>`.
fn by_module(ty: &TypeRef, module: &str, registry: &Registry, app_crate: Option<&str>) -> Result<&'static TypeInfo, Problem> {
  registry
    .type_by_rust_path(&format!("{module}::{}", ty.name))
    .ok_or_else(|| unknown(ty, &format!("`using:{module}`"), [crate_of(module)], app_crate))
}

/// The unknown-type error, with help when one of `crates` is `app_crate`, the crate whose build
/// script runs, whose types don't exist yet.
fn unknown<'a>(ty: &TypeRef, namespace: &str, crates: impl IntoIterator<Item = &'a str>, app_crate: Option<&str>) -> Problem {
  let diagnostic = Diagnostic::error(ty.span, format!("no type `{}` in {namespace}", ty.name)).with_label("unknown type");
  let Some(app) = app_crate else {
    return diagnostic.into();
  };
  if crates.into_iter().any(|krate| krate == app) {
    diagnostic
      .with_help(format!(
        "a type defined in `{app}` doesn't exist yet when its build script runs: move `{}` to a library crate and add that crate to `[build-dependencies]`",
        ty.name
      ))
      .into()
  } else {
    diagnostic.into()
  }
}
