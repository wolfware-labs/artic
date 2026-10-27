use crate::members::attributes::{PropertySpec, parse_spec_setting, unknown_key};
use artic_ident::{IdentError, rust_identifier, screaming_snake_case, snake_case};
use quote::format_ident;
use syn::meta::ParseNestedMeta;
use syn::{DeriveInput, Error, Ident, LitStr, Path, Type};

/// What `#[ui(...)]` on the struct says.
#[cfg_attr(test, derive(Debug))]
pub struct TypeAttributes {
  pub whitespace: Whitespace,
  pub crate_path: Option<Path>,
  pub attached: Vec<Attached>,
  /// The properties set by position, in order, if the type is a value provider.
  pub value_provider: Option<Vec<Ident>>,
  pub override_defaults: bool,
}

/// One `attached(...)` entry on the struct: an attached property the type owns, with the names of
/// the items the derive writes for it.
#[cfg_attr(test, derive(Debug))]
pub struct Attached {
  pub name: LitStr,
  pub ty: Type,
  pub spec: PropertySpec,
  pub target: Target,
  /// The static the key lives in, such as `__ATTACHED_COLUMN_SPAN`.
  pub key_static: Ident,
  pub constant: Ident,
  pub getter: Ident,
  pub setter: Ident,
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
  let mut spec = PropertySpec::default();
  let mut target = None;
  attached.parse_nested_meta(|meta| {
    if parse_spec_setting(&meta, &mut spec)? {
      return Ok(());
    }
    if meta.path.is_ident("name") {
      name = Some(meta.value()?.parse::<LitStr>()?);
    } else if meta.path.is_ident("ty") {
      ty = Some(meta.value()?.parse::<Type>()?);
    } else if meta.path.is_ident("target") {
      let value: Ident = meta.value()?.parse()?;
      target = Some(match value.to_string().as_str() {
        "children" => Target::Children,
        "any" => Target::Any,
        _ => return Err(Error::new_spanned(value, "expected `children` or `any`")),
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
  let name = name.ok_or_else(|| missing("name"))?;
  let ty = ty.ok_or_else(|| missing("ty"))?;
  let target = target.ok_or_else(|| missing("target"))?;
  let text = name.value();
  let snake = snake_case(&text);
  let getter = match rust_identifier(&snake) {
    Ok(getter) => syn::parse_str::<Ident>(&getter).expect("a Rust identifier parses"),
    Err(error) => {
      let why = match error {
        IdentError::NotIdentifier => "isn't a Rust identifier",
        IdentError::PathKeyword => "can't be a raw identifier",
      };
      return Err(Error::new_spanned(
        &name,
        format_args!("`{text}` can't name an attached property: its getter would be `{snake}`, which {why}"),
      ));
    }
  };
  let constant = screaming_snake_case(&text);
  Ok(Attached {
    key_static: format_ident!("__ATTACHED_{constant}"),
    constant: format_ident!("{constant}"),
    setter: format_ident!("set_{snake}"),
    getter,
    name,
    ty,
    spec,
    target,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::members::attributes::Inherits;
  use pretty_assertions::assert_eq;
  use syn::parse_quote;

  /// The names of `stages`, for comparing.
  fn stages(stages: &[Ident]) -> Vec<String> {
    stages.iter().map(ToString::to_string).collect()
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
    assert!(row.spec.default.is_some());
    assert_eq!(stages(&row.spec.affects), ["layout"]);
    assert_eq!((row.target, label.target), (Target::Children, Target::Any));
    assert!(label.spec.default.is_none());
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
      attached(quote::quote!(name = "Chrome", ty = u8, target = any, inherits))
        .spec
        .inherits,
      Some(Inherits::Replace)
    ));
    assert!(matches!(
      attached(quote::quote!(name = "Enabled", ty = bool, target = any, inherits = and)).spec.inherits,
      Some(Inherits::Combine(path)) if path.is_ident("and")
    ));
    assert!(
      attached(quote::quote!(name = "Row", ty = u8, target = any))
        .spec
        .inherits
        .is_none()
    );
  }
  // `value_provider` marks a value provider, with the properties set by position in order.
  #[test]
  fn value_provider_is_parsed() {
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
}
