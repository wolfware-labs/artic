use proc_macro2::Span;
use syn::meta::ParseNestedMeta;
use syn::{DeriveInput, Error, Expr, Field, Ident, LitStr, Meta, Path, Token, Type};

/// What `#[ui(...)]` on the struct says.
#[cfg_attr(test, derive(Debug))]
pub struct TypeAttributes {
  pub whitespace: Whitespace,
  pub crate_path: Option<Path>,
  pub attached: Vec<Attached>,
  /// The properties set by position, in order, if the type is a markup extension.
  pub value_provider: Option<Vec<Ident>>,
  pub override_defaults: bool,
}

/// One `attached(...)` entry on the struct: an attached property the type owns.
#[cfg_attr(test, derive(Debug))]
pub struct Attached {
  pub name: LitStr,
  pub ty: Type,
  pub default: Option<Expr>,
  pub affects: Affects,
  pub inherits: Option<Inherits>,
  pub target: Target,
}

/// Where an attached property may be set, from `target = children | any`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Target {
  Any,
  Children,
}

/// How the parser treats whitespace in the element's text content.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Whitespace {
  Collapse,
  Significant,
}

/// What `#[ui(...)]` on a field says.
#[derive(Default)]
#[cfg_attr(test, derive(Debug))]
pub struct FieldAttributes {
  pub default: Option<Expr>,
  pub key: Option<Path>,
  pub affects: Option<Affects>,
  pub inherits: Option<Inherits>,
  pub name: Option<LitStr>,
  pub flatten: Option<Span>,
  pub content: Option<Span>,
}

/// How a property inherits, from a bare `inherits` or `inherits = path`.
#[cfg_attr(test, derive(Debug))]
pub enum Inherits {
  Replace,
  Combine(Path),
}

/// The stages listed in `affects = layout | render | surface`.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct Affects {
  pub layout: bool,
  pub render: bool,
  pub surface: bool,
}

/// Reads the struct's `#[ui(...)]` attributes.
pub fn parse_type_attributes(input: &DeriveInput) -> syn::Result<TypeAttributes> {
  let mut whitespace = Whitespace::Collapse;
  let mut crate_path = None;
  let mut attached = Vec::new();
  let mut value_provider = None;
  let mut override_defaults = false;
  for attribute in input.attrs.iter().filter(|attribute| attribute.path().is_ident("ui")) {
    attribute.parse_nested_meta(|meta| {
      if meta.path.is_ident("whitespace") {
        let value: LitStr = meta.value()?.parse()?;
        whitespace = match value.value().as_str() {
          "collapse" => Whitespace::Collapse,
          "significant" => Whitespace::Significant,
          _ => return Err(Error::new_spanned(value, "expected \"collapse\" or \"significant\"")),
        };
      } else if meta.path.is_ident("crate") {
        let value: LitStr = meta.value()?.parse()?;
        crate_path = Some(value.parse::<Path>()?);
      } else if meta.path.is_ident("attached") {
        attached.push(parse_attached(&meta)?);
      } else if meta.path.is_ident("value_provider") {
        value_provider = Some(parse_value_provider(&meta)?);
      } else if meta.path.is_ident("override_defaults") {
        override_defaults = true;
      } else {
        return Err(unknown_key(
          &meta,
          "`whitespace`, `crate`, `attached`, `value_provider` or `override_defaults`",
        ));
      }
      Ok(())
    })?;
  }
  Ok(TypeAttributes {
    whitespace,
    crate_path,
    attached,
    value_provider,
    override_defaults,
  })
}

/// Reads `value_provider` or `value_provider(positional(key, fallback))`, returning the
/// properties set by position.
fn parse_value_provider(value_provider: &ParseNestedMeta) -> syn::Result<Vec<Ident>> {
  let mut positional = Vec::new();
  if value_provider.input.peek(syn::token::Paren) {
    value_provider.parse_nested_meta(|meta| {
      if !meta.path.is_ident("positional") {
        return Err(unknown_key(&meta, "`positional`"));
      }
      meta.parse_nested_meta(|argument| {
        positional.push(argument.path.require_ident()?.clone());
        Ok(())
      })
    })?;
  }
  Ok(positional)
}

