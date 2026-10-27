pub(crate) mod attributes;
pub(crate) mod builders;
pub(crate) mod fields;
pub(crate) mod generate;

use fields::analyze;
use generate::generate;
use proc_macro2::TokenStream;
use syn::DeriveInput;

pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
  generate(&analyze(input)?)
}

#[cfg(test)]
mod tests {
  use super::*;
  use syn::parse_quote;

  // A type using every setting, expanded whole: the snapshot pins everything the derive writes.
  #[test]
  fn whole_type_expansion() {
    let input: DeriveInput = parse_quote! {
      #[ui(
        whitespace = "significant",
        value_provider(positional(width)),
        override_defaults,
        attached(name = "Row", ty = usize, default = 0, affects = layout, target = children),
        attached(name = "ColumnSpan", ty = usize, affects = layout | render, inherits, target = any)
      )]
      pub struct Panel {
        #[ui(content)] pub children: Children,
        #[ui(default = 1.0, affects = layout | render)] pub width: Property<f32>,
        #[ui(affects = render, inherits = and)] pub enabled: Property<bool>,
        #[ui(key = SHARED, default = 2)] pub shared: Property<i32>,
        #[ui(name = "Label")] pub text: Property<Option<String>>,
        #[ui] pub clicked: Event<()>,
        #[ui(flatten)] pub layout: Layout,
        cache: Vec<u8>,
      }
    };

    let file: syn::File = syn::parse2(expand(&input).expect("valid")).expect("parses");
    insta::assert_snapshot!(prettyplease::unparse(&file));
  }
}
