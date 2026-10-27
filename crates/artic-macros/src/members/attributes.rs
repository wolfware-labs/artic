use proc_macro2::Span;
use syn::meta::ParseNestedMeta;
use syn::{Error, Expr, Field, Ident, LitStr, Meta, Path, Token};

/// The settings a field property and an attached property share.
#[derive(Default)]
#[cfg_attr(test, derive(Debug))]
pub struct PropertySpec {
  pub default: Option<Expr>,
  pub affects: Vec<Ident>,
  pub inherits: Option<Inherits>,
}

/// What `#[ui(...)]` on a field says.
#[derive(Default)]
#[cfg_attr(test, derive(Debug))]
pub struct FieldAttributes {
  pub spec: PropertySpec,
  pub key: Option<Path>,
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

/// Reads `meta` into `spec` if it's `default`, `affects` or `inherits`, and returns whether it
/// was. `affects` takes stage names joined with `|` as written: artic-core's `Affects` names the
/// stages, so one it doesn't have fails to compile at its span.
pub fn parse_spec_setting(meta: &ParseNestedMeta, spec: &mut PropertySpec) -> syn::Result<bool> {
  if meta.path.is_ident("default") {
    spec.default = Some(meta.value()?.parse::<Expr>()?);
  } else if meta.path.is_ident("affects") {
    let input = meta.value()?;
    spec.affects = vec![input.parse()?];
    while input.peek(Token![|]) {
      input.parse::<Token![|]>()?;
      spec.affects.push(input.parse()?);
    }
  } else if meta.path.is_ident("inherits") {
    spec.inherits = Some(if meta.input.peek(Token![=]) {
      Inherits::Combine(meta.value()?.parse::<Path>()?)
    } else {
      Inherits::Replace
    });
  } else {
    return Ok(false);
  }
  Ok(true)
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
      if parse_spec_setting(&meta, &mut attributes.spec)? {
        return Ok(());
      }
      if meta.path.is_ident("key") {
        attributes.key = Some(meta.value()?.parse::<Path>()?);
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
    && (attributes.spec.default.is_some()
      || attributes.key.is_some()
      || !attributes.spec.affects.is_empty()
      || attributes.spec.inherits.is_some()
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
    && (!attributes.spec.affects.is_empty() || attributes.spec.inherits.is_some())
  {
    return Err(Error::new_spanned(
      key,
      "a shared `key` already holds `affects` and `inherits`; set them on the key instead",
    ));
  }
  Ok(found)
}

/// The error for a `ui` key that isn't one of `expected`.
pub fn unknown_key(meta: &ParseNestedMeta, expected: &str) -> Error {
  let key = meta.path.get_ident().map_or_else(|| "?".to_owned(), ToString::to_string);
  meta.error(format_args!("unknown `ui` key `{key}`; expected {expected}"))
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use syn::{DeriveInput, parse_quote};

  /// Returns the first field of `input`.
  fn first_field(input: &DeriveInput) -> &Field {
    let syn::Data::Struct(data) = &input.data else {
      unreachable!("tests pass structs")
    };
    data.fields.iter().next().expect("the struct has a field")
  }

  /// The attributes of the first field of `input`.
  fn field_attributes(input: &DeriveInput) -> FieldAttributes {
    parse_field_attributes(first_field(input))
      .expect("valid attributes")
      .expect("has #[ui]")
  }

  // A field's `affects` lists stages joined with `|`, beside its other settings.
  #[test]
  fn affects_lists_stages() {
    let input: DeriveInput = parse_quote! {
      struct Text { #[ui(affects = layout | render, inherits, default = 12.0)] size: u8 }
    };

    let attributes = field_attributes(&input);

    let stages: Vec<String> = attributes.spec.affects.iter().map(ToString::to_string).collect();
    assert_eq!(stages, ["layout", "render"]);
    assert!(attributes.spec.inherits.is_some() && attributes.spec.default.is_some());
  }

  // `affects` keeps any name as written: artic-core's `Affects` decides which stages exist.
  #[test]
  fn affects_names_any_stage() {
    let input: DeriveInput = parse_quote! { struct Text { #[ui(affects = render | layut)] size: u8 } };

    let attributes = field_attributes(&input);

    let stages: Vec<String> = attributes.spec.affects.iter().map(ToString::to_string).collect();
    assert_eq!(stages, ["render", "layut"]);
  }

  // A bare `inherits` replaces what the field inherits, and `inherits = path` combines with it.
  #[test]
  fn inherits_takes_a_combining_function() {
    let replace: DeriveInput = parse_quote! { struct A { #[ui(inherits)] size: u8 } };
    let combine: DeriveInput = parse_quote! { struct A { #[ui(inherits = and)] enabled: bool } };

    assert!(matches!(field_attributes(&replace).spec.inherits, Some(Inherits::Replace)));
    assert!(matches!(field_attributes(&combine).spec.inherits, Some(Inherits::Combine(path)) if path.is_ident("and")));
  }

  // A bare `#[ui]` marks the field with no settings, and no attribute means not a UI field.
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
