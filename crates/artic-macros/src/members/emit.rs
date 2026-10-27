use crate::members::attributes::Inherits;
use crate::members::{Content, Key, Members, PropertyField, generic_argument};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Ident, Type};

/// The `Default` impl of `ident`: each property at its key's default, every other field at its
/// type's. A type with something to run on the built value passes it as `after`, which reads the
/// value as `__artic_value`.
pub fn default_impl(ident: &Ident, members: &Members, after: Option<TokenStream>) -> TokenStream {
  let properties = members.properties.iter().map(default_field);
  let content = match &members.content {
    Some(Content::Collection { ident, .. }) => Some(*ident),
    _ => None,
  };
  let others = members
    .events
    .iter()
    .map(|event| event.ident)
    .chain(members.groups.iter().map(|group| group.ident))
    .chain(content)
    .chain(members.others.iter().copied());
  let value = quote! {
    Self {
      #(#properties,)*
      #(#others: ::core::default::Default::default(),)*
    }
  };
  let body = match after {
    Some(after) => quote! {
      let __artic_value = #value;
      #after
      __artic_value
    },
    None => value,
  };
  quote! {
    impl ::core::default::Default for #ident {
      fn default() -> Self {
        #body
      }
    }
  }
}

/// The body of `visit_properties`: each property, then the properties of each flattened group.
pub fn visits(members: &Members) -> TokenStream {
  let properties = members.properties.iter().map(|field| {
    let field_ident = field.ident;
    quote!(visitor.visit(&self.#field_ident);)
  });
  let groups = members.groups.iter().map(|group| {
    let field_ident = group.ident;
    quote!(__artic::PropertyGroup::visit_properties(&self.#field_ident, visitor);)
  });
  quote!(#(#properties)* #(#groups)*)
}

/// The `__PROPERTIES` static: the `PropertyInfo` of each property, in field order.
pub fn properties_static(members: &Members) -> TokenStream {
  let count = members.properties.len();
  let infos = members.properties.iter().map(property_info);
  quote!(static __PROPERTIES: [__artic::PropertyInfo; #count] = [#(#infos),*];)
}

/// The `__checks` function, never called, whose body fails to compile when a property's value
/// type can't be written in a UI language or a flattened field isn't a group, followed by the
/// derive's `others`.
pub fn checks(members: &Members, others: impl IntoIterator<Item = TokenStream>) -> TokenStream {
  let values = members.properties.iter().map(|field| value_check(field.value));
  let groups = members.groups.iter().map(|group| {
    let ty = group.ty;
    quote_spanned!(ty.span()=> __artic::assert_property_group::<#ty>();)
  });
  let others = others.into_iter();
  quote! {
    #[allow(dead_code, clippy::needless_borrow)]
    fn __checks() {
      use __artic::{ViaChild as _, ViaFromStr as _, ViaUiType as _};
      #(#values)*
      #(#groups)*
      #(#others)*
    }
  }
}

/// The `static PropertyKey` the derive writes for a property without a shared `key`.
pub fn key_static(field: &PropertyField) -> TokenStream {
  let Key::Own {
    key_static: key,
    affects,
    inherits,
    ..
  } = &field.key
  else {
    return TokenStream::new();
  };
  let (inner, name) = (field.inner, &field.ui_name);
  let affects = affects_tokens(affects);
  let inheritance = inheritance_tokens(inherits.as_ref());
  quote! {
    static #key: __artic::PropertyKey<#inner> = __artic::PropertyKey::new(#name, #affects, #inheritance);
  }
}

/// The public constant for a property's own key, or `None` for a field that shares a key, whose
/// owner already names it.
pub fn key_constant(field: &PropertyField) -> Option<TokenStream> {
  let Key::Own {
    key_static, constant, ..
  } = &field.key
  else {
    return None;
  };
  let (vis, inner) = (field.vis, field.inner);
  let doc = format!("The key of `{}`, for styles.", field.ui_name);
  Some(quote! {
    #[doc = #doc]
    #vis const #constant: &'static __artic::PropertyKey<#inner> = &#key_static;
  })
}

/// How `Default` creates one property field.
fn default_field(field: &PropertyField) -> TokenStream {
  let (field_ident, inner) = (field.ident, field.inner);
  let key = key_path(field);
  let value = match (&field.key, &field.default) {
    (Key::Shared(shared), Some(expression)) => {
      quote_spanned!(expression.span()=> __artic::default_beside(&#shared, #expression))
    }
    (Key::Shared(shared), None) => quote!({
      use __artic::SharedKey as _;
      #shared.initial_value()
    }),
    (Key::Own { .. }, Some(expression)) => quote!(#expression),
    (Key::Own { .. }, None) => quote_spanned!(inner.span()=> <#inner as ::core::default::Default>::default()),
  };
  quote!(#field_ident: __artic::Property::new(#key, #value))
}

/// The `PropertyInfo` for one property, with its metadata read from the key.
pub fn property_info(field: &PropertyField) -> TokenStream {
  let (inner, value) = (field.inner, field.value);
  let key = key_path(field);
  let field_name = field.ident.to_string();
  quote! {
    __artic::PropertyInfo {
      name: #key.name(),
      field: #field_name,
      value_type: ::core::stringify!(#inner),
      affects: #key.affects(),
      inherits: #key.inherits(),
      literal: __artic::literal!(#value),
    }
  }
}

/// How a builder method sets a `Property` field: its parameter, the value it stores and its doc.
pub struct Setter {
  pub parameter: TokenStream,
  pub stored: TokenStream,
  pub doc: String,
}

/// The `Setter` of a property field. A `Property<Option<V>>` takes a `V`: `None` is the default,
/// so a builder only ever sets a value.
///
/// The method takes `impl Into<T>`, except for a number type, which it takes as is: a literal
/// such as `8.0` is an `f64` until something fixes its type, and `impl Into<f32>` doesn't.
pub fn setter(field: &PropertyField) -> Setter {
  let (inner, value, name) = (field.inner, field.value, &field.ui_name);
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
  Setter { parameter, stored, doc }
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

/// The check that a value type can be written in a UI language, pointing at the type.
pub fn value_check(value: &Type) -> TokenStream {
  quote_spanned!(value.span()=>
    (&&__artic::ValueCheck::<#value>(__artic::PhantomData)).value_must_parse_from_text_and_implement_torust_or_derive_uitype();
  )
}

/// The key a property is stored under, as a `&'static PropertyKey` expression usable in const
/// code: the derive's own static, or a shared key's `key()`, which a property key and an attached
/// key both have.
pub fn key_path(field: &PropertyField) -> TokenStream {
  match &field.key {
    Key::Shared(key) => quote!(#key.key()),
    Key::Own { key_static, .. } => quote!((&#key_static)),
  }
}

/// The `Affects` value of the stages listed, each named at its span so artic-core reports a stage it
/// doesn't have there.
pub fn affects_tokens(affects: &[Ident]) -> TokenStream {
  let stages: Vec<TokenStream> = affects
    .iter()
    .map(|stage| {
      let constant = format_ident!("{}", stage.to_string().to_uppercase(), span = stage.span());
      quote_spanned!(stage.span()=> __artic::Affects::#constant)
    })
    .collect();
  match stages.split_first() {
    None => quote!(__artic::Affects::NONE),
    Some((first, rest)) => quote!(#first #(.union(#rest))*),
  }
}

/// The `Inheritance` value of an `inherits` setting: a bare name combines with the function of
/// that name in `artic_core::property::inherit`.
pub fn inheritance_tokens(inherits: Option<&Inherits>) -> TokenStream {
  match inherits {
    None => quote!(__artic::Inheritance::None),
    Some(Inherits::Replace) => quote!(__artic::Inheritance::Replace),
    Some(Inherits::Combine(path)) => match path.get_ident() {
      Some(name) => quote_spanned!(path.span()=> __artic::Inheritance::Combine(__artic::inherit::#name)),
      None => quote_spanned!(path.span()=> __artic::Inheritance::Combine(#path)),
    },
  }
}
