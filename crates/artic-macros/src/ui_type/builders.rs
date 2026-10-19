use crate::ui_type::fields::{Analysis, Role, UiField, generic_argument, ui_name};
use crate::ui_type::generate::generated_key;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::{Ident, Type};

/// The builder items a derived type gets, written inside its inherent `impl`: `new`, `build`, one
/// method per `Property` field, which sets the field's `Local` level, and a constant per property
/// key the type owns.
pub fn builders(analysis: &Analysis) -> TokenStream {
  let ident = analysis.ident;
  let new_doc = format!("Creates a `{ident}` with each property at its default.");
  let build_doc = format!("Adds this `{ident}` to `tree` and returns its handle.");
  let constants = analysis.ui_fields.iter().filter_map(key_constant);
  let methods = analysis.ui_fields.iter().filter_map(property_method);
  quote! {
    #(#constants)*

    #[doc = #new_doc]
    #[must_use]
    pub fn new() -> Self {
      <Self as ::core::default::Default>::default()
    }

    #[doc = #build_doc]
    #[must_use = "the handle is needed to link or reach the node"]
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
  let (field_ident, vis) = (field.ident, field.vis);
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
    #vis fn #field_ident(self, value: #parameter) -> Self {
      self.#field_ident.set(#stored);
      self
    }
  })
}

/// Whether `ty` is written as one of Rust's number types: `f32`, or a path to it through
/// `core::primitive` or `std::primitive`. A type alias isn't seen through; a property of one takes
/// `impl Into<T>`, so a literal passed to it needs a suffix, as in `8.0_f32`.
fn is_number(ty: &Type) -> bool {
  const NUMBERS: &[&str] = &[
    "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
  ];
  let Type::Path(path) = ty else {
    return false;
  };
  let segments: Vec<String> = path.path.segments.iter().map(|segment| segment.ident.to_string()).collect();
  let name = match segments.as_slice() {
    [name] if path.path.leading_colon.is_none() => name,
    [krate, primitive, name] if (krate == "core" || krate == "std") && primitive == "primitive" => name,
    _ => return false,
  };
  path.qself.is_none()
    && path.path.segments.iter().all(|segment| segment.arguments.is_none())
    && NUMBERS.contains(&name.as_str())
}

/// The public name of a property's key: the field name without `r#`, in SCREAMING_SNAKE_CASE.
pub fn key_constant_name(field: &Ident) -> Ident {
  format_ident!("{}", field.unraw().to_string().to_uppercase())
}

/// The public constant for a property's own key, or `None` for a field that shares a key, whose
/// owner already names it.
fn key_constant(field: &UiField) -> Option<TokenStream> {
  let Role::Property { inner, .. } = field.role else {
    return None;
  };
  if field.attributes.key.is_some() {
    return None;
  }
  let vis = field.vis;
  let constant = key_constant_name(field.ident);
  let key = generated_key(field.ident);
  let doc = format!("The key of `{}`, for styles.", ui_name(field));
  Some(quote! {
    #[doc = #doc]
    #vis const #constant: &'static __artic::PropertyKey<#inner> = &#key;
  })
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
        #[ui(content)] pub child: Child,
        #[ui(default = Thickness::ZERO, affects = layout)] pub padding: Property<Thickness>,
        #[ui(affects = render)] pub background: Property<Option<Brush>>,
        #[ui] pub opacity: Property<f32>,
        #[ui] pub pointer_pressed: Event<PointerEvent>,
      }
    };

    insta::assert_snapshot!(expand(&input));
  }

  // A field with a shared key gets a method but no constant: the key's owner names it.
  #[test]
  fn shared_key_field_has_a_method_and_no_constant() {
    let input: DeriveInput = parse_quote! {
      struct TextBlock {
        #[ui(key = TextElement::FONT_SIZE)] pub font_size: Property<f32>,
        #[ui] pub text: Property<String>,
      }
    };

    insta::assert_snapshot!(expand(&input));
  }

  // A private field's method and key constant are as private as the field, so a public type
  // doesn't expose them, or its field's private type, through its builder.
  #[test]
  fn builder_items_take_the_field_visibility() {
    let input: DeriveInput = parse_quote! {
      pub struct Card {
        #[ui] pub title: Property<String>,
        #[ui] secret: Property<Secret>,
      }
    };

    let expanded = expand(&input);

    assert!(expanded.contains("pub fn title(") && expanded.contains("pub const TITLE:"));
    assert!(expanded.contains("    fn secret(") && !expanded.contains("pub fn secret("));
    assert!(expanded.contains("    const SECRET:") && !expanded.contains("pub const SECRET:"));
  }

  // A number type written through `core::primitive` or `std::primitive` is still a number.
  #[test]
  fn primitive_paths_are_numbers() {
    let input: DeriveInput = parse_quote! {
      pub struct Gauge {
        #[ui] pub low: Property<core::primitive::f32>,
        #[ui] pub high: Property<::std::primitive::u32>,
      }
    };

    let expanded = expand(&input);

    assert!(expanded.contains("pub fn low(self, value: core::primitive::f32)"));
    assert!(expanded.contains("pub fn high(self, value: ::std::primitive::u32)"));
  }
}
