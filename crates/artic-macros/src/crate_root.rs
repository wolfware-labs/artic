use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::{Error, Ident, Path};

/// The crates the UI derives reach artic through: (package, library).
pub const REGISTRY: &[(&str, &str)] = &[("artic", "artic"), ("artic-registry", "artic_registry")];

/// The crates `#[derive(UiEnum)]` reaches artic through: (package, library). Its output names
/// only `ParseEnumError`, which lives in `artic-core`.
pub const ENUM: &[(&str, &str)] = &[
  ("artic", "artic"),
  ("artic-registry", "artic_registry"),
  ("artic-core", "artic_core"),
];

/// The crates `#[element]` reaches artic through: (package, library).
pub const ELEMENT: &[(&str, &str)] = &[("artic", "artic"), ("artic-element", "artic_element")];

/// The path generated code uses to reach artic: `override_path` if given, or the first of
/// `candidates` the calling crate depends on. Generated code then names
/// `<root>::__private::...`. The error points at `target`.
pub fn crate_root(
  override_path: Option<&Path>,
  target: &impl ToTokens,
  candidates: &[(&str, &str)],
) -> syn::Result<TokenStream> {
  if let Some(path) = override_path {
    return Ok(quote!(#path));
  }
  for (package, library) in candidates {
    match crate_name(package) {
      Ok(FoundCrate::Name(name)) => {
        let name = Ident::new(&name, Span::call_site());
        return Ok(quote!(::#name));
      }
      Ok(FoundCrate::Itself) => {
        let name = Ident::new(library, Span::call_site());
        return Ok(quote!(::#name));
      }
      Err(_) => {}
    }
  }
  let names: Vec<String> = candidates.iter().map(|(package, _)| format!("`{package}`")).collect();
  Err(Error::new_spanned(
    target,
    format!("this macro needs {} in this crate's dependencies", names.join(" or ")),
  ))
}
