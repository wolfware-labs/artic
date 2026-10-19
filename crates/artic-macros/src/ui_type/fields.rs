use crate::ui_type::attributes::{FieldAttributes, TypeAttributes, parse_field_attributes, parse_type_attributes};
use crate::ui_type::builders::{content_method_name, key_constant_name};
use quote::ToTokens;
use syn::ext::IdentExt;
use syn::{Data, DeriveInput, Error, Field, Fields, GenericArgument, Ident, PathArguments, Type, Visibility};

/// A struct read for the derive: its attributes and its fields, sorted by role.
pub struct Analysis<'a> {
  pub ident: &'a Ident,
  pub attributes: TypeAttributes,
  pub ui_fields: Vec<UiField<'a>>,
  pub other_fields: Vec<&'a Ident>,
}

/// A field marked `#[ui]`.
pub struct UiField<'a> {
  pub ident: &'a Ident,
  /// The field's visibility, which its generated method and key constant share.
  pub vis: &'a Visibility,
  pub ty: &'a Type,
  pub role: Role<'a>,
  pub attributes: FieldAttributes,
}

/// What a `#[ui]` field is, decided from how its type is written.
pub enum Role<'a> {
  /// `Property<T>`. `inner` is `T`; `value` is `T`, or the `X` of `Option<X>`, which is what text
  /// or an element sets.
  Property { inner: &'a Type, value: &'a Type },
  /// `Event<A>`, with `A` the arguments handlers receive.
  Event { args: &'a Type },
  /// Any other type, which must be a collection of child elements.
  Collection,
  /// `#[ui(flatten)]`: a property group whose properties are listed as the type's.
  Group,
}

/// Reads the struct's attributes and sorts its fields by role.
pub fn analyze(input: &DeriveInput) -> syn::Result<Analysis<'_>> {
  let Data::Struct(data) = &input.data else {
    return Err(Error::new_spanned(&input.ident, "`UiType` can only be derived for a struct"));
  };
  let fields = match &data.fields {
    Fields::Named(fields) => fields.named.iter().collect(),
    Fields::Unit => Vec::new(),
    Fields::Unnamed(_) => {
      return Err(Error::new_spanned(&data.fields, "`UiType` needs a struct with named fields"));
    }
  };
  if !input.generics.params.is_empty() {
    return Err(Error::new_spanned(
      &input.generics,
      "`UiType` can't be derived for a generic struct",
    ));
  }
  let attributes = parse_type_attributes(input)?;
  let mut ui_fields = Vec::new();
  let mut other_fields = Vec::new();
  for field in fields {
    let ident = field.ident.as_ref().expect("named fields have names");
    match parse_field_attributes(field)? {
      Some(field_attributes) => ui_fields.push(classify(field, ident, field_attributes)?),
      None => {
        reject_unmarked_member(field)?;
        other_fields.push(ident);
      }
    }
  }
  check_attached(&attributes)?;
  check_generated_names(&attributes, &ui_fields)?;
  check_positional(&input.ident, &attributes, &ui_fields)?;
  check_content(&ui_fields)?;
  check_child_fields(&ui_fields)?;
  Ok(Analysis {
    ident: &input.ident,
    attributes,
    ui_fields,
    other_fields,
  })
}

/// Rejects a second field marked `#[ui(content)]`, and the mark on an event.
pub fn check_content(ui_fields: &[UiField]) -> syn::Result<()> {
  let mut marked = ui_fields
    .iter()
    .filter_map(|field| field.attributes.content.map(|span| (field, span)));
  if let Some((field, span)) = marked.next()
    && matches!(field.role, Role::Event { .. })
  {
    return Err(Error::new(
      span,
      "`content` goes on a `Property` or a collection field, not an event",
    ));
  }
  if let Some((_, span)) = marked.next() {
    return Err(Error::new(span, "a type has at most one `#[ui(content)]` member"));
  }
  Ok(())
}