/// Reads `attached(name = "Row", ty = usize, default = 0, affects = layout, inherits, target = children)`.
fn parse_attached(attached: &ParseNestedMeta) -> syn::Result<Attached> {
  let mut name = None;
  let mut ty = None;
  let mut default = None;
  let mut affects = Affects::default();
  let mut inherits = None;
  let mut target = None;
  attached.parse_nested_meta(|meta| {
    if meta.path.is_ident("name") {
      name = Some(meta.value()?.parse::<LitStr>()?);
    } else if meta.path.is_ident("ty") {
      ty = Some(meta.value()?.parse::<Type>()?);
    } else if meta.path.is_ident("default") {
      default = Some(meta.value()?.parse::<Expr>()?);
    } else if meta.path.is_ident("affects") {
      affects = parse_affects(meta.value()?)?;
    } else if meta.path.is_ident("target") {
      let value: Ident = meta.value()?.parse()?;
      target = Some(match value.to_string().as_str() {
        "children" => Target::Children,
        "any" => Target::Any,
        _ => return Err(Error::new_spanned(value, "expected `children` or `any`")),
      });
    } else if meta.path.is_ident("inherits") {
      inherits = Some(if meta.input.peek(Token![=]) {
        Inherits::Combine(meta.value()?.parse::<Path>()?)
      } else {
        Inherits::Replace
      });
    } else {
      return Err(unknown_key(
        &meta,
        "`name`, `ty`, `default`, `affects`, `inherits` or `target`",
      ));
    }
    Ok(())
  })?;
  let missing = |key: &str| attached.error(format_args!("`attached(...)` needs `{key} = ...`"));
  Ok(Attached {
    name: name.ok_or_else(|| missing("name"))?,
    ty: ty.ok_or_else(|| missing("ty"))?,
    default,
    affects,
    inherits,
    target: target.ok_or_else(|| missing("target"))?,
  })
}

/// Reads a field's `#[ui]` or `#[ui(...)]` attribute, or returns `None` if it has none.
pub fn parse_field_attributes(field: &Field) -> syn::Result<Option<FieldAttributes>> {
  let mut found = None;
  for attribute in field.attrs.iter().filter(|attribute| attribute.path().is_ident("ui")) {
    let attributes = found.get_or_insert_with(FieldAttributes::default);
    if matches!(attribute.meta, Meta::Path(_)) {
      continue;
    }
    attribute.parse_nested_meta(|meta| {
      if meta.path.is_ident("default") {
        attributes.default = Some(meta.value()?.parse::<Expr>()?);
      } else if meta.path.is_ident("key") {
        attributes.key = Some(meta.value()?.parse::<Path>()?);
      } else if meta.path.is_ident("affects") {
        attributes.affects = Some(parse_affects(meta.value()?)?);
      } else if meta.path.is_ident("inherits") {
        attributes.inherits = Some(if meta.input.peek(Token![=]) {
          Inherits::Combine(meta.value()?.parse::<Path>()?)
        } else {
          Inherits::Replace
        });
      } else if meta.path.is_ident("name") {
        attributes.name = Some(meta.value()?.parse::<LitStr>()?);
      } else if meta.path.is_ident("flatten") {
        attributes.flatten = Some(meta.path.get_ident().map_or_else(Span::call_site, Ident::span));
      } else if meta.path.is_ident("content") {
        attributes.content = Some(meta.path.get_ident().map_or_else(Span::call_site, Ident::span));
      } else {
        return Err(unknown_key(
          &meta,
          "`default`, `key`, `affects`, `inherits`, `name`, `flatten` or `content`",
        ));
      }
      Ok(())
    })?;
  }
  if let Some(attributes) = &found
    && attributes.flatten.is_some()
    && (attributes.default.is_some()
      || attributes.key.is_some()
      || attributes.affects.is_some()
      || attributes.inherits.is_some()
      || attributes.name.is_some()
      || attributes.content.is_some())
  {
    return Err(Error::new_spanned(
      field,
      "`flatten` takes no other settings: the group's properties carry their own",
    ));
  }
  if let Some(attributes) = &found
    && let Some(key) = &attributes.key
    && (attributes.affects.is_some() || attributes.inherits.is_some())
  {
    return Err(Error::new_spanned(
      key,
      "a shared `key` already holds `affects` and `inherits`; set them on the key instead",
    ));
  }
  Ok(found)
}

