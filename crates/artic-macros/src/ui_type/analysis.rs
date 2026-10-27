use crate::members::{Kind, Members};
use crate::ui_type::attributes::{TypeAttributes, parse_type_attributes};
use syn::ext::IdentExt;
use syn::{DeriveInput, Error, Ident};

/// A struct read for `UiType`: its fields and its struct attributes.
pub struct TypeAnalysis<'a> {
  pub ident: &'a Ident,
  pub members: Members<'a>,
  pub attributes: TypeAttributes,
}

/// Reads the struct's attributes and its fields, and checks what they generate together.
pub fn analyze(input: &DeriveInput) -> syn::Result<TypeAnalysis<'_>> {
  let attributes = parse_type_attributes(input)?;
  let members = Members::analyze(input, Kind::Type)?;
  check_attached(&attributes)?;
  members.check_names(attributes.attached.iter().flat_map(|attached| {
    let name = attached.name.value();
    [
      (&attached.getter, "getter"),
      (&attached.setter, "setter"),
      (&attached.constant, "constant"),
    ]
    .map(|(item, role)| {
      (
        item.unraw().to_string(),
        format!("the {role} of the attached property `{name}`"),
        attached.name.span(),
      )
    })
  }))?;
  check_positional(&input.ident, &attributes, &members)?;
  Ok(TypeAnalysis {
    ident: &input.ident,
    members,
    attributes,
  })
}

/// Rejects two attached properties with one name.
fn check_attached(attributes: &TypeAttributes) -> syn::Result<()> {
  for (index, attached) in attributes.attached.iter().enumerate() {
    let name = attached.name.value();
    if attributes.attached[..index]
      .iter()
      .any(|earlier| earlier.name.value() == name)
    {
      return Err(Error::new_spanned(
        &attached.name,
        format_args!("this type already declares the attached property `{name}`"),
      ));
    }
  }
  Ok(())
}

