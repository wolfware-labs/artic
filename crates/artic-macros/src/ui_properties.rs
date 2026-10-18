use crate::crate_root::{REGISTRY, crate_root};
use crate::ui_type::attributes::parse_field_attributes;
use crate::ui_type::fields::{Role, check_content, classify, reject_unmarked_member};
use crate::ui_type::generate::{default_field, key_static, property_info, value_check};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields};

/// Writes a property group: its keys, `Default`, and `PropertyGroup` with its descriptors.
pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
  let ident = &input.ident;
  let Data::Struct(data) = &input.data else {
    return Err(Error::new_spanned(ident, "`UiProperties` can only be derived for a struct"));
  };
  let Fields::Named(fields) = &data.fields else {
    return Err(Error::new_spanned(
      &data.fields,
      "`UiProperties` needs a struct with named fields",
    ));
  };
  if !input.generics.params.is_empty() {
    return Err(Error::new_spanned(
      &input.generics,
      "`UiProperties` can't be derived for a generic struct",
    ));
  }
  if let Some(attribute) = input.attrs.iter().find(|attribute| attribute.path().is_ident("ui")) {
    return Err(Error::new_spanned(
      attribute,
      "a property group takes no `#[ui(...)]` on the struct",
    ));
  }
  let mut properties = Vec::new();
  let mut others = Vec::new();
  for field in &fields.named {
    let field_ident = field.ident.as_ref().expect("named fields have names");
    match parse_field_attributes(field)? {
      Some(attributes) => {
        let classified = classify(field, field_ident, attributes)?;
        if !matches!(classified.role, Role::Property { .. }) {
          return Err(Error::new_spanned(&field.ty, "a property group holds only `Property` fields"));
        }
        properties.push(classified);
      }
      None => {
        reject_unmarked_member(field)?;
        others.push(field_ident);
      }
    }
  }
  check_content(&properties)?;
  let content = properties
    .iter()
    .position(|field| field.attributes.content.is_some())
    .map(|index| quote!(const CONTENT: ::core::option::Option<&'static __artic::PropertyInfo> = ::core::option::Option::Some(&__PROPERTIES[#index]);));
  let root = crate_root(None, ident, REGISTRY)?;
  let keys = properties.iter().map(key_static);
  let defaults = properties.iter().map(default_field);
  let infos = properties.iter().map(property_info);
  let checks = properties.iter().map(value_check);
  let visits = properties.iter().map(|field| {
    let field_ident = field.ident;
    quote!(visit(&self.#field_ident);)
  });
  let count = properties.len();
  Ok(quote! {
    const _: () = {
      use #root::__private as __artic;

      #(#keys)*

      impl ::core::default::Default for #ident {
        fn default() -> Self {
          Self {
            #(#defaults,)*
            #(#others: ::core::default::Default::default(),)*
          }
        }
      }

      static __PROPERTIES: [__artic::PropertyInfo; #count] = [#(#infos),*];

      impl __artic::PropertyGroup for #ident {
        const PROPERTIES: &'static [__artic::PropertyInfo] = &__PROPERTIES;
        #content

        fn visit_properties(&self, visit: &mut dyn ::core::ops::FnMut(&dyn __artic::AnyProperty)) {
          #(#visits)*
        }
      }

      #[allow(dead_code, clippy::needless_borrow)]
      fn __checks() {
        use __artic::{ViaChild as _, ViaFromStr as _, ViaUiType as _};
        #(#checks)*
      }
    };
  })
}
