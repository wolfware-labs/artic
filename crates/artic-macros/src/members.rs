pub(crate) mod attributes;

use crate::members::attributes::{FieldAttributes, Inherits, parse_field_attributes};
use artic_ident::{pascal_case, screaming_snake_case};
use proc_macro2::Span;
use quote::format_ident;
use syn::ext::IdentExt;
use syn::{Data, DeriveInput, Error, Expr, Field, Fields, GenericArgument, Ident, Path, PathArguments, Type, Visibility};

/// The derive a struct is read for, which decides what its fields may be.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Kind {
  /// `UiType`: a type with properties, events, flattened groups and content.
  Type,
  /// `UiProperties`: a property group, which holds only properties.
  Group,
}

impl Kind {
  /// The derive's name, for errors.
  fn derive(self) -> &'static str {
    match self {
      Kind::Type => "UiType",
      Kind::Group => "UiProperties",
    }
  }

  /// The names a member can't take, with what already holds each: a type's builder start and
  /// finish and `UiType::TYPE_INFO`, which a constant would hide; for a group, the builder items
  /// of every type that flattens it.
  fn reserved(self) -> Vec<(String, String)> {
    match self {
      Kind::Type => [
        ("new", "the builder's start"),
        ("build", "the builder's finish"),
        ("TYPE_INFO", "`UiType::TYPE_INFO`"),
      ]
      .into_iter()
      .map(|(name, what)| (name.to_owned(), what.to_owned()))
      .collect(),
      Kind::Group => ["new", "build", "child", "children"]
        .into_iter()
        .map(|name| (name.to_owned(), format!("the `{name}` of every type that flattens the group")))
        .collect(),
    }
  }
}

/// A struct's fields read for a derive, sorted by what they are.
pub struct Members<'a> {
  kind: Kind,
  pub properties: Vec<PropertyField<'a>>,
  pub events: Vec<EventField<'a>>,
  pub groups: Vec<GroupField<'a>>,
  pub content: Option<Content<'a>>,
  pub others: Vec<&'a Ident>,
}

/// A `Property<T>` field marked `#[ui]`.
pub struct PropertyField<'a> {
  pub ident: &'a Ident,
  /// The field's visibility, which its generated method and key constant share.
  pub vis: &'a Visibility,
  pub inner: &'a Type,
  /// `T`, or the `X` of `Option<X>`: what text or an element sets.
  pub value: &'a Type,
  /// The name in UI languages: the `name` override, or the field name in PascalCase.
  pub ui_name: String,
  pub default: Option<Expr>,
  pub key: Key,
}

/// The key a property is stored under.
pub enum Key {
  /// A key the derive writes, as the static `key_static`, named in the type by `constant`.
  Own {
    key_static: Ident,
    constant: Ident,
    affects: Vec<Ident>,
    inherits: Option<Inherits>,
  },
  /// `key = PATH`: a key another type owns.
  Shared(Path),
}

/// An `Event<A>` field marked `#[ui]`, with `args` the `A` handlers receive.
pub struct EventField<'a> {
  pub ident: &'a Ident,
  pub ui_name: String,
  pub args: &'a Type,
}

/// A `#[ui(flatten)]` field: a property group whose properties are listed as the type's.
pub struct GroupField<'a> {
  pub ident: &'a Ident,
  pub ty: &'a Type,
}

/// The member marked `#[ui(content)]`.
pub enum Content<'a> {
  /// The property at this index in `Members::properties`.
  Property(usize),
  /// A collection field, which holds the type's children.
  Collection {
    ident: &'a Ident,
    vis: &'a Visibility,
    ty: &'a Type,
    method: Option<ContentMethod>,
  },
}

/// The method that takes a type's child elements, as the registry's `ContentMethod` names it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ContentMethod {
  /// `child`, for a field written `Child`.
  Child,
  /// `children`, for a field written `Children`.
  Children,
}

impl ContentMethod {
  /// The method's name.
  pub fn name(self) -> &'static str {
    match self {
      ContentMethod::Child => "child",
      ContentMethod::Children => "children",
    }
  }

  /// The registry's variant, for the type's descriptor.
  pub fn variant(self) -> Ident {
    match self {
      ContentMethod::Child => format_ident!("Child"),
      ContentMethod::Children => format_ident!("Children"),
    }
  }
}