/// Rejects a `Child` or `Children` field that isn't the content: a type's children are linked
/// through its content, by a UI language and by `WithChildren` alike.
fn check_child_fields(ui_fields: &[UiField]) -> syn::Result<()> {
  for field in ui_fields {
    let Type::Path(path) = field.ty else { continue };
    let Some(last) = path.path.segments.last() else { continue };
    let is_child_field = last.ident == "Child" || last.ident == "Children";
    if is_child_field && field.attributes.content.is_none() {
      return Err(Error::new_spanned(
        field.ty,
        format_args!(
          "a `{}` field must be the type's content: mark it `#[ui(content)]`",
          last.ident
        ),
      ));
    }
  }
  Ok(())
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

/// Rejects two items of one name in the inherent `impl` the derive writes: the builder's `new`
/// and `build`, the content method, each property's method and key constant, and each attached
/// property's getter, setter and constant. `TYPE_INFO` is taken too, since a constant of that name
/// would hide `UiType::TYPE_INFO`. The error points at the second item's source.
fn check_generated_names(attributes: &TypeAttributes, ui_fields: &[UiField]) -> syn::Result<()> {
  let mut taken: Vec<(String, String)> = [
    ("new", "the builder's start"),
    ("build", "the builder's finish"),
    ("TYPE_INFO", "`UiType::TYPE_INFO`"),
  ]
  .into_iter()
  .map(|(name, what)| (name.to_owned(), what.to_owned()))
  .collect();
  if let Some(field) = ui_fields
    .iter()
    .find(|field| field.attributes.content.is_some() && matches!(field.role, Role::Collection))
  {
    let name = content_method_name(field.ty).to_owned();
    take(
      &mut taken,
      name,
      format!("the content method of `{}`", field.ident),
      field.ident,
    )?;
  }
  for field in ui_fields {
    if !matches!(field.role, Role::Property { .. }) {
      continue;
    }
    let ident = field.ident;
    take(
      &mut taken,
      ident.unraw().to_string(),
      format!("the method of the property `{ident}`"),
      ident,
    )?;
    if field.attributes.key.is_none() {
      let constant = key_constant_name(ident).to_string();
      take(
        &mut taken,
        constant,
        format!("the key constant of the property `{ident}`"),
        ident,
      )?;
    }
  }
  for attached in &attributes.attached {
    let name = attached.name.value();
    let getter = snake_case(&name);
    let items = [(format!("set_{getter}"), "setter"), (getter.to_uppercase(), "constant")];
    take(
      &mut taken,
      getter,
      format!("the getter of the attached property `{name}`"),
      &attached.name,
    )?;
    for (item, role) in items {
      take(
        &mut taken,
        item,
        format!("the {role} of the attached property `{name}`"),
        &attached.name,
      )?;
    }
  }
  Ok(())
}

/// Records that `what` is named `name`, or fails if an earlier item already is.
fn take(taken: &mut Vec<(String, String)>, name: String, what: String, source: &impl ToTokens) -> syn::Result<()> {
  if let Some((_, earlier)) = taken.iter().find(|(other, _)| *other == name) {
    return Err(Error::new_spanned(
      source,
      format_args!("`{name}` would be both {earlier} and {what}; rename one"),
    ));
  }
  taken.push((name, what));
  Ok(())
}

/// Rejects a positional argument that isn't a property, or that is listed twice.
fn check_positional(owner: &Ident, attributes: &TypeAttributes, ui_fields: &[UiField]) -> syn::Result<()> {
  let Some(positional) = &attributes.value_provider else {
    return Ok(());
  };
  for (index, argument) in positional.iter().enumerate() {
    let is_property = ui_fields
      .iter()
      .any(|field| field.ident == argument && matches!(field.role, Role::Property { .. }));
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

/// The field's name in UI languages: its `name` override, or the field name in PascalCase.
pub fn ui_name(field: &UiField) -> String {
  match &field.attributes.name {
    Some(name) => name.value(),
    None => pascal_case(&field.ident.to_string()),
  }
}

/// Decides a `#[ui]` field's role from its attributes and how its type is written.
pub fn classify<'a>(field: &'a Field, ident: &'a Ident, attributes: FieldAttributes) -> syn::Result<UiField<'a>> {
  let role = if attributes.flatten.is_some() {
    Role::Group
  } else if let Some(value) = generic_argument(&field.ty, "Property") {
    Role::Property {
      inner: value,
      value: generic_argument(value, "Option").unwrap_or(value),
    }
  } else if let Some(args) = generic_argument(&field.ty, "Event") {
    Role::Event { args }
  } else {
    Role::Collection
  };
  if matches!(role, Role::Event { .. } | Role::Collection) {
    let property_only =
      attributes.default.is_some() || attributes.key.is_some() || attributes.affects.is_some() || attributes.inherits.is_some();
    if property_only {
      return Err(Error::new_spanned(
        &field.ty,
        "`default`, `key`, `affects` and `inherits` only apply to `Property` fields",
      ));
    }
  }
  Ok(UiField {
    ident,
    vis: &field.vis,
    ty: &field.ty,
    role,
    attributes,
  })
}

/// A `Property` or `Event` field must be marked `#[ui]`: the derive builds properties with their
/// keys, and an unmarked one would have none.
pub fn reject_unmarked_member(field: &Field) -> syn::Result<()> {
  for wrapper in ["Property", "Event"] {
    if generic_argument(&field.ty, wrapper).is_some() {
      return Err(Error::new_spanned(
        &field.ty,
        format_args!("add `#[ui]` to this `{wrapper}` field so the derive can create it"),
      ));
    }
  }
  Ok(())
}

/// If `ty` is written `Wrapper<T>`, possibly with a path in front, returns `T`.
pub fn generic_argument<'a>(ty: &'a Type, wrapper: &str) -> Option<&'a Type> {
  let Type::Path(path) = ty else {
    return None;
  };
  let segment = path.path.segments.last()?;
  if segment.ident != wrapper {
    return None;
  }
  let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
    return None;
  };
  match arguments.args.first()? {
    GenericArgument::Type(argument) if arguments.args.len() == 1 => Some(argument),
    _ => None,
  }
}

