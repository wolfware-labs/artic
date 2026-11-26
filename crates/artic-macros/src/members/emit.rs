use crate::members::attributes::Inherits;
use crate::members::{Content, Key, Members, PropertyField, ValueField, is_optional, unraw_name};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Ident, Type};

/// The expression that creates the struct: each property at its default, each required field
/// from the variable of its name, and every other field at its own `Default`, followed by `after`,
/// which reads the value as `__artic_value`.
pub fn initializer(members: &Members, after: Option<TokenStream>) -> TokenStream {
  let properties = members.properties.iter().map(default_field);
  let content = match &members.content {
    Some(Content::Collection { ident, .. }) => Some(*ident),
    _ => None,
  };
  let plain = members
    .events
    .iter()
    .map(|event| event.ident)
    .chain(members.groups.iter().map(|group| group.ident))
    .chain(content)
    .chain(members.unmarked.iter().copied());
  let required = members.required.iter().map(|field| field.ident);
  let value = quote! {
    Self {
      #(#properties,)*
      #(#plain: ::core::default::Default::default(),)*
      #(#required,)*
    }
  };
  match after {
    Some(after) => quote! {
      let __artic_value = #value;
      #after
      __artic_value
    },
    None => value,
  }
}

/// The `Default` impl: the struct as [`initializer`] creates it.
pub fn default_impl(ident: &Ident, members: &Members, after: Option<TokenStream>) -> TokenStream {
  let body = initializer(members, after);
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

/// The `__PROPERTIES` static of `owner`: the `PropertyInfo` of each property, or of each plain
/// field of a value, in field order.
pub fn properties_static(owner: &Ident, members: &Members) -> TokenStream {
  let count = members.properties.len() + members.values.len();
  let infos = members
    .properties
    .iter()
    .map(property_info)
    .chain(members.values.iter().map(|field| value_field_info(owner, field)));
  quote!(static __PROPERTIES: [__artic::PropertyInfo; #count] = [#(#infos),*];)
}

/// The `__checks` function, never called, whose body fails to compile when a property's value
/// type can't be written in a UI language or a flattened field isn't a group, followed by the
/// derive's `extra_checks`.
pub fn checks(members: &Members, extra_checks: impl IntoIterator<Item = TokenStream>) -> TokenStream {
  let values = members
    .properties
    .iter()
    .map(|field| value_check(field.value, ValueRoute::AnyValue))
    .chain(
      members
        .values
        .iter()
        .map(|field| value_check(field.value, ValueRoute::PlainValue)),
    );
  let groups = members.groups.iter().map(|group| {
    let ty = group.ty;
    quote_spanned!(ty.span()=> __artic::assert_property_group::<#ty>();)
  });
  let extra_checks = extra_checks.into_iter();
  let imports = value_check_imports();
  quote! {
    #[allow(dead_code, clippy::needless_borrow)]
    fn __checks() {
      #imports
      #(#values)*
      #(#groups)*
      #(#extra_checks)*
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
  let access = property_access(field);
  quote! {
    __artic::PropertyInfo {
      name: #key.name(),
      field: #field_name,
      value_type: ::core::stringify!(#inner),
      affects: #key.affects(),
      inherits: #key.inherits(),
      literal: __artic::literal!(#value),
      access: #access,
    }
  }
}

/// The `PropertyAccess` of one property: thunks whose bodies know its value type and find the
/// property on any node by its key, at the level the loader passes.
fn property_access(field: &PropertyField) -> TokenStream {
  let key = key_path(field);
  access_tokens(&Access {
    value: field.value,
    route: ValueRoute::AnyValue,
    optional: is_optional(field.inner),
    name: quote!(#key.name()),
    level: quote!(level),
    write: &|value| quote!(__artic::set_property(tree, node, #key, level, #value)),
    clear: quote!(|tree, node, level| __artic::clear_property(tree, node, #key, level)),
  })
}

/// The `PropertyInfo` for a value's plain field. The field has no key, so it affects no frame
/// stage and inherits nothing: the property that holds the value does both.
fn value_field_info(owner: &Ident, field: &ValueField) -> TokenStream {
  let (name, ty, value) = (&field.ui_name, field.ty, field.value);
  let field_name = field.ident.to_string();
  let access = value_field_access(owner, field);
  quote! {
    __artic::PropertyInfo {
      name: #name,
      field: #field_name,
      value_type: ::core::stringify!(#ty),
      affects: __artic::Affects::NONE,
      inherits: false,
      literal: __artic::literal!(#value),
      access: #access,
    }
  }
}

/// The `PropertyAccess` of a value type's plain field: thunks that assign the field of a staged
/// value. A value has no precedence levels, so the level is ignored, and clearing the field puts
/// back the type's default. The field takes no node: a value taken out of the tree would leave the
/// node there with no owner, so a `NodeId` field fails the value check.
fn value_field_access(owner: &Ident, field: &ValueField) -> TokenStream {
  let (ident, name) = (field.ident, &field.ui_name);
  let reach = quote!(|owner: &mut #owner| &mut owner.#ident);
  access_tokens(&Access {
    value: field.value,
    route: ValueRoute::PlainValue,
    optional: is_optional(field.ty),
    name: quote!(#name),
    level: quote!(_),
    write: &|value| quote!(__artic::set_field(tree, node, #reach, #value)),
    clear: quote!(|tree, node, _| {
      __artic::set_field(tree, node, #reach, <#owner as ::core::default::Default>::default().#ident)
    }),
  })
}

/// What one property's thunks differ in: its value type, which values its check lets through,
/// whether it is optional, its name, the level parameter its writes use, how a converted value is
/// written, and how it is cleared.
struct Access<'a> {
  value: &'a Type,
  route: ValueRoute,
  optional: bool,
  name: TokenStream,
  level: TokenStream,
  write: &'a dyn Fn(TokenStream) -> TokenStream,
  clear: TokenStream,
}

/// A `PropertyAccess` built from `access`. A `Property<Option<V>>` or an `Option<V>` field parses
/// and takes a `V`, and is the only kind that can be set to null; the others refuse null with the
/// property's name.
///
/// The thunks convert through the value check's own expression, spanned at the value type, so a
/// value type a UI language can't write fails once, there. An object converts through its own
/// check, which refuses one for a value type not taken from objects. Each converts the value before
/// writing it, so a conversion that needs the tree mutably ends before the write borrows it.
fn access_tokens(access: &Access) -> TokenStream {
  let Access {
    value,
    route,
    optional,
    name,
    level,
    write,
    clear,
  } = access;
  let wrap = |converted: TokenStream| {
    if *optional {
      quote!(::core::option::Option::Some(#converted))
    } else {
      converted
    }
  };
  let set = |converted: TokenStream| {
    let converted = wrap(converted);
    let written = write(quote!(value));
    quote!({
      let value = #converted;
      #written
    })
  };
  let ops = value_ops(value, *route);
  let imports = value_check_imports();
  let parsed = set(quote!((#ops.parse)(text, ::core::stringify!(#value))?));
  let node_value = set(quote!((#ops.node)(child, ::core::stringify!(#value))?));
  let object = set(quote!(
    (&__artic::ObjectCheck::<#value>(__artic::PhantomData)).take_object(tree, node, object, ::core::stringify!(#value))?
  ));
  let set_null = if *optional {
    let null = write(quote!(::core::option::Option::None));
    quote!(|tree, node, #level| #null)
  } else {
    quote!(|_, _, _| ::core::result::Result::Err(__artic::ValueError::NotNullable { property: #name }))
  };
  quote! {
    &__artic::PropertyAccess {
      set_text: |tree, node, text, #level| {
        #imports
        #parsed
      },
      set_node: |tree, node, child, #level| {
        #imports
        #node_value
      },
      set_object: |tree, node, object, #level| {
        use __artic::{NotFromObject as _, ViaFromUiObject as _};
        #object
      },
      set_null: #set_null,
      clear: #clear,
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
  let optional = is_optional(inner);
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

/// Which values the value check lets through, as the registry's marker of the same name says.
#[derive(Clone, Copy)]
pub enum ValueRoute {
  /// A property's value: text, an object or a node.
  AnyValue,
  /// A value type's plain field: text or an object. A `NodeId` there fails as a type that can't be
  /// written does.
  PlainValue,
}

impl ValueRoute {
  /// The registry's marker type for this route.
  fn marker(self) -> TokenStream {
    match self {
      ValueRoute::AnyValue => quote!(__artic::AnyValue),
      ValueRoute::PlainValue => quote!(__artic::PlainValue),
    }
  }
}

/// The check that a value type can be written in a UI language, pointing at the type.
pub fn value_check(value: &Type, route: ValueRoute) -> TokenStream {
  let ops = value_ops(value, route);
  quote_spanned!(value.span()=> #ops;)
}

/// The value check as an expression, pointing at the type: it compiles only for a value type a UI
/// language can write through `route`, and gives the `ValueOps` the loader's thunks convert with.
/// The registry's `value_ops` pins that type, so a user trait with the check's method name in scope
/// where the derive expands can't stand in for the check with a type of its own.
fn value_ops(value: &Type, route: ValueRoute) -> TokenStream {
  let route = route.marker();
  quote_spanned!(value.span()=>
    __artic::value_ops::<#value>(
      (&&__artic::ValueCheck::<#value, #route>(__artic::PhantomData)).value_must_parse_from_text_and_implement_torust_or_derive_uitype()
    )
  )
}

/// The imports the value check resolves through, one trait per kind of value.
fn value_check_imports() -> TokenStream {
  quote!(
    use __artic::{ViaChild as _, ViaFromStr as _, ViaUiType as _};
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

/// The `Affects` value of the stages listed, each named at its span so artic-core reports a
/// stage it doesn't have there.
pub fn affects_tokens(affects: &[Ident]) -> TokenStream {
  let stages: Vec<TokenStream> = affects
    .iter()
    .map(|stage| {
      let constant = format_ident!("{}", unraw_name(stage).to_uppercase(), span = stage.span());
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

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use syn::parse_quote;

  // A stage written as a raw identifier names the same constant as the plain one, rather than
  // panicking the derive with an invalid identifier.
  #[test]
  fn a_raw_stage_names_its_constant() {
    let stages: Vec<Ident> = vec![parse_quote!(layout), parse_quote!(r#type)];

    assert_eq!(
      affects_tokens(&stages).to_string(),
      "__artic :: Affects :: LAYOUT . union (__artic :: Affects :: TYPE)"
    );
  }

  // A number type is `f32` itself or a path to it through `core::primitive` or `std::primitive`,
  // from the root or not; a bare name from the root, another module or another crate isn't one.
  #[test]
  fn number_paths_go_only_through_primitive() {
    assert!(is_number(&parse_quote!(f32)));
    assert!(is_number(&parse_quote!(core::primitive::f32)));
    assert!(is_number(&parse_quote!(std::primitive::u8)));
    assert!(is_number(&parse_quote!(::core::primitive::f32)));
    assert!(!is_number(&parse_quote!(::f32)));
    assert!(!is_number(&parse_quote!(core::other::f32)));
    assert!(!is_number(&parse_quote!(units::primitive::f32)));
  }
}
