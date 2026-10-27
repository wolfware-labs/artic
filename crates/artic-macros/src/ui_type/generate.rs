use crate::crate_root::{REGISTRY, crate_root};
use crate::members::emit::{
  affects_tokens, checks, default_impl, inheritance_tokens, key_path, key_static, properties_static, value_check, visits,
};
use crate::members::{Content, Members, PropertyField, value_type};
use crate::ui_type::analysis::TypeAnalysis;
use crate::ui_type::attributes::{Attached, Target, Whitespace};
use crate::ui_type::builders::builders;
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::Ident;
use syn::ext::IdentExt;
use syn::spanned::Spanned;

/// Writes the code for one derived struct: its property keys, `Default`, `Object` and `UiType`
/// impls, its registry descriptors and entry, and the checks on its field types.
pub fn generate(analysis: &TypeAnalysis) -> syn::Result<TokenStream> {
  let root = crate_root(analysis.attributes.crate_path.as_ref(), analysis.ident, REGISTRY)?;
  let ident = analysis.ident;
  let members = &analysis.members;
  let attached = &analysis.attributes.attached;
  let keys = members.properties.iter().map(key_static);
  let override_defaults = analysis
    .attributes
    .override_defaults
    .then(|| quote!(<Self as __artic::OverrideDefaults>::override_defaults(&__artic_value);));
  let default_impl = default_impl(ident, members, override_defaults);
  let visits = visits(members);
  let group_items = group_items(ident, members);
  let descriptors = descriptors(members, attached);
  let attached_keys = attached.iter().map(attached_key);
  let value_provider = value_provider(analysis);
  let content_functions = content_functions(ident, members);
  let builders = builders(analysis);
  let attached_helpers = attached.iter().map(|attached| attached_helpers(ident, attached));
  let type_info = type_info(analysis);
  let checks = checks(members, attached.iter().map(|attached| value_check(value_type(&attached.ty))));
  Ok(quote! {
    const _: () = {
      use #root::__private as __artic;

      #(#keys)*

      #default_impl

      impl __artic::Object for #ident {
        fn visit_properties(&self, visitor: &mut __artic::PropertyVisitor<'_>) {
          #visits
        }
      }

      #group_items

      #descriptors

      #(#attached_keys)*

      #value_provider

      #content_functions

      impl #ident {
        #builders

        #(#attached_helpers)*
      }

      #type_info

      #checks
    };
  })
}