/// Rejects a positional argument that isn't a property, or that is listed twice.
fn check_positional(owner: &Ident, attributes: &TypeAttributes, members: &Members) -> syn::Result<()> {
  let Some(positional) = &attributes.value_provider else {
    return Ok(());
  };
  for (index, argument) in positional.iter().enumerate() {
    let is_property = members.properties.iter().any(|field| field.ident == argument);
    if !is_property {
      return Err(Error::new_spanned(
        argument,
        format_args!("`positional({argument})` names no property of `{owner}`"),
      ));
    }
    if positional[..index].contains(argument) {
      return Err(Error::new_spanned(
        argument,
        format_args!("`{argument}` is listed twice in `positional(...)`"),
      ));
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use syn::parse_quote;

  // Each `#[ui]` field's role follows how its type is written, looking through `Option` for a

  /// The error `analyze` gives for `input`.
  fn error(input: &DeriveInput) -> String {
    analyze(input).err().expect("a name clash").to_string()
  }

  // A property named like the builder's start or finish is rejected: its method would clash.
  #[test]
  fn builder_names_are_reserved() {
    for (field, words) in [("new", "the builder's start"), ("build", "the builder's finish")] {
      let field = Ident::new(field, proc_macro2::Span::call_site());
      let input: DeriveInput = parse_quote! { struct Border { #[ui] #field: Property<f32> } };

      assert_eq!(
        error(&input),
        format!("`{field}` would be both {words} and the method of the property `{field}`; rename one")
      );
    }
  }

  // A field that generates no member may use a builder name.
  #[test]
  fn builder_names_are_free_for_members_without_methods() {
    let input: DeriveInput = parse_quote! { struct Button { #[ui] new: Event<ClickArgs> } };

    assert!(analyze(&input).is_ok());
  }

  // A property named `type_info` with its own key is rejected: its key constant would hide

  // `UiType::TYPE_INFO`.
  #[test]
  fn type_info_is_reserved() {
    let input: DeriveInput = parse_quote! { struct Border { #[ui] type_info: Property<f32> } };

    assert_eq!(
      error(&input),
      "`TYPE_INFO` would be both `UiType::TYPE_INFO` and the key constant of the property `type_info`; rename one"
    );
  }

  // A property with a shared key gets no constant, so it may be named `type_info`.
  #[test]
  fn type_info_is_free_with_a_shared_key() {
    let input: DeriveInput = parse_quote! { struct Border { #[ui(key = INFO)] type_info: Property<f32> } };

    assert!(analyze(&input).is_ok());
  }

  // A property written in SCREAMING_SNAKE_CASE would get a method and a key constant of one name.
  #[test]
  fn method_clashing_with_its_own_key_constant_is_an_error() {
    let input: DeriveInput = parse_quote! { struct Border { #[ui] PADDING: Property<f32> } };

    assert_eq!(
      error(&input),
      "`PADDING` would be both the method of the property `PADDING` and the key constant of the property `PADDING`; rename one"
    );
  }

  // An attached property whose getter would be `new` or `build` is rejected.
  #[test]
  fn attached_getter_named_like_a_builder_method_is_an_error() {
    let input: DeriveInput = parse_quote! {
      #[ui(attached(name = "Build", ty = usize, target = children))]
      struct Grid;
    };

    assert_eq!(
      error(&input),
      "`build` would be both the builder's finish and the getter of the attached property `Build`; rename one"
    );
  }

  // Two attached properties whose names differ but give the same helpers are rejected.
  #[test]
  fn attached_names_with_the_same_helpers_are_an_error() {
    let input: DeriveInput = parse_quote! {
      #[ui(attached(name = "FooBar", ty = usize, target = any))]
      #[ui(attached(name = "fooBar", ty = usize, target = any))]
      struct Grid;
    };

    assert_eq!(
      error(&input),
      "`foo_bar` would be both the getter of the attached property `FooBar` and the getter of the attached property `fooBar`; rename one"
    );
  }

  // A property's key constant and an attached property's constant with one name are rejected.
  #[test]
  fn key_constant_clashing_with_an_attached_constant_is_an_error() {
    let input: DeriveInput = parse_quote! {
      #[ui(attached(name = "Row", ty = usize, target = children))]
      struct Grid { #[ui] Row: Property<usize> }
    };

    assert_eq!(
      error(&input),
      "`ROW` would be both the key constant of the property `Row` and the constant of the attached property `Row`; rename one"
    );
  }

  // An attached property's helpers split an acronym as a word, so `URLField`'s getter is

  // `url_field`, and a property of that name clashes with it.
  #[test]
  fn attached_acronym_gives_one_word_helpers() {
    let input: DeriveInput = parse_quote! {
      #[ui(attached(name = "URLField", ty = usize, target = children))]
      struct Form { #[ui] url_field: Property<usize> }
    };

    assert_eq!(
      error(&input),
      "`url_field` would be both the method of the property `url_field` and the getter of the attached property `URLField`; rename one"
    );
  }

  // An attached name whose getter isn't an identifier is rejected, rather than panicking later.
  #[test]
  fn attached_name_that_is_not_an_identifier_is_an_error() {
    let input: DeriveInput = parse_quote! {
      #[ui(attached(name = "Row Span", ty = usize, target = children))]
      struct Grid;
    };

    assert_eq!(
      error(&input),
      "`Row Span` can't name an attached property: its getter would be `row span`, which isn't a Rust identifier"
    );
  }

  // An attached name whose getter is `self`, `crate` or `super` is rejected: those can't be raw.
  #[test]
  fn attached_name_that_is_a_path_keyword_is_an_error() {
    let input: DeriveInput = parse_quote! {
      #[ui(attached(name = "SELF", ty = usize, target = children))]
      struct Grid;
    };

    assert_eq!(
      error(&input),
      "`SELF` can't name an attached property: its getter would be `self`, which can't be a raw identifier"
    );
  }

  // Two fields whose key constants are one name, such as `row_span` and `rowSpan`, are rejected.
  #[test]
  fn fields_with_one_key_constant_are_an_error() {
    let input: DeriveInput = parse_quote! {
      struct Grid { #[ui] row_span: Property<usize>, #[ui] rowSpan: Property<usize> }
    };

    assert_eq!(
      error(&input),
      "`ROW_SPAN` would be both the key constant of the property `row_span` and the key constant of the property `rowSpan`; rename one"
    );
  }

  // The content method takes part in the name check, so a property can't share its name.
  #[test]
  fn content_method_clashing_with_a_property_is_an_error() {
    let input: DeriveInput = parse_quote! {
      struct Panel {
        #[ui(content)] items: Children,
        #[ui] children: Property<f32>,
      }
    };

    assert_eq!(
      error(&input),
      "`children` would be both the content method of `items` and the method of the property `children`; rename one"
    );
  }

  // A `Child` or `Children` field without `#[ui]` is rejected too, rather than left as plain data

  // A collection field that isn't the content is rejected, whatever its type: children reach a
}