impl<'a> Members<'a> {
  /// Reads the fields of `input` for the derive `kind`, rejecting what that derive can't hold.
  pub fn analyze(input: &'a DeriveInput, kind: Kind) -> syn::Result<Members<'a>> {
    let derive = kind.derive();
    let Data::Struct(data) = &input.data else {
      return Err(Error::new_spanned(
        &input.ident,
        format_args!("`{derive}` can only be derived for a struct"),
      ));
    };
    let fields = match &data.fields {
      Fields::Named(fields) => fields.named.iter().collect(),
      Fields::Unit if kind == Kind::Type => Vec::new(),
      _ => {
        return Err(Error::new_spanned(
          &data.fields,
          format_args!("`{derive}` needs a struct with named fields"),
        ));
      }
    };
    if !input.generics.params.is_empty() {
      return Err(Error::new_spanned(
        &input.generics,
        format_args!("`{derive}` can't be derived for a generic struct"),
      ));
    }
    let mut members = Members {
      kind,
      properties: Vec::new(),
      events: Vec::new(),
      groups: Vec::new(),
      content: None,
      others: Vec::new(),
    };
    let mut marked = Vec::new();
    let mut collections = Vec::new();
    for field in fields {
      let ident = field.ident.as_ref().expect("named fields have names");
      let Some(attributes) = parse_field_attributes(field)? else {
        reject_unmarked_member(field, kind)?;
        members.others.push(ident);
        continue;
      };
      let content = attributes.content;
      let member = classify(field, ident, attributes)?;
      if kind == Kind::Group && !matches!(member, Member::Property(_)) {
        return Err(Error::new_spanned(&field.ty, "a property group holds only `Property` fields"));
      }
      let resolved = match member {
        Member::Property(property) => {
          members.properties.push(*property);
          Some(Content::Property(members.properties.len() - 1))
        }
        Member::Event(event) => {
          members.events.push(event);
          None
        }
        Member::Group(group) => {
          members.groups.push(group);
          None
        }
        Member::Collection => {
          collections.push((&field.ty, content.is_some()));
          Some(Content::Collection {
            ident,
            vis: &field.vis,
            ty: &field.ty,
            method: content_method(&field.ty),
          })
        }
      };
      if let Some(span) = content {
        marked.push((span, resolved));
      }
    }
    members.content = resolve_content(marked)?;
    check_collections(&collections)?;
    Ok(members)
  }
}

impl Members<'_> {
  /// Rejects two items of one name in the `impl`s the derive writes: the reserved names, the
  /// content method, each property's method and key constant, then each of `others` (a type's
  /// attached property helpers), as name, what it is and where it comes from. The error points at
  /// the second item's source.
  pub fn check_names(&self, others: impl IntoIterator<Item = (String, String, Span)>) -> syn::Result<()> {
    let mut taken = self.kind.reserved();
    if let Some(Content::Collection {
      ident,
      method: Some(method),
      ..
    }) = &self.content
    {
      take(
        &mut taken,
        method.name().to_owned(),
        format!("the content method of `{ident}`"),
        ident.span(),
      )?;
    }
    for field in &self.properties {
      let ident = field.ident;
      take(
        &mut taken,
        ident.unraw().to_string(),
        format!("the method of the property `{ident}`"),
        ident.span(),
      )?;
      if let Key::Own { constant, .. } = &field.key {
        take(
          &mut taken,
          constant.to_string(),
          format!("the key constant of the property `{ident}`"),
          ident.span(),
        )?;
      }
    }
    for (name, what, source) in others {
      take(&mut taken, name, what, source)?;
    }
    Ok(())
  }
}

/// Records that `what` is named `name`, or fails if an earlier item already is.
fn take(taken: &mut Vec<(String, String)>, name: String, what: String, source: Span) -> syn::Result<()> {
  if let Some((_, earlier)) = taken.iter().find(|(other, _)| *other == name) {
    return Err(Error::new(
      source,
      format_args!("`{name}` would be both {earlier} and {what}; rename one"),
    ));
  }
  taken.push((name, what));
  Ok(())
}