fn parse_affects(input: syn::parse::ParseStream) -> syn::Result<Affects> {
  let mut affects = Affects::default();
  loop {
    let stage: Ident = input.parse()?;
    match stage.to_string().as_str() {
      "layout" => affects.layout = true,
      "render" => affects.render = true,
      "surface" => affects.surface = true,
      _ => return Err(Error::new_spanned(stage, "expected `layout`, `render` or `surface`")),
    }
    if !input.peek(Token![|]) {
      return Ok(affects);
    }
    input.parse::<Token![|]>()?;
  }
}

fn unknown_key(meta: &ParseNestedMeta, expected: &str) -> Error {
  let key = meta.path.get_ident().map_or_else(|| "?".to_owned(), ToString::to_string);
  meta.error(format_args!("unknown `ui` key `{key}`; expected {expected}"))
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use syn::parse_quote;

  /// Returns the first field of `input`.
  fn first_field(input: &DeriveInput) -> &Field {
    let syn::Data::Struct(data) = &input.data else {
      unreachable!("tests pass structs")
    };
    data.fields.iter().next().expect("the struct has a field")
  }

  // The struct attributes are read into plain values.
  #[test]
  fn type_attributes_are_parsed() {
    let input: DeriveInput = parse_quote! {
      #[ui(override_defaults, whitespace = "significant")]
      struct Border { child: u8 }
    };

    let attributes = parse_type_attributes(&input).expect("valid attributes");

    assert!(attributes.override_defaults);
    assert_eq!(attributes.whitespace, Whitespace::Significant);
  }

  // `crate` names the path generated code uses to reach artic.
  #[test]
  fn crate_path_is_parsed() {
    let input: DeriveInput = parse_quote! {
      #[ui(crate = "::my_artic")]
      struct Border;
    };

    let attributes = parse_type_attributes(&input).expect("valid attributes");

    let path = attributes.crate_path.expect("crate is set");
    assert_eq!(quote::quote!(#path).to_string(), ":: my_artic");
  }

  // An `attached(...)` entry is read with its type, default, stages and target.
  #[test]
  fn attached_is_parsed() {
    let input: DeriveInput = parse_quote! {
      #[ui(attached(name = "Row", ty = usize, default = 1, affects = layout, target = children))]
      #[ui(attached(name = "Label", ty = String, target = any))]
      struct Grid;
    };

    let attributes = parse_type_attributes(&input).expect("valid attributes");

    let [row, label] = attributes.attached.as_slice() else {
      panic!("two attached properties");
    };
    assert_eq!(row.name.value(), "Row");
    let ty = &row.ty;
    assert_eq!(quote::quote!(#ty).to_string(), "usize");
    assert!(row.default.is_some());
    assert_eq!(
      row.affects,
      Affects {
        layout: true,
        render: false,
        surface: false
      }
    );
    assert_eq!((row.target, label.target), (Target::Children, Target::Any));
    assert!(label.default.is_none());
  }

  // `attached(...)` needs a name, a type and a target, and knows its targets.
  #[test]
  fn attached_errors() {
    let message = |attached: proc_macro2::TokenStream| {
      let input: DeriveInput = parse_quote! {
        #[ui(attached(#attached))]
        struct Grid;
      };
      parse_type_attributes(&input).expect_err("invalid attached").to_string()
    };

    assert_eq!(
      message(quote::quote!(ty = u8, target = any)),
      "`attached(...)` needs `name = ...`"
    );
    assert_eq!(
      message(quote::quote!(name = "Row", target = any)),
      "`attached(...)` needs `ty = ...`"
    );
    assert_eq!(
      message(quote::quote!(name = "Row", ty = u8)),
      "`attached(...)` needs `target = ...`"
    );
    assert_eq!(
      message(quote::quote!(name = "Row", ty = u8, target = parent)),
      "expected `children` or `any`"
    );
  }

  // `inherits` on an attached property is read like a field's: bare, or with a combining function.
  #[test]
  fn attached_inherits_is_parsed() {
    let attached = |tokens: proc_macro2::TokenStream| {
      let input: DeriveInput = parse_quote! {
        #[ui(attached(#tokens))]
        struct Desktop;
      };
      parse_type_attributes(&input).expect("valid").attached.remove(0)
    };

    assert!(matches!(
      attached(quote::quote!(name = "Chrome", ty = u8, target = any, inherits)).inherits,
      Some(Inherits::Replace)
    ));
    assert!(matches!(
      attached(quote::quote!(name = "Enabled", ty = bool, target = any, inherits = and)).inherits,
      Some(Inherits::Combine(path)) if path.is_ident("and")
    ));
    assert!(
      attached(quote::quote!(name = "Row", ty = u8, target = any))
        .inherits
        .is_none()
    );
  }

  // `value_provider` marks a markup extension, with the properties set by position in order.
  #[test]
  fn markup_extension_is_parsed() {
    let positional = |input: DeriveInput| {
      let attributes = parse_type_attributes(&input).expect("valid attributes");
      attributes
        .value_provider
        .map(|names| names.iter().map(ToString::to_string).collect::<Vec<_>>())
    };

    let plain: DeriveInput = parse_quote! { struct Border; };
    let bare: DeriveInput = parse_quote! { #[ui(value_provider)] struct Null; };
    let listed: DeriveInput = parse_quote! {
      #[ui(value_provider(positional(key, fallback)))]
      struct Localize;
    };

    assert_eq!(positional(plain), None);
    assert_eq!(positional(bare), Some(Vec::new()));
    assert_eq!(positional(listed), Some(vec!["key".to_owned(), "fallback".to_owned()]));
  }

  // A struct needs no attributes: its namespace is its crate, and `namespace` is no longer a key.
  #[test]
  fn struct_needs_no_attributes() {
    let plain: DeriveInput = parse_quote! { struct Border; };
    let old: DeriveInput = parse_quote! { #[ui(namespace = "https://artic.rs/ui")] struct Border; };

    assert!(parse_type_attributes(&plain).is_ok());
    assert!(
      parse_type_attributes(&old)
        .expect_err("unknown key")
        .to_string()
        .starts_with("unknown `ui` key `namespace`")
    );
  }

  // A field's `affects` takes `layout`, `render` or both joined with `|`.
  #[test]
  fn affects_lists_stages() {
    let input: DeriveInput = parse_quote! {
      struct Text { #[ui(affects = layout | render, inherits, default = 12.0)] size: u8 }
    };

    let attributes = parse_field_attributes(first_field(&input))
      .expect("valid attributes")
      .expect("has #[ui]");

    assert_eq!(
      attributes.affects,
      Some(Affects {
        layout: true,
        render: true,
        surface: false
      })
    );
    assert!(attributes.inherits.is_some() && attributes.default.is_some());
  }

  // A bare `inherits` replaces what the field inherits, and `inherits = path` combines with it.
  #[test]
  fn inherits_takes_a_combining_function() {
    let replace: DeriveInput = parse_quote! { struct A { #[ui(inherits)] size: u8 } };
    let combine: DeriveInput = parse_quote! { struct A { #[ui(inherits = and)] enabled: bool } };

    let replace = parse_field_attributes(first_field(&replace))
      .expect("valid")
      .expect("has #[ui]");
    let combine = parse_field_attributes(first_field(&combine))
      .expect("valid")
      .expect("has #[ui]");

    assert!(matches!(replace.inherits, Some(Inherits::Replace)));
    assert!(matches!(combine.inherits, Some(Inherits::Combine(path)) if path.is_ident("and")));
  }

  // A bare `#[ui]` marks the field with no settings, and no attribute means not a XAML field.
  #[test]
  fn bare_and_missing_field_attributes() {
    let bare: DeriveInput = parse_quote! { struct A { #[ui] value: u8 } };
    let missing: DeriveInput = parse_quote! { struct A { value: u8 } };

    assert!(parse_field_attributes(first_field(&bare)).expect("valid").is_some());
    assert!(parse_field_attributes(first_field(&missing)).expect("valid").is_none());
  }

  // A shared key can't be combined with `affects` or `inherits`, which the key already holds.
  #[test]
  fn key_with_affects_is_an_error() {
    let input: DeriveInput = parse_quote! {
      struct Text { #[ui(key = FONT_SIZE, affects = layout)] size: u8 }
    };

    let error = parse_field_attributes(first_field(&input)).expect_err("conflict");

    assert!(error.to_string().starts_with("a shared `key` already holds"));
  }
}
