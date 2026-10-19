use crate::crate_root::{ENUM, crate_root};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, Ident};

/// Writes a `FromStr` impl that matches the enum's variant names, ignoring ASCII case, and a
/// `ToRust` impl that writes the variant's path.
pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
  let variants = unit_variants(input)?;
  let root = crate_root(None, &input.ident, ENUM)?;
  let ident = &input.ident;
  let type_name = ident.to_string();
  let names: Vec<String> = variants.iter().map(ToString::to_string).collect();
  let paths: Vec<String> = names.iter().map(|name| format!("::{name}")).collect();
  Ok(quote! {
    impl ::core::str::FromStr for #ident {
      type Err = #root::__private::ParseEnumError;

      fn from_str(text: &str) -> ::core::result::Result<Self, Self::Err> {
        #(
          if text.eq_ignore_ascii_case(#names) {
            return ::core::result::Result::Ok(Self::#variants);
          }
        )*
        ::core::result::Result::Err(#root::__private::ParseEnumError {
          value: ::std::string::String::from(text),
          type_name: #type_name,
          expected: &[#(#names),*],
        })
      }
    }

    impl #root::__private::ToRust for #ident {
      fn to_rust(&self, out: &mut #root::__private::RustExpr) {
        out.type_path(::core::env!("CARGO_CRATE_NAME"), #type_name);
        out.raw(match self {
          #(Self::#variants => #paths,)*
        });
      }
    }
  })
}

/// Returns the enum's variants, or an error if the input isn't an enum, has no variants, has a
/// variant with fields, or has two variants whose names differ only in case.
fn unit_variants(input: &DeriveInput) -> syn::Result<Vec<&Ident>> {
  let Data::Enum(data) = &input.data else {
    return Err(Error::new_spanned(&input.ident, "`UiEnum` can only be derived for an enum"));
  };
  if data.variants.is_empty() {
    return Err(Error::new_spanned(&input.ident, "`UiEnum` needs at least one variant"));
  }
  let mut variants: Vec<&Ident> = Vec::new();
  for variant in &data.variants {
    if !matches!(variant.fields, Fields::Unit) {
      return Err(Error::new_spanned(
        &variant.fields,
        "`UiEnum` variants can't have fields: each one is a name written as text",
      ));
    }
    let name = variant.ident.to_string();
    if let Some(earlier) = variants
      .iter()
      .find(|earlier| earlier.to_string().eq_ignore_ascii_case(&name))
    {
      return Err(Error::new_spanned(
        &variant.ident,
        format_args!("`{name}` matches the same text as `{earlier}` when case is ignored"),
      ));
    }
    variants.push(&variant.ident);
  }
  Ok(variants)
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use syn::parse_quote;

  // Unit variants are accepted, in declaration order.
  #[test]
  fn unit_variants_are_collected_in_order() {
    let input: DeriveInput = parse_quote! { enum Orientation { Horizontal, Vertical = 2 } };

    let variants = unit_variants(&input).expect("valid enum");

    assert_eq!(
      variants.iter().map(ToString::to_string).collect::<Vec<_>>(),
      ["Horizontal", "Vertical"]
    );
  }

  // Two variants whose names differ only in case are rejected, naming both.
  #[test]
  fn names_equal_ignoring_case_are_an_error() {
    let input: DeriveInput = parse_quote! { enum Size { Auto, AUTO } };

    let error = unit_variants(&input).expect_err("ambiguous names");

    assert_eq!(
      error.to_string(),
      "`AUTO` matches the same text as `Auto` when case is ignored"
    );
  }

  // An enum with no variants is rejected.
  #[test]
  fn empty_enum_is_an_error() {
    let input: DeriveInput = parse_quote! { enum Never {} };

    assert!(unit_variants(&input).is_err());
  }
}