/// A `#[ui]` field read by `classify`, before `Members::analyze` files it.
enum Member<'a> {
  Property(Box<PropertyField<'a>>),
  Event(EventField<'a>),
  Group(GroupField<'a>),
  Collection,
}

/// Decides what a `#[ui]` field is from its attributes and how its type is written.
fn classify<'a>(field: &'a Field, ident: &'a Ident, attributes: FieldAttributes) -> syn::Result<Member<'a>> {
  let ui_name = match &attributes.name {
    Some(name) => name.value(),
    None => pascal_case(&ident.to_string()),
  };
  if attributes.flatten.is_some() {
    return Ok(Member::Group(GroupField { ident, ty: &field.ty }));
  }
  if let Some(inner) = generic_argument(&field.ty, "Property") {
    let spec = attributes.spec;
    let key = match attributes.key {
      Some(path) => Key::Shared(path),
      None => {
        let constant = screaming_snake_case(&ident.unraw().to_string());
        Key::Own {
          key_static: format_ident!("__KEY_{constant}"),
          constant: format_ident!("{constant}"),
          affects: spec.affects,
          inherits: spec.inherits,
        }
      }
    };
    return Ok(Member::Property(Box::new(PropertyField {
      ident,
      vis: &field.vis,
      inner,
      value: generic_argument(inner, "Option").unwrap_or(inner),
      ui_name,
      default: spec.default,
      key,
    })));
  }
  let property_only = attributes.spec.default.is_some()
    || attributes.key.is_some()
    || !attributes.spec.affects.is_empty()
    || attributes.spec.inherits.is_some();
  if property_only {
    return Err(Error::new_spanned(
      &field.ty,
      "`default`, `key`, `affects` and `inherits` only apply to `Property` fields",
    ));
  }
  Ok(match generic_argument(&field.ty, "Event") {
    Some(args) => Member::Event(EventField { ident, ui_name, args }),
    None => Member::Collection,
  })
}

/// The content from the members marked `#[ui(content)]`, in field order: at most one, and not an
/// event, which resolves to `None`.
fn resolve_content(marked: Vec<(Span, Option<Content<'_>>)>) -> syn::Result<Option<Content<'_>>> {
  let mut marked = marked.into_iter();
  let Some((span, content)) = marked.next() else {
    return Ok(None);
  };
  if content.is_none() {
    return Err(Error::new(
      span,
      "`content` goes on a `Property` or a collection field, not an event",
    ));
  }
  if let Some((span, _)) = marked.next() {
    return Err(Error::new(span, "a type has at most one `#[ui(content)]` member"));
  }
  Ok(content)
}

/// Rejects a collection field that isn't the content: a type's children reach it only through
/// its content, by a UI language and by `WithChildren` alike, so nothing could fill another one.
fn check_collections(collections: &[(&Type, bool)]) -> syn::Result<()> {
  for (ty, is_content) in collections {
    if !is_content {
      let message = match content_method(ty) {
        Some(method) => format!(
          "a `{}` field must be the type's content: mark it `#[ui(content)]`",
          method.variant()
        ),
        None => "`#[ui]` makes this field a collection of children, and only the content can be one: mark it \
                 `#[ui(content)]`, or make it a `Property` field"
          .to_owned(),
      };
      return Err(Error::new_spanned(ty, message));
    }
  }
  Ok(())
}

/// A `Property` or `Event` field must be marked `#[ui]`: the derive builds properties with their
/// keys, and an unmarked one would have none. In a type, a `Child` or `Children` field must be
/// marked `#[ui(content)]`, since children reach a type only through its content; a group holds
/// neither children nor events.
fn reject_unmarked_member(field: &Field, kind: Kind) -> syn::Result<()> {
  let held_by_types_only = content_method(&field.ty).is_some() || generic_argument(&field.ty, "Event").is_some();
  if kind == Kind::Group && held_by_types_only {
    return Err(Error::new_spanned(&field.ty, "a property group holds only `Property` fields"));
  }
  if let Some(method) = content_method(&field.ty) {
    return Err(Error::new_spanned(
      &field.ty,
      format_args!(
        "add `#[ui(content)]` to this `{}` field so it holds the type's children",
        method.variant()
      ),
    ));
  }
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

/// The method that takes a collection field's children: `Child` for a field written `Child`,
/// `Children` for one written `Children`, and `None` for any other collection, whose values
/// aren't child elements.
fn content_method(ty: &Type) -> Option<ContentMethod> {
  let Type::Path(path) = ty else {
    return None;
  };
  match path.path.segments.last()?.ident.to_string().as_str() {
    "Child" => Some(ContentMethod::Child),
    "Children" => Some(ContentMethod::Children),
    _ => None,
  }
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

  /// The error `Members::analyze` gives for `input` read for `kind`.
  fn error(input: &DeriveInput, kind: Kind) -> String {
    Members::analyze(input, kind).err().expect("an error").to_string()
  }

  // Each `#[ui]` field is filed by how its type is written, looking through `Option` for a
  // property's value; unmarked fields are kept apart.
  #[test]
  fn fields_are_sorted_by_role() {
    let input: DeriveInput = parse_quote! {
      struct Border {
        layout: Layout,
        #[ui] child: Property<Option<NodeId>>,
        #[ui(default = Thickness::ZERO)] padding: artic::Property<Thickness>,
        #[ui] pointer_pressed: Event<PointerEvent>,
        #[ui(content)] items: Vec<NodeId>,
      }
    };

    let members = Members::analyze(&input, Kind::Type).expect("valid struct");

    let values: Vec<String> = members.properties.iter().map(|property| text(property.value)).collect();
    assert_eq!(values, ["NodeId", "Thickness"]);
    let args: Vec<String> = members.events.iter().map(|event| text(event.args)).collect();
    assert_eq!(args, ["PointerEvent"]);
    assert!(matches!(members.content, Some(Content::Collection { ident, method: None, .. }) if ident == "items"));
    assert_eq!(members.others, ["layout"]);
  }

  // `#[ui(flatten)]` makes a field a group, whatever its type looks like.
  #[test]
  fn flatten_makes_a_group() {
    let input: DeriveInput = parse_quote! {
      struct Border { #[ui(flatten)] layout: Layout }
    };

    let members = Members::analyze(&input, Kind::Type).expect("valid struct");

    assert_eq!(members.groups[0].ident, "layout");
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

    let members = Members::analyze(&input, Kind::Type).expect("valid struct");

    let names: Vec<&str> = members.properties.iter().map(|property| property.ui_name.as_str()).collect();
    assert_eq!(names, ["CornerRadius", "BorderBrush"]);
  }

  // An event can't be content: content is a value or child elements.
  #[test]
  fn content_on_an_event_is_an_error() {
    let input: DeriveInput = parse_quote! {
      struct Button { #[ui(content)] click: Event<ClickEvent> }
    };

    assert!(error(&input, Kind::Type).contains("not an event"));
  }

  // A `Property` field without `#[ui]` is rejected, since the derive must create it with a key.
  #[test]
  fn unmarked_property_is_an_error() {
    let input: DeriveInput = parse_quote! {
      struct Border { padding: Property<f32> }
    };

    assert!(error(&input, Kind::Type).starts_with("add `#[ui]` to this `Property` field"));
  }

  // Property settings on an event field are rejected.
  #[test]
  fn property_settings_on_an_event_are_an_error() {
    let input: DeriveInput = parse_quote! {
      struct Button { #[ui(affects = render)] clicked: Event<ClickArgs> }
    };

    assert!(error(&input, Kind::Type).contains("only apply to `Property` fields"));
  }

  // A `Child` or `Children` field without `#[ui]` is rejected too, rather than left as plain data
  // that no child ever reaches.
  #[test]
  fn unmarked_child_fields_are_an_error() {
    let input: DeriveInput = parse_quote! { struct Panel { items: Children } };

    assert_eq!(
      error(&input, Kind::Type),
      "add `#[ui(content)]` to this `Children` field so it holds the type's children"
    );
  }

  // A collection field that isn't the content is rejected, whatever its type: children reach a
  // type only through its content, so nothing could fill it.
  #[test]
  fn collection_fields_must_be_the_content() {
    for (ty, name) in [(quote::quote!(Child), "Child"), (quote::quote!(artic::Children), "Children")] {
      let input: DeriveInput = parse_quote! { struct Panel { #[ui] items: #ty } };

      assert_eq!(
        error(&input, Kind::Type),
        format!("a `{name}` field must be the type's content: mark it `#[ui(content)]`")
      );
    }
    let input: DeriveInput = parse_quote! { struct Gradient { #[ui] stops: Items } };
    assert_eq!(
      error(&input, Kind::Type),
      "`#[ui]` makes this field a collection of children, and only the content can be one: mark it `#[ui(content)]`, or make it a `Property` field"
    );
  }

  // A group holds only properties, and says so on the first error, marked or not.
  #[test]
  fn a_group_holds_only_properties() {
    let inputs: [DeriveInput; 4] = [
      parse_quote! { struct Common { items: Children } },
      parse_quote! { struct Common { clicked: Event<()> } },
      parse_quote! { struct Common { #[ui(content)] items: Children } },
      parse_quote! { struct Common { #[ui] clicked: Event<()> } },
    ];

    for input in inputs {
      assert_eq!(error(&input, Kind::Group), "a property group holds only `Property` fields");
    }
  }

  // Only a type may be a unit struct, and the error names the derive.
  #[test]
  fn a_unit_group_is_an_error() {
    let input: DeriveInput = parse_quote! { struct Common; };

    assert!(Members::analyze(&input, Kind::Type).is_ok());
    assert_eq!(error(&input, Kind::Group), "`UiProperties` needs a struct with named fields");
  }
}
