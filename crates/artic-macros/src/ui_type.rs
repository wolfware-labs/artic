pub(crate) mod analysis;
pub(crate) mod attributes;
pub(crate) mod builders;
pub(crate) mod generate;

use analysis::analyze;
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

  // A type with required fields: `new` takes them in order, and there is no `Default` and no
  // `TypeInfo::new`.
  #[test]
  fn required_type_expansion() {
    let input: DeriveInput = parse_quote! {
      #[ui(override_defaults)]
      pub struct Login {
        #[ui(required)] pub ui: View,
        #[ui(default = 1.0)] pub width: Property<f32>,
        #[ui(required)] session: u32,
        attempts: u8,
      }
    };

    let file: syn::File = syn::parse2(expand(&input).expect("valid")).expect("parses");
    insta::assert_snapshot!(prettyplease::unparse(&file));
  }

  // Each setting only a node type has is refused on a value, at the type or at the field.
  #[test]
  fn a_value_refuses_node_settings() {
    let node_setting = "a value takes no `attached`, `value_provider`, `override_defaults`, `code_type_root` or `whitespace`";
    let plain_field = "a value's `#[ui]` fields are plain data, such as `#[ui] color: Color`, with no property settings";
    let collection = "a value's content is a collection of values, such as `Vec<GradientStop>`";
    let node_field = "a value's `#[ui]` fields are plain data, not a `NodeId`: its node would stay in the tree with no owner \
                      once the value is taken out";
    let inputs: [(DeriveInput, &str); 11] = [
      (
        parse_quote!(
          #[ui(value, value_provider(positional(a)))]
          struct V {
            #[ui]
            a: f32,
          }
        ),
        node_setting,
      ),
      (
        parse_quote!(
          #[ui(value, override_defaults)]
          struct V {
            #[ui]
            a: f32,
          }
        ),
        node_setting,
      ),
      (
        parse_quote!(
          #[ui(value, code_type_root)]
          struct V {
            #[ui]
            a: f32,
          }
        ),
        node_setting,
      ),
      (
        parse_quote!(
          #[ui(value, whitespace = "significant")]
          struct V {
            #[ui]
            a: f32,
          }
        ),
        node_setting,
      ),
      (
        parse_quote!(
          #[ui(value)]
          struct V {
            #[ui]
            clicked: Event<()>,
          }
        ),
        plain_field,
      ),
      (
        parse_quote!(
          #[ui(value)]
          struct V {
            #[ui(flatten)]
            layout: Layout,
          }
        ),
        plain_field,
      ),
      (
        parse_quote!(
          #[ui(value)]
          struct V {
            #[ui(required)]
            a: f32,
          }
        ),
        plain_field,
      ),
      (
        parse_quote!(
          #[ui(value)]
          struct V {
            #[ui(affects = layout)]
            a: f32,
          }
        ),
        plain_field,
      ),
      (
        parse_quote!(
          #[ui(value)]
          struct V {
            #[ui(content)]
            text: Property<String>,
          }
        ),
        collection,
      ),
      (
        parse_quote!(
          #[ui(value)]
          struct V {
            #[ui]
            sample: NodeId,
          }
        ),
        node_field,
      ),
      (
        parse_quote!(
          #[ui(value)]
          struct V {
            #[ui]
            sample: Option<artic::tree::NodeId>,
          }
        ),
        node_field,
      ),
    ];

    for (input, message) in inputs {
      assert_eq!(expand(&input).expect_err(message).to_string(), message);
    }
  }
}
