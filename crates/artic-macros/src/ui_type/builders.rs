use crate::members::emit::{Setter, key_constant, setter};
use crate::members::{Content, ContentMethod, PropertyField};
use crate::ui_type::analysis::TypeAnalysis;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// The builder items a derived type gets, written inside its inherent `impl`: `new`, `build`, one
/// method per `Property` field, which sets the field's `Local` level, the content method for a
/// type whose content is a collection, and a constant per property key the type owns.
pub fn builders(analysis: &TypeAnalysis) -> TokenStream {
  let ident = analysis.ident;
  let properties = &analysis.members.properties;
  let new_doc = format!("Creates a `{ident}` with each property at its default.");
  let build_doc = format!("Adds this `{ident}` to `tree` and returns its handle.");
  let constants = properties.iter().filter_map(key_constant);
  let content = content_builder(analysis);
  let methods = properties.iter().map(property_method);
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

    #content

    #(#methods)*
  }
}

/// The method that takes the children of a type whose content is a collection, or `None`. It
/// returns a `WithChildren`, whose `build` links them once the node exists.
fn content_builder(analysis: &TypeAnalysis) -> Option<TokenStream> {
  let Some(Content::Collection {
    vis,
    method: Some(method),
    ..
  }) = analysis.members.content
  else {
    return None;
  };
  let ident = format_ident!("{}", method.name());
  Some(match method {
    ContentMethod::Child => quote! {
      /// Sets the child, linked when the node is built.
      #[must_use]
      #vis fn #ident(self, child: impl ::core::convert::Into<__artic::NodeId>) -> __artic::WithChildren<Self, ::core::option::Option<__artic::NodeId>> {
        __artic::WithChildren::new(self, ::core::option::Option::Some(::core::convert::Into::into(child)))
      }
    },
    ContentMethod::Children => quote! {
      /// Sets the children, linked in order when the node is built.
      #[must_use]
      #vis fn #ident<I>(self, children: I) -> __artic::WithChildren<Self, I>
      where
        I: ::core::iter::IntoIterator,
        I::Item: ::core::convert::Into<__artic::NodeId>,
      {
        __artic::WithChildren::new(self, children)
      }
    },
  })
}

/// The inherent builder method of a property field.
fn property_method(field: &PropertyField) -> TokenStream {
  let Setter { parameter, stored, doc } = setter(field);
  let (field_ident, vis) = (field.ident, field.vis);
  quote! {
    #[doc = #doc]
    #[must_use]
    #vis fn #field_ident(self, value: #parameter) -> Self {
      self.#field_ident.set(#stored);
      self
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ui_type::analysis::analyze;
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

  // A collection content field gets `children`, generic over what it is given.
  #[test]
  fn children_builder() {
    let input: DeriveInput = parse_quote! {
      pub struct StackPanel {
        #[ui(content)] pub children: Children,
        #[ui] pub spacing: Property<f32>,
      }
    };

    insta::assert_snapshot!(expand(&input));
  }

  // A content collection of values, such as gradient stops, gets no content method: its values
  // aren't child elements, so they don't go through `WithChildren`.
  #[test]
  fn value_collection_gets_no_content_method() {
    let input: DeriveInput = parse_quote! {
      pub struct LinearGradientBrush {
        #[ui(content)] pub gradient_stops: Vec<GradientStop>,
      }
    };

    let expanded = expand(&input);

    assert!(!expanded.contains("fn children") && !expanded.contains("fn child("));
  }
}
