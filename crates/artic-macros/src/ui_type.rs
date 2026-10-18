pub(crate) mod attributes;
pub(crate) mod fields;
pub(crate) mod generate;

use fields::analyze;
use generate::generate;
use proc_macro2::TokenStream;
use syn::DeriveInput;

pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
  generate(&analyze(input)?)
}
