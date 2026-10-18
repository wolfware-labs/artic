use crate::crate_root::{REGISTRY, crate_root};
use crate::ui_type::attributes::{Affects, Attached, Inherits, Target, Whitespace};
use crate::ui_type::fields::{Analysis, Role, UiField, generic_argument, snake_case, ui_name};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::Ident;
use syn::spanned::Spanned;

/// Writes the code for one derived struct: its property keys, `Default`, `Object` and `UiType`
/// impls, its registry descriptors and entry, and the checks on its field types.
pub fn generate(analysis: &Analysis) -> syn::Result<TokenStream> {
  let root = crate_root(analysis.attributes.crate_path.as_ref(), analysis.ident, REGISTRY)?;
  let ident = analysis.ident;
  let properties: Vec<&UiField> = analysis.ui_fields.iter().filter(|f| is_property(f)).collect();
  let events: Vec<&UiField> = analysis
    .ui_fields
    .iter()
    .filter(|f| matches!(f.role, Role::Event { .. }))
    .collect();
  let collections: Vec<&UiField> = analysis
    .ui_fields
    .iter()
    .filter(|f| matches!(f.role, Role::Collection))
    .collect();
  let groups: Vec<&UiField> = analysis.ui_fields.iter().filter(|f| matches!(f.role, Role::Group)).collect();

  let keys = properties.iter().map(|field| key_static(field));
  let default_fields = analysis.ui_fields.iter().map(|field| default_field(field));
  let other_fields = &analysis.other_fields;
  let visits = properties.iter().map(|field| {
    let field_ident = field.ident;
    quote!(visit(&self.#field_ident);)
  });
  let group_visits = groups.iter().map(|field| {
    let field_ident = field.ident;
    quote!(__artic::PropertyGroup::visit_properties(&self.#field_ident, visit);)
  });
  let group_infos = groups.iter().map(|field| {
    let ty = field.ty;
    let field_name = field.ident.to_string();
    quote!(__artic::PropertyGroupInfo { field: #field_name, properties: <#ty as __artic::PropertyGroup>::PROPERTIES })
  });
  let group_checks = groups.iter().map(|field| {
    let ty = field.ty;
    quote_spanned!(ty.span()=> __artic::assert_property_group::<#ty>();)
  });
  let group_count = groups.len();
  let property_infos = properties.iter().map(|field| property_info(field));
  let event_infos = events.iter().map(|field| {
    let name = ui_name(field);
    let Role::Event { args } = field.role else {
      unreachable!("filtered to events")
    };
    quote!(__artic::EventInfo { name: #name, args_type: ::core::stringify!(#args) })
  });
  let property_count = properties.len();
  let event_count = events.len();
  let value_checks = properties.iter().map(|field| value_check(field));
  let collection_checks = collections.iter().map(|field| {
    let ty = field.ty;
    quote_spanned!(ty.span()=> __artic::assert_collection::<#ty>();)
  });
  let own_content = analysis
    .ui_fields
    .iter()
    .find(|field| field.attributes.content.is_some())
    .map_or_else(
      || quote!(::core::option::Option::None),
      |field| {
        let name = field.ident.to_string();
        match properties.iter().position(|property| property.ident == field.ident) {
          Some(index) => quote!(::core::option::Option::Some(__artic::ContentInfo::Property(__artic::PropertyRef {
            group: ::core::option::Option::None,
            info: &__PROPERTIES[#index],
          }))),
          None => quote!(::core::option::Option::Some(__artic::ContentInfo::Collection {
            field: #name,
            add_child: __add_child,
          })),
        }
      },
    );
  let content_functions = analysis
    .ui_fields
    .iter()
    .find(|field| field.attributes.content.is_some() && matches!(field.role, Role::Collection))
    .map(|field| {
      let (field_ident, ty) = (field.ident, field.ty);
      quote! {
        fn __content_field<'a>(
          tree: &'a mut __artic::UiTree,
          owner: __artic::NodeId,
        ) -> ::core::option::Option<&'a mut #ty> {
          tree.get_as_mut::<#ident>(owner).map(|node| &mut node.#field_ident)
        }

        fn __add_child(
          tree: &mut __artic::UiTree,
          owner: __artic::NodeId,
          child: __artic::NodeId,
        ) -> ::core::result::Result<(), __artic::ValueError> {
          <#ty as __artic::UiCollection>::push_child(tree, owner, child, __content_field)
        }
      }
    });
  let group_contents = groups.iter().map(|field| {
    let ty = field.ty;
    let field_name = field.ident.to_string();
    quote!(__artic::group_content(#field_name, <#ty as __artic::PropertyGroup>::CONTENT))
  });
  let content = quote!(__artic::one_content(&[#own_content, #(#group_contents),*]));
  let is_collection = !collections.is_empty();
  let attached = &analysis.attributes.attached;
  let attached_keys = attached.iter().map(attached_key);
  let attached_helpers = attached.iter().map(|attached| attached_helpers(ident, attached));
  let attached_infos = attached.iter().map(attached_info);
  let attached_count = attached.len();
  let value_provider = value_provider(analysis, &properties);
  let markup_extension_info = if analysis.attributes.value_provider.is_some() {
    quote!(::core::option::Option::Some(&__VALUE_PROVIDER))
  } else {
    quote!(::core::option::Option::None)
  };
  let attached_checks = attached.iter().map(|attached| {
    let ty = &attached.ty;
    let value = generic_argument(ty, "Option").unwrap_or(ty);
    quote_spanned!(ty.span()=>
      (&&__artic::ValueCheck::<#value>(__artic::PhantomData)).value_must_implement_fromstr_or_derive_uitype();
    )
  });
  let override_defaults = analysis
    .attributes
    .override_defaults
    .then(|| quote!(<Self as __artic::OverrideDefaults>::override_defaults(&__artic_value);));
  let whitespace = match analysis.attributes.whitespace {
    Whitespace::Collapse => quote!(__artic::Whitespace::Collapse),
    Whitespace::Significant => quote!(__artic::Whitespace::Significant),
  };

  Ok(quote! {
    const _: () = {
      use #root::__private as __artic;

      #(#keys)*

      impl ::core::default::Default for #ident {
        fn default() -> Self {
          let __artic_value = Self {
            #(#default_fields,)*
            #(#other_fields: ::core::default::Default::default(),)*
          };
          #override_defaults
          __artic_value
        }
      }

      impl __artic::Object for #ident {
        fn visit_properties(&self, visit: &mut dyn ::core::ops::FnMut(&dyn __artic::AnyProperty)) {
          #(#visits)*
          #(#group_visits)*
        }
      }

      static __PROPERTIES: [__artic::PropertyInfo; #property_count] = [#(#property_infos),*];
      static __EVENTS: [__artic::EventInfo; #event_count] = [#(#event_infos),*];
      static __ATTACHED: [__artic::AttachedPropertyInfo; #attached_count] = [#(#attached_infos),*];
      static __GROUPS: [__artic::PropertyGroupInfo; #group_count] = [#(#group_infos),*];

      #(#attached_keys)*

      #value_provider

      #content_functions

      impl #ident {
        #(#attached_helpers)*
      }

      fn __new(tree: &mut __artic::UiTree) -> __artic::NodeId {
        tree.alloc(::std::boxed::Box::new(<#ident as ::core::default::Default>::default()))
      }

      static __TYPE_INFO: __artic::TypeInfo = __artic::TypeInfo {
        name: ::core::stringify!(#ident),
        rust_path: ::core::concat!(::core::module_path!(), "::", ::core::stringify!(#ident)),
        new: __new,
        content: #content,
        is_collection: #is_collection,
        properties: &__PROPERTIES,
        groups: &__GROUPS,
        events: &__EVENTS,
        attached_properties: &__ATTACHED,
        value_provider: #markup_extension_info,
        whitespace: #whitespace,
      };

      impl __artic::UiType for #ident {
        const TYPE_INFO: &'static __artic::TypeInfo = &__TYPE_INFO;
      }

      __artic::register_type!(__TYPE_INFO);

      #[allow(dead_code, clippy::needless_borrow)]
      fn __checks() {
        use __artic::{ViaChild as _, ViaFromStr as _, ViaUiType as _};
        #(#value_checks)*
        #(#collection_checks)*
        #(#group_checks)*
        #(#attached_checks)*
      }
    };
  })
}

/// The `ValueProviderInfo` static of a markup extension, or nothing for another type. Every
/// property is a named argument, and `positional(...)` lists those also set by position.
fn value_provider(analysis: &Analysis, properties: &[&UiField]) -> TokenStream {
  let Some(positional) = &analysis.attributes.value_provider else {
    return TokenStream::new();
  };
  let by_position: Vec<&UiField> = positional
    .iter()
    .map(|argument| {
      *properties
        .iter()
        .find(|field| field.ident == argument)
        .expect("checked to name a property")
    })
    .collect();
  let positional = by_position.iter().map(|field| arg_info(field));
  let named = properties.iter().map(|field| arg_info(field));
  let (positional_count, named_count) = (by_position.len(), properties.len());
  quote! {
    static __POSITIONAL: [__artic::ArgInfo; #positional_count] = [#(#positional),*];
    static __NAMED: [__artic::ArgInfo; #named_count] = [#(#named),*];
    static __VALUE_PROVIDER: __artic::ValueProviderInfo = __artic::ValueProviderInfo {
      positional: &__POSITIONAL,
      named: &__NAMED,
    };
  }
}

/// The `ArgInfo` of a property used as a markup extension argument: its name and value type, as in
/// its `PropertyInfo`.
fn arg_info(field: &UiField) -> TokenStream {
  let Role::Property { inner, .. } = field.role else {
    unreachable!("properties only")
  };
  let key = key_path(field);
  quote!(__artic::ArgInfo { name: #key.name(), value_type: ::core::stringify!(#inner) })
}

/// The static an attached property's key lives in, such as `__ATTACHED_COLUMN_SPAN`.
fn attached_static(attached: &Attached) -> Ident {
  format_ident!("__ATTACHED_{}", snake_case(&attached.name.value()).to_uppercase())
}

/// The `static AttachedKey` of one attached property.
fn attached_key(attached: &Attached) -> TokenStream {
  let key = attached_static(attached);
  let Attached {
    name,
    ty,
    default,
    affects,
    inherits,
    ..
  } = attached;
  let default = match default {
    Some(expression) => quote!(#expression),
    None => quote_spanned!(ty.span()=> <#ty as ::core::default::Default>::default()),
  };
  let affects = affects_tokens(*affects);
  let inheritance = inheritance_tokens(inherits.as_ref());
  quote! {
    static #key: __artic::AttachedKey<#ty> = __artic::AttachedKey::new(#name, #affects, #inheritance, || #default);
  }
}

/// The key constant and the `set_<name>` and `<name>` helpers of one attached property.
fn attached_helpers(owner: &Ident, attached: &Attached) -> TokenStream {
  let key = attached_static(attached);
  let ty = &attached.ty;
  let name = attached.name.value();
  let snake = snake_case(&name);
  let constant = format_ident!("{}", snake.to_uppercase());
  let getter = format_ident!("{snake}");
  let setter = format_ident!("set_{snake}");
  let value = format_ident!("{snake}");
  let property = format!("`{owner}.{name}`");
  let constant_doc = format!("The {property} attached property.");
  let setter_doc = format!("Sets {property} on `node`, at local precedence.");
  let getter_doc = format!("Returns {property} of `node`, or its default when nothing set it.");
  quote! {
    #[doc = #constant_doc]
    pub const #constant: &'static __artic::AttachedKey<#ty> = &#key;

    #[doc = #setter_doc]
    pub fn #setter(tree: &mut __artic::UiTree, node: impl ::core::convert::Into<__artic::NodeId>, #value: #ty) {
      tree.attached_property(node.into(), Self::#constant).set(#value);
    }

    #[doc = #getter_doc]
    #[must_use]
    pub fn #getter(tree: &__artic::UiTree, node: impl ::core::convert::Into<__artic::NodeId>) -> #ty {
      tree.attached(node.into(), Self::#constant)
    }
  }
}

/// The `AttachedPropertyInfo` of one attached property.
fn attached_info(attached: &Attached) -> TokenStream {
  let Attached { name, ty, target, .. } = attached;
  let target = match target {
    Target::Any => quote!(__artic::AttachedTarget::Any),
    Target::Children => quote!(__artic::AttachedTarget::ChildrenOfOwner),
  };
  quote! {
    __artic::AttachedPropertyInfo {
      name: #name,
      value_type: ::core::stringify!(#ty),
      target: #target,
    }
  }
}

/// The check that a property's value type can be written in markup, pointing at the type.
pub fn value_check(field: &UiField) -> TokenStream {
  let Role::Property { value, .. } = field.role else {
    unreachable!("properties only")
  };
  quote_spanned!(value.span()=>
    (&&__artic::ValueCheck::<#value>(__artic::PhantomData)).value_must_implement_fromstr_or_derive_uitype();
  )
}

fn is_property(field: &UiField) -> bool {
  matches!(field.role, Role::Property { .. })
}

/// The key a property is stored under, as a `&'static PropertyKey` expression usable in const
/// code: the derive's own static, or a shared key's `key()`, which a property key and an attached
/// key both have.
fn key_path(field: &UiField) -> TokenStream {
  match &field.attributes.key {
    Some(key) => quote!(#key.key()),
    None => {
      let key = generated_key(field.ident);
      quote!((&#key))
    }
  }
}

fn generated_key(field: &Ident) -> Ident {
  format_ident!("__KEY_{}", field.to_string().to_uppercase())
}

/// The `static PropertyKey` the derive writes for a property without a shared `key`.
pub fn key_static(field: &UiField) -> TokenStream {
  if field.attributes.key.is_some() {
    return TokenStream::new();
  }
  let Role::Property { inner, .. } = field.role else {
    unreachable!("properties only")
  };
  let key = generated_key(field.ident);
  let name = ui_name(field);
  let affects = affects_tokens(field.attributes.affects.unwrap_or_default());
  let inheritance = inheritance_tokens(field.attributes.inherits.as_ref());
  quote! {
    static #key: __artic::PropertyKey<#inner> = __artic::PropertyKey::new(#name, #affects, #inheritance);
  }
}

fn affects_tokens(affects: Affects) -> TokenStream {
  let stages: Vec<TokenStream> = [
    (affects.layout, quote!(LAYOUT)),
    (affects.render, quote!(RENDER)),
    (affects.surface, quote!(SURFACE)),
  ]
  .into_iter()
  .filter(|(listed, _)| *listed)
  .map(|(_, stage)| quote!(__artic::Affects::#stage))
  .collect();
  match stages.split_first() {
    None => quote!(__artic::Affects::NONE),
    Some((first, rest)) => quote!(#first #(.union(#rest))*),
  }
}

fn inheritance_tokens(inherits: Option<&Inherits>) -> TokenStream {
  match inherits {
    None => quote!(__artic::Inheritance::None),
    Some(Inherits::Replace) => quote!(__artic::Inheritance::Replace),
    Some(Inherits::Combine(path)) => match path.get_ident() {
      Some(name) => quote_spanned!(path.span()=> __artic::Inheritance::Combine(__artic::inherit::#name)),
      None => quote_spanned!(path.span()=> __artic::Inheritance::Combine(#path)),
    },
  }
}

/// How `Default` creates one `#[ui]` field.
pub fn default_field(field: &UiField) -> TokenStream {
  let field_ident = field.ident;
  match field.role {
    Role::Property { inner, .. } => {
      let key = key_path(field);
      let value = match (&field.attributes.key, &field.attributes.default) {
        (Some(shared), Some(expression)) => {
          quote_spanned!(expression.span()=> __artic::default_beside(&#shared, #expression))
        }
        (Some(shared), None) => quote!({
          use __artic::SharedKey as _;
          #shared.initial_value()
        }),
        (None, Some(expression)) => quote!(#expression),
        (None, None) => quote_spanned!(inner.span()=> <#inner as ::core::default::Default>::default()),
      };
      quote!(#field_ident: __artic::Property::new(#key, #value))
    }
    Role::Event { .. } | Role::Collection | Role::Group => quote!(#field_ident: ::core::default::Default::default()),
  }
}

/// The `PropertyInfo` for one property, with its metadata read from the key.
pub fn property_info(field: &UiField) -> TokenStream {
  let Role::Property { inner, .. } = field.role else {
    unreachable!("properties only")
  };
  let key = key_path(field);
  let field_name = field.ident.to_string();
  quote! {
    __artic::PropertyInfo {
      name: #key.name(),
      field: #field_name,
      value_type: ::core::stringify!(#inner),
      affects: #key.affects(),
      inherits: #key.inherits(),
    }
  }
}
