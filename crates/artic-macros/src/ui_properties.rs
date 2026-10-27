use crate::crate_root::{REGISTRY, crate_root};
use crate::members::attributes::unknown_key;
use crate::members::{Content, Kind, Members, PropertyField};
use crate::ui_type::builders::{Setter, key_constant, setter};
use crate::ui_type::generate::{default_field, key_static, property_info, value_check};
use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::ext::IdentExt;
use syn::{DeriveInput, Error, Ident, LitStr, Path, Visibility};

/// A struct read for `UiProperties`: its fields and the crate path its `#[ui(crate = "...")]`
/// names, if any.
struct GroupAnalysis<'a> {
  ident: &'a Ident,
  members: Members<'a>,
  crate_path: Option<Path>,
}

/// Reads a group's struct attribute and fields, and rejects a field named like a builder method.
fn analyze(input: &DeriveInput) -> syn::Result<GroupAnalysis<'_>> {
  let mut crate_path = None;
  for attribute in input.attrs.iter().filter(|attribute| attribute.path().is_ident("ui")) {
    attribute.parse_nested_meta(|meta| {
      if !meta.path.is_ident("crate") {
        return Err(unknown_key(&meta, "`crate`"));
      }
      let value: LitStr = meta.value()?.parse()?;
      crate_path = Some(value.parse::<Path>()?);
      Ok(())
    })?;
  }
  let members = Members::analyze(input, Kind::Group)?;
  for field in &members.properties {
    let name = field.ident.unraw().to_string();
    if ["new", "build", "child", "children"].contains(&name.as_str()) {
      return Err(Error::new_spanned(
        field.ident,
        format_args!("a group field can't be named `{name}`: every type that flattens the group has its own `{name}`"),
      ));
    }
  }
  Ok(GroupAnalysis {
    ident: &input.ident,
    members,
    crate_path,
  })
}

/// Writes a property group: its keys and their public constants, `Default`, `PropertyGroup` with
/// its descriptors, and its methods trait.
pub fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
  let analysis = analyze(input)?;
  let ident = analysis.ident;
  let properties = &analysis.members.properties;
  let others = &analysis.members.others;
  let content = match analysis.members.content {
    Some(Content::Property(index)) => Some(
      quote!(const CONTENT: ::core::option::Option<&'static __artic::PropertyInfo> = ::core::option::Option::Some(&__PROPERTIES[#index]);),
    ),
    _ => None,
  };
  let root = crate_root(analysis.crate_path.as_ref(), ident, REGISTRY)?;
  let keys = properties.iter().map(key_static);
  let defaults = properties.iter().map(default_field);
  let infos = properties.iter().map(property_info);
  let checks = properties.iter().map(|field| value_check(field.value));
  let visits = properties.iter().map(|field| {
    let field_ident = field.ident;
    quote!(visitor.visit(&self.#field_ident);)
  });
  let count = properties.len();
  let methods = methods_trait(input, properties, &root);
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

        fn visit_properties(&self, visitor: &mut __artic::PropertyVisitor<'_>) {
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
fn methods_trait(input: &DeriveInput, properties: &[PropertyField], root: &TokenStream) -> TokenStream {
  let (ident, vis) = (&input.ident, &input.vis);
  let name = format_ident!("{ident}Methods");
  let doc = format!("The builder methods of the `{ident}` group, on every type that flattens it.");
  let methods = properties.iter().filter(|field| exposed(field.vis, vis)).map(|field| {
    let Setter { parameter, stored, doc } = setter(field);
    let field_ident = field.ident;
    quote! {
      #[doc = #doc]
      #[must_use]
      fn #field_ident(self, value: #parameter) -> Self {
        <Self as #root::__private::HasGroup<#ident>>::group(&self).#field_ident.set(#stored);
        self
      }
    }
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
    let members = Members::analyze(input, Kind::Group).expect("a valid group");
    let file: syn::File = syn::parse2(methods_trait(input, &members.properties, &quote!(::artic_registry))).expect("parses");
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

  // A group expanded whole: the snapshot pins everything the derive writes.
  #[test]
  fn whole_group_expansion() {
    let input: DeriveInput = parse_quote! {
      pub struct Layout {
        #[ui(content)] pub width: Property<f32>,
        #[ui(affects = layout)] pub margin: Property<Thickness>,
        cache: u8,
      }
    };

    let file: syn::File = syn::parse2(expand(&input).expect("valid")).expect("parses");
    insta::assert_snapshot!(prettyplease::unparse(&file));
  }

  // A group's struct takes `crate` and nothing else in `#[ui(...)]`.
  #[test]
  fn group_struct_keys_other_than_crate_are_an_error() {
    let input: DeriveInput = parse_quote! { #[ui(whitespace = "collapse")] struct Sizing { #[ui] width: Property<f32> } };

    let error = expand(&input).expect_err("an unknown key").to_string();

    assert_eq!(error, "unknown `ui` key `whitespace`; expected `crate`");
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
