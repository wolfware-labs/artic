use crate::crate_root::{REGISTRY, crate_root};
use crate::ui_type::attributes::parse_field_attributes;
use crate::ui_type::builders::{Setter, key_constant, setter};
use crate::ui_type::fields::{Role, UiField, check_content, classify, reject_unmarked_member};
use crate::ui_type::generate::{default_field, key_static, property_info, value_check};
use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::ext::IdentExt;
use syn::{Data, DeriveInput, Error, Fields, Visibility};

/// Writes a property group: its keys and their public constants, `Default`, `PropertyGroup` with
/// its descriptors, and its methods trait.
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
  for field in &properties {
    let name = field.ident.unraw().to_string();
    if ["new", "build", "child", "children"].contains(&name.as_str()) {
      return Err(Error::new_spanned(
        field.ident,
        format_args!("a group field can't be named `{name}`: every type that flattens the group has its own `{name}`"),
      ));
    }
  }
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
    quote!(visit(__artic::Property::as_ref(&self.#field_ident));)
  });
  let count = properties.len();
  let methods = methods_trait(input, &properties, &root);
  let trait_name = format_ident!("{ident}Methods").to_string();
  let constants = properties.iter().filter_map(key_constant);
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

      impl #ident {
        #(#constants)*
      }

      impl __artic::PropertyGroup for #ident {
        const PROPERTIES: &'static [__artic::PropertyInfo] = &__PROPERTIES;
        const METHODS: &'static str = ::core::concat!(::core::module_path!(), "::", #trait_name);
        #content

        fn visit_properties(&self, visit: &mut dyn ::core::ops::FnMut(__artic::AnyProperty<'_>)) {
          #(#visits)*
        }
      }

      #[allow(dead_code, clippy::needless_borrow)]
      fn __checks() {
        use __artic::{ViaChild as _, ViaFromStr as _, ViaUiType as _};
        #(#checks)*
      }
    };

    #methods
  })
}

/// The group's methods trait: one provided method per field the trait can expose, reaching the
/// group through `HasGroup`, and a blanket impl for every type that holds the group. It is
/// written beside the group, outside the derive's anonymous block, so it can be named.
fn methods_trait(input: &DeriveInput, properties: &[UiField], root: &TokenStream) -> TokenStream {
  let (ident, vis) = (&input.ident, &input.vis);
  let name = format_ident!("{ident}Methods");
  let doc = format!("The builder methods of the `{ident}` group, on every type that flattens it.");
  let methods = properties.iter().filter(|field| exposed(field.vis, vis)).filter_map(|field| {
    let Setter { parameter, stored, doc } = setter(field)?;
    let field_ident = field.ident;
    Some(quote! {
      #[doc = #doc]
      #[must_use]
      fn #field_ident(self, value: #parameter) -> Self {
        <Self as #root::__private::HasGroup<#ident>>::group(&self).#field_ident.set(#stored);
        self
      }
    })
  });
  quote! {
    #[doc = #doc]
    #vis trait #name: #root::__private::HasGroup<#ident> + ::core::marker::Sized {
      #(#methods)*
    }

    impl<T: #root::__private::HasGroup<#ident>> #name for T {}
  }
}

/// How far a visibility reaches, for the scopes every module has: private or `self`, the parent
/// module, the crate, or everywhere. `None` for `pub(in path)` with any other path.
fn reach(vis: &Visibility) -> Option<u8> {
  match vis {
    Visibility::Inherited => Some(0),
    Visibility::Public(_) => Some(3),
    Visibility::Restricted(restricted) => match restricted.path.get_ident()?.to_string().as_str() {
      "self" => Some(0),
      "super" => Some(1),
      "crate" => Some(2),
      _ => None,
    },
  }
}

/// Whether a field is at least as visible as its group, so a method of the group's trait, which
/// is as visible as the group, exposes nothing the field hides. A scope with another path counts
/// only when it is written the same as the group's.
fn exposed(field: &Visibility, group: &Visibility) -> bool {
  match (reach(field), reach(group)) {
    (Some(field), Some(group)) => field >= group,
    _ => matches!(field, Visibility::Public(_)) || field.to_token_stream().to_string() == group.to_token_stream().to_string(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use syn::parse_quote;

  /// The methods trait for `input`, pretty-printed.
  fn expand_trait(input: &DeriveInput) -> String {
    let properties: Vec<UiField> = match &input.data {
      Data::Struct(data) => data
        .fields
        .iter()
        .map(|field| {
          let attributes = parse_field_attributes(field).expect("valid").expect("marked");
          classify(field, field.ident.as_ref().expect("named"), attributes).expect("a property")
        })
        .collect(),
      _ => unreachable!("a struct"),
    };
    let file: syn::File = syn::parse2(methods_trait(input, &properties, &quote!(::artic_registry))).expect("parses");
    prettyplease::unparse(&file)
  }

  // A public group's trait has a method per public field, through `HasGroup`, and a blanket impl.
  #[test]
  fn layout_methods_trait() {
    let input: DeriveInput = parse_quote! {
      pub struct Layout {
        #[ui] pub width: Property<Option<f32>>,
        #[ui] pub margin: Property<Thickness>,
      }
    };

    insta::assert_snapshot!(expand_trait(&input));
  }

  // A private field of a public group gets no method: a trait method is as public as its trait.
  #[test]
  fn private_group_field_gets_no_method() {
    let input: DeriveInput = parse_quote! {
      pub struct Layout {
        #[ui] pub width: Property<f32>,
        #[ui] cache: Property<f32>,
      }
    };

    let expanded = expand_trait(&input);

    assert!(expanded.contains("fn width(") && !expanded.contains("fn cache("));
  }

  // A field at least as visible as its group gets a method however its scope is spelled, and a
  // field more private than the group doesn't.
  #[test]
  fn restricted_scopes_compare_by_reach() {
    let input: DeriveInput = parse_quote! {
      pub(super) struct Layout {
        #[ui] pub(crate) wide: Property<f32>,
        #[ui] pub(in super) same: Property<f32>,
        #[ui] pub(self) narrow: Property<f32>,
      }
    };

    let expanded = expand_trait(&input);

    assert!(expanded.contains("fn wide(") && expanded.contains("fn same("));
    assert!(!expanded.contains("fn narrow("));
  }

  // A group field named like a builder method every type has is rejected.
  #[test]
  fn group_fields_with_builder_names_are_an_error() {
    let input: DeriveInput = parse_quote! { struct Sizing { #[ui] build: Property<f32> } };

    let error = expand(&input).expect_err("a reserved name").to_string();

    assert_eq!(
      error,
      "a group field can't be named `build`: every type that flattens the group has its own `build`"
    );
  }
}
