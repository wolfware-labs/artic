use crate::crate_root::{REGISTRY, crate_root};
use crate::members::attributes::unknown_key;
use crate::members::emit::{Setter, checks, default_impl, key_constant, key_static, properties_static, setter, visits};
use crate::members::{Content, Kind, Members, PropertyField};
use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{DeriveInput, Ident, LitStr, Path, Visibility};

/// A struct read for `UiProperties`: its fields and the crate path its `#[ui(crate = "...")]`
/// names, if any.
struct GroupAnalysis<'a> {
  ident: &'a Ident,
  members: Members<'a>,
  crate_path: Option<Path>,
}

/// Reads a group's struct attribute and fields, and checks the names they generate.
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
  members.check_names([])?;
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
  let members = &analysis.members;
  let root = crate_root(analysis.crate_path.as_ref(), ident, REGISTRY)?;
  let keys = members.properties.iter().map(key_static);
  let default_impl = default_impl(ident, members, None);
  let properties = properties_static(members);
  let constants = members.properties.iter().filter_map(key_constant);
  let trait_name = format_ident!("{ident}Methods").to_string();
  let content = match members.content {
    Some(Content::Property(index)) => Some(
      quote!(const CONTENT: ::core::option::Option<&'static __artic::PropertyInfo> = ::core::option::Option::Some(&__PROPERTIES[#index]);),
    ),
    _ => None,
  };
  let visits = visits(members);
  let checks = checks(members, []);
  let methods = methods_trait(input, &members.properties, &root);
  Ok(quote! {
    const _: () = {
      use #root::__private as __artic;

      #(#keys)*

      #default_impl

      #properties

      impl #ident {
        #(#constants)*
      }

      impl __artic::PropertyGroup for #ident {
        const PROPERTIES: &'static [__artic::PropertyInfo] = &__PROPERTIES;
        const METHODS: &'static str = ::core::concat!(::core::module_path!(), "::", #trait_name);
        #content

        fn visit_properties(&self, visitor: &mut __artic::PropertyVisitor<'_>) {
          #visits
        }
      }

      #checks
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

  // A group's methods live in its trait and its key constants on the group, so a field whose
  // method and constant share a name is fine, while two constants of one name aren't.
  #[test]
  fn group_methods_and_constants_are_checked_apart() {
    let screaming: DeriveInput = parse_quote! { struct Sizing { #[ui] WIDTH: Property<f32> } };
    let twins: DeriveInput = parse_quote! { struct Sizing { #[ui] row_span: Property<f32>, #[ui] rowSpan: Property<f32> } };

    assert!(expand(&screaming).is_ok());
    assert_eq!(
      expand(&twins).expect_err("one constant name").to_string(),
      "`ROW_SPAN` would be both the key constant of the property `row_span` and the key constant of the property `rowSpan`; rename one"
    );
  }

  // A group's key constant named like `PropertyGroup`'s descriptors would hide them, so it's
  // rejected. `CONTENT` stays free: a `content` property's key is `CONTENT`, like any other key
  // constant, and the group's content is reached through the trait.
  #[test]
  fn group_constants_named_like_property_group_items_are_an_error() {
    let content: DeriveInput = parse_quote! { struct ContentState { #[ui(content)] content: Property<f32> } };

    assert!(expand(&content).is_ok());
    for (field, item) in [("properties", "PROPERTIES"), ("methods", "METHODS")] {
      let field = Ident::new(field, proc_macro2::Span::call_site());
      let input: DeriveInput = parse_quote! { struct Sizing { #[ui] #field: Property<f32> } };

      assert_eq!(
        expand(&input).expect_err("a hidden item").to_string(),
        format!("`{item}` would be both `PropertyGroup::{item}` and the key constant of the property `{field}`; rename one")
      );
    }
  }

  // The crate path a group names is the one its expansion reaches artic through.
  #[test]
  fn group_crate_path_is_used() {
    let input: DeriveInput = parse_quote! { #[ui(crate = "registry_alias")] struct Sizing { #[ui] gap: Property<f32> } };

    assert!(
      expand(&input)
        .expect("valid")
        .to_string()
        .contains("use registry_alias :: __private as __artic")
    );
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
      "`build` would be both the `build` of every type that flattens the group and the method of the property `build`; rename one"
    );
  }

  // `pub(self)` is private; a scope with another path is exposed only when written as the group's,
  // and a public field always is.
  #[test]
  fn visibility_scopes_compare_by_reach_or_text() {
    let vis = |tokens: &str| -> Visibility { syn::parse_str(tokens).expect("a visibility") };

    assert!(exposed(&vis("pub(self)"), &vis("")));
    assert!(exposed(&vis("pub(in crate::a)"), &vis("pub(in crate::a)")));
    assert!(!exposed(&vis("pub(in crate::a)"), &vis("pub(in crate::b)")));
    assert!(exposed(&vis("pub"), &vis("pub(in crate::a)")));
    assert!(!exposed(&vis(""), &vis("pub(in crate::a)")));
  }
}