/// For each flattened group: its `HasGroup` impl, and the const checks that no property of the
/// type and no other group has a field of one of its names, whose methods would clash.
fn group_items(ident: &Ident, members: &Members) -> TokenStream {
  let groups = &members.groups;
  let impls = groups.iter().map(|group| {
    let (ty, field_ident) = (group.ty, group.ident);
    quote! {
      impl __artic::HasGroup<#ty> for #ident {
        fn group(&self) -> &#ty {
          &self.#field_ident
        }
      }
    }
  });
  let own_field_checks = members.properties.iter().flat_map(|field| {
    let field_name = field.ident.to_string();
    let message = format!(
      "the property `{}` has the name of a field of a group this type flattens; its method would hide the group's",
      field.ident.unraw()
    );
    groups.iter().map(move |group| {
      let ty = group.ty;
      quote!(::core::assert!(!__artic::group_has_field(<#ty as __artic::PropertyGroup>::PROPERTIES, #field_name), #message);)
    })
  });
  let pair_checks = groups.iter().enumerate().flat_map(|(index, first)| {
    groups[index + 1..].iter().map(move |second| {
      let (a, b) = (first.ty, second.ty);
      let message = format!(
        "the groups in `{}` and `{}` have a field of one name; their methods would clash",
        first.ident, second.ident
      );
      quote!(::core::assert!(!__artic::groups_share_field(<#a as __artic::PropertyGroup>::PROPERTIES, <#b as __artic::PropertyGroup>::PROPERTIES), #message);)
    })
  });
  quote! {
    #(#impls)*

    const _: () = {
      #(#own_field_checks)*
      #(#pair_checks)*
    };
  }
}

/// The descriptor statics the type's `TypeInfo` points at: its properties, events, attached
/// properties and groups.
fn descriptors(members: &Members, attached: &[Attached]) -> TokenStream {
  let properties = properties_static(members);
  let events = members.events.iter().map(|event| {
    let (name, args) = (&event.ui_name, event.args);
    quote!(__artic::EventInfo { name: #name, args_type: ::core::stringify!(#args) })
  });
  let attached_infos = attached.iter().map(attached_info);
  let groups = members.groups.iter().map(|group| {
    let ty = group.ty;
    let field_name = group.ident.to_string();
    quote!(__artic::PropertyGroupInfo {
      field: #field_name,
      properties: <#ty as __artic::PropertyGroup>::PROPERTIES,
      methods: <#ty as __artic::PropertyGroup>::METHODS,
    })
  });
  let (event_count, attached_count, group_count) = (members.events.len(), attached.len(), members.groups.len());
  quote! {
    #properties
    static __EVENTS: [__artic::EventInfo; #event_count] = [#(#events),*];
    static __ATTACHED: [__artic::AttachedPropertyInfo; #attached_count] = [#(#attached_infos),*];
    static __GROUPS: [__artic::PropertyGroupInfo; #group_count] = [#(#groups),*];
  }
}

/// For a type whose content is a collection, `__content_field`, which reaches the field, and
/// `__add_child`, which its `ContentInfo` calls to add a child to it.
fn content_functions(ident: &Ident, members: &Members) -> Option<TokenStream> {
  let Some(Content::Collection {
    ident: field_ident, ty, ..
  }) = &members.content
  else {
    return None;
  };
  Some(quote! {
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
  })
}

/// The type's content as `one_content` resolves it at compile time: its own content member, if
/// any, and each flattened group's.
fn content_info(members: &Members) -> TokenStream {
  let own = match &members.content {
    None => quote!(::core::option::Option::None),
    Some(Content::Property(index)) => quote!(::core::option::Option::Some(__artic::ContentInfo::Property(__artic::PropertyRef {
      group: ::core::option::Option::None,
      info: &__PROPERTIES[#index],
    }))),
    Some(Content::Collection { ident, method, .. }) => {
      let name = ident.to_string();
      let method = match method {
        Some(method) => {
          let variant = method.variant();
          quote!(::core::option::Option::Some(__artic::ContentMethod::#variant))
        }
        None => quote!(::core::option::Option::None),
      };
      quote!(::core::option::Option::Some(__artic::ContentInfo::Collection {
        field: #name,
        method: #method,
        add_child: __add_child,
      }))
    }
  };
  let groups = members.groups.iter().map(|group| {
    let ty = group.ty;
    let field_name = group.ident.to_string();
    quote!(__artic::group_content(#field_name, <#ty as __artic::PropertyGroup>::CONTENT))
  });
  quote!(__artic::one_content(&[#own, #(#groups),*]))
}

/// The type's `TypeInfo`, its `UiType` impl and its registry entry.
fn type_info(analysis: &TypeAnalysis) -> TokenStream {
  let ident = analysis.ident;
  let content = content_info(&analysis.members);
  let value_provider = if analysis.attributes.value_provider.is_some() {
    quote!(::core::option::Option::Some(&__VALUE_PROVIDER))
  } else {
    quote!(::core::option::Option::None)
  };
  let whitespace = match analysis.attributes.whitespace {
    Whitespace::Collapse => quote!(__artic::Whitespace::Collapse),
    Whitespace::Significant => quote!(__artic::Whitespace::Significant),
  };
  quote! {
    fn __new(tree: &mut __artic::UiTree) -> __artic::NodeId {
      tree.alloc(::std::boxed::Box::new(<#ident as ::core::default::Default>::default()))
    }

    static __TYPE_INFO: __artic::TypeInfo = __artic::TypeInfo {
      name: ::core::stringify!(#ident),
      rust_path: ::core::concat!(::core::module_path!(), "::", ::core::stringify!(#ident)),
      new: __new,
      content: #content,
      properties: &__PROPERTIES,
      groups: &__GROUPS,
      events: &__EVENTS,
      attached_properties: &__ATTACHED,
      value_provider: #value_provider,
      whitespace: #whitespace,
    };

    impl __artic::UiType for #ident {
      const TYPE_INFO: &'static __artic::TypeInfo = &__TYPE_INFO;
    }

    __artic::register_type!(__TYPE_INFO);
  }
}

/// The `ValueProviderInfo` static of a value provider, or nothing for another type. Every
/// property is a named argument, and `positional(...)` lists those also set by position.
fn value_provider(analysis: &TypeAnalysis) -> TokenStream {
  let Some(positional) = &analysis.attributes.value_provider else {
    return TokenStream::new();
  };
  let properties = &analysis.members.properties;
  let by_position: Vec<&PropertyField> = positional
    .iter()
    .map(|argument| {
      properties
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

/// The `ArgInfo` of a property used as a value provider argument: its name and value type, as in
/// its `PropertyInfo`.
fn arg_info(field: &PropertyField) -> TokenStream {
  let inner = field.inner;
  let key = key_path(field);
  quote!(__artic::ArgInfo { name: #key.name(), value_type: ::core::stringify!(#inner) })
}

/// The `static AttachedKey` of one attached property.
fn attached_key(attached: &Attached) -> TokenStream {
  let Attached {
    name,
    ty,
    spec,
    key_static: key,
    ..
  } = attached;
  let default = match &spec.default {
    Some(expression) => quote!(#expression),
    None => quote_spanned!(ty.span()=> <#ty as ::core::default::Default>::default()),
  };
  let affects = affects_tokens(&spec.affects);
  let inheritance = inheritance_tokens(spec.inherits.as_ref());
  quote! {
    static #key: __artic::AttachedKey<#ty> = __artic::AttachedKey::new(#name, #affects, #inheritance, || #default);
  }
}

/// The key constant and the `set_<name>` and `<name>` helpers of one attached property.
fn attached_helpers(owner: &Ident, attached: &Attached) -> TokenStream {
  let Attached {
    ty,
    key_static: key,
    constant,
    getter,
    setter,
    ..
  } = attached;
  let name = attached.name.value();
  let property = format!("`{owner}.{name}`");
  let constant_doc = format!("The {property} attached property.");
  let setter_doc = format!("Sets {property} on `node`, at local precedence.");
  let getter_doc = format!("Returns {property} of `node`, or its default when nothing set it.");
  quote! {
    #[doc = #constant_doc]
    pub const #constant: &'static __artic::AttachedKey<#ty> = &#key;

    #[doc = #setter_doc]
    pub fn #setter(tree: &mut __artic::UiTree, node: impl ::core::convert::Into<__artic::NodeId>, value: #ty) {
      tree.attached_property(node.into(), Self::#constant).set(value);
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
