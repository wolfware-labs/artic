use crate::ui_type::fields::{Analysis, Role, UiField, generic_argument, ui_name};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Type;

/// The builder items a derived type gets, written inside its inherent `impl`: `new`, `build` and
/// one method per `Property` field, which sets the field's `Local` level.
pub fn builders(analysis: &Analysis) -> TokenStream {
  let ident = analysis.ident;
  let new_doc = format!("Creates a `{ident}` with each property at its default.");
  let build_doc = format!("Adds this `{ident}` to `tree` and returns its handle.");
  let methods = analysis.ui_fields.iter().filter_map(property_method);
  quote! {
    #[doc = #new_doc]
    #[must_use]
    pub fn new() -> Self {
      <Self as ::core::default::Default>::default()
    }

    #[doc = #build_doc]
    pub fn build(self, tree: &mut __artic::UiTree) -> __artic::Handle<Self> {
      tree.alloc_typed(self)
    }

    #(#methods)*
  }
}

/// The builder method of a `Property` field, or `None` for another field. A `Property<Option<V>>`
/// takes a `V`: `None` is the default, so a builder only ever sets a value.
///
/// The method takes `impl Into<T>`, except for a number type, which it takes as is: a literal
/// such as `8.0` is an `f64` until something fixes its type, and `impl Into<f32>` doesn't.
fn property_method(field: &UiField) -> Option<TokenStream> {
  let Role::Property { inner, value } = field.role else {
    return None;
  };
  let field_ident = field.ident;
  let name = ui_name(field);
  let optional = generic_argument(inner, "Option").is_some();
  let argument = if optional { value } else { inner };
  let (parameter, converted) = if is_number(argument) {
    (quote!(#argument), quote!(value))
  } else {
    (
      quote!(impl ::core::convert::Into<#argument>),
      quote!(::core::convert::Into::into(value)),
    )
  };
  let (stored, doc) = if optional {
    (
      quote!(::core::option::Option::Some(#converted)),
      format!("Sets `{name}` to `Some(value)`, the local value."),
    )
  } else {
    (converted, format!("Sets `{name}`, the local value."))
  };
  Some(quote! {
    #[doc = #doc]
    #[must_use]
    pub fn #field_ident(self, value: #parameter) -> Self {
      self.#field_ident.set(#stored);
      self
    }
  })
}

/// Whether `ty` is written as one of Rust's number types, such as `f32` or `u32`.
fn is_number(ty: &Type) -> bool {
  const NUMBERS: &[&str] = &[
    "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
  ];
  let Type::Path(path) = ty else {
    return false;
  };
  path.qself.is_none()
    && path
      .path
      .get_ident()
      .is_some_and(|ident| NUMBERS.iter().any(|number| ident == number))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui_type::fields::analyze;
  use syn::{DeriveInput, parse_quote};

  /// The builder items for `input`, inside `impl Name { … }` and pretty-printed.
  fn expand(input: &DeriveInput) -> String {
    let analysis = analyze(input).expect("valid struct");
    let ident = analysis.ident;
    let items = builders(&analysis);
    let file: syn::File = syn::parse2(quote!(impl #ident { #items })).expect("generated items parse");
    prettyplease::unparse(&file)
  }

  // The spec's `Border` gets `new`, `build` and one method per property; an `Option` property
  // takes the value inside, a number is taken as is, and the event gets nothing yet.
  #[test]
  fn border_builders() {
    let input: DeriveInput = parse_quote! {
      struct Border {
        #[ui(content)] child: Child,
        #[ui(default = Thickness::ZERO, affects = layout)] padding: Property<Thickness>,
        #[ui(affects = render)] background: Property<Option<Brush>>,
        #[ui] opacity: Property<f32>,
        #[ui] pointer_pressed: Event<PointerEvent>,
      }
    };

    insta::assert_snapshot!(expand(&input));
  }
}