/// `ColumnSpan` as `column_span`: the name of a generated helper.
pub fn snake_case(name: &str) -> String {
  let mut snake = String::with_capacity(name.len() + 4);
  for (index, character) in name.chars().enumerate() {
    if character.is_uppercase() && index > 0 {
      snake.push('_');
    }
    snake.extend(character.to_lowercase());
  }
  snake
}

fn pascal_case(name: &str) -> String {
  name
    .trim_start_matches("r#")
    .split('_')
    .filter(|part| !part.is_empty())
    .map(|part| {
      let mut chars = part.chars();
      chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect::<String>())
        .unwrap_or_default()
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use quote::ToTokens;
  use syn::parse_quote;

  /// Returns the source text of `ty`, for comparing types.
  fn text(ty: &Type) -> String {
    ty.to_token_stream().to_string()
  }

  // Each `#[ui]` field's role follows how its type is written, looking through `Option` for a
  // property's value; unmarked fields are kept apart.
  #[test]
  fn fields_are_sorted_by_role() {
    let input: DeriveInput = parse_quote! {
      struct Border {
        layout: Layout,
        #[ui(content)] child: Property<Option<NodeId>>,
        #[ui(default = Thickness::ZERO)] padding: artic::Property<Thickness>,
        #[ui] pointer_pressed: Event<PointerEvent>,
        #[ui] items: Vec<NodeId>,
      }
    };

    let analysis = analyze(&input).expect("valid struct");

    let roles: Vec<String> = analysis
      .ui_fields
      .iter()
      .map(|field| match &field.role {
        Role::Property { value, .. } => format!("property {}", text(value)),
        Role::Event { args } => format!("event {}", text(args)),
        Role::Collection => "collection".to_owned(),
        Role::Group => "group".to_owned(),
      })
      .collect();
    assert_eq!(
      roles,
      ["property NodeId", "property Thickness", "event PointerEvent", "collection"]
    );
    assert_eq!(analysis.other_fields, ["layout"]);
  }

  // `#[ui(flatten)]` makes a field a group, whatever its type looks like.
  #[test]
  fn flatten_makes_a_group() {
    let input: DeriveInput = parse_quote! {
      struct Border { #[ui(flatten)] layout: Layout }
    };

    let analysis = analyze(&input).expect("valid struct");

    assert!(matches!(analysis.ui_fields[0].role, Role::Group));
  }

  // A field's name in UI languages is its name in PascalCase unless `name` overrides it.
  #[test]
  fn ui_names() {
    let input: DeriveInput = parse_quote! {
      struct Border {
        #[ui] corner_radius: Property<f32>,
        #[ui(name = "BorderBrush")] brush: Property<f32>,
      }
    };

    let analysis = analyze(&input).expect("valid struct");

    let names: Vec<String> = analysis.ui_fields.iter().map(ui_name).collect();
    assert_eq!(names, ["CornerRadius", "BorderBrush"]);
  }

  // Attached property names become snake_case helper names.
  #[test]
  fn snake_case_names() {
    let names: Vec<String> = ["Row", "ColumnSpan", "Name"].into_iter().map(snake_case).collect();
    assert_eq!(names, ["row", "column_span", "name"]);
  }

  // An event can't be content: content is a value or child elements.
  #[test]
  fn content_on_an_event_is_an_error() {
    let input: DeriveInput = parse_quote! {
      struct Button { #[ui(content)] click: Event<ClickEvent> }
    };

    let error = analyze(&input).err().expect("content on an event");

    assert!(error.to_string().contains("not an event"));
  }

  // A `Property` field without `#[ui]` is rejected, since the derive must create it with a key.
  #[test]
  fn unmarked_property_is_an_error() {
    let input: DeriveInput = parse_quote! {
      struct Border { padding: Property<f32> }
    };

    let error = analyze(&input).err().expect("unmarked property");

    assert!(error.to_string().starts_with("add `#[ui]` to this `Property` field"));
  }

  // Property settings on an event field are rejected.
  #[test]
  fn property_settings_on_an_event_are_an_error() {
    let input: DeriveInput = parse_quote! {
      struct Button { #[ui(affects = render)] clicked: Event<ClickArgs> }
    };

    let error = analyze(&input).err().expect("affects on an event");

    assert!(error.to_string().contains("only apply to `Property` fields"));
  }

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

  // A `Child` or `Children` field that isn't the content is rejected: children are linked only
  // through the content.
  #[test]
  fn child_fields_must_be_the_content() {
    for (ty, name) in [(quote::quote!(Child), "Child"), (quote::quote!(artic::Children), "Children")] {
      let input: DeriveInput = parse_quote! { struct Panel { #[ui] items: #ty } };

      assert_eq!(
        error(&input),
        format!("a `{name}` field must be the type's content: mark it `#[ui(content)]`")
      );
    }
  }
}
