use crate::crate_root::{ELEMENT, crate_root};
use crate::ui_type::attributes::parse_type_attributes;
use proc_macro2::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{Attribute, DeriveInput, Error, Field, Fields, FieldsNamed, Ident, ItemStruct, Path, Token, Type, parse_quote};

/// The fields `#[element]` adds, with their group types and docs, in the order it adds them.
const GROUPS: [(&str, &str, &str); 4] = [
  ("layout", "Layout", "Where the element sits and how big it is."),
  ("appearance", "Appearance", "How the element is drawn."),
  ("interaction", "Interaction", "How the element takes input."),
  ("identity", "Identity", "What style selectors and code find the element by."),
];

/// Makes the struct an element: adds the four property groups and `#[derive(UiType)]`, with the
/// crate it found as `#[ui(crate = ...)]` unless the struct names one, and writes `Layoutable`,
/// `Element`, `HasState` for its state fields and its cast-table entry.
pub fn expand(attribute: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
  if !attribute.is_empty() {
    return Err(Error::new_spanned(attribute, "`#[element]` takes no settings"));
  }
  let mut item: ItemStruct = syn::parse2(item)?;
  let ident = item.ident.clone();
  if !item.generics.params.is_empty() {
    return Err(Error::new_spanned(
      &item.generics,
      "`#[element]` can't be used on a generic struct",
    ));
  }
  reject_written_derives(&item.attrs)?;
  let mut fields = match &item.fields {
    Fields::Named(fields) => fields.clone(),
    Fields::Unit => parse_quote!({}),
    Fields::Unnamed(fields) => {
      return Err(Error::new_spanned(
        fields,
        "`#[element]` needs a struct with named fields, or a unit struct",
      ));
    }
  };
  for field in &fields.named {
    let name = field.ident.as_ref().expect("named fields have names");
    if GROUPS.iter().any(|(group, _, _)| name == group) {
      return Err(Error::new_spanned(
        name,
        format!("`{name}` is added by `#[element]`; remove this field"),
      ));
    }
  }
  let states = state_fields(&fields)?;

  let probe: DeriveInput = syn::parse2(quote!(#item))?;
  let crate_path = parse_type_attributes(&probe)?.crate_path;
  let root = crate_root(crate_path.as_ref(), &ident, ELEMENT)?;

  let added: FieldsNamed = {
    let groups = GROUPS.iter().map(|(name, ty, doc)| {
      let name = Ident::new(name, ident.span());
      let ty = Ident::new(ty, ident.span());
      quote!(#[doc = #doc] #[ui(flatten)] pub #name: #root::__private::#ty)
    });
    parse_quote!({ #(#groups,)* })
  };
  let own = std::mem::take(&mut fields.named);
  fields.named = added.named.into_iter().chain(own).collect();
  item.fields = Fields::Named(fields);
  item.semi_token = None;

  item.attrs.insert(0, parse_quote!(#[derive(#root::__private::UiType)]));
  if crate_path.is_none() {
    let root_text = root.to_string().replace(' ', "");
    item.attrs.push(parse_quote!(#[ui(crate = #root_text)]));
  }

  strip_element_settings(&mut item);
  let state_impls = states.iter().map(|(field_ident, ty)| {
    quote! {
      impl __artic::HasState<#ty> for #ident {
        fn state(&self) -> &#ty {
          &self.#field_ident
        }

        fn state_mut(&mut self) -> &mut #ty {
          &mut self.#field_ident
        }
      }
    }
  });
  Ok(quote! {
    #item

    const _: () = {
      use #root::__private as __artic;

      impl __artic::Layoutable for #ident {
        fn measure(&self, cx: &mut __artic::MeasureCx<'_>, available: __artic::Size) -> __artic::Size {
          self.layout.measure(cx, available, |cx, available| {
            <Self as __artic::ElementImpl>::measure_override(self, cx, available)
          })
        }

        fn arrange(&self, cx: &mut __artic::ArrangeCx<'_>, slot: __artic::Rect) {
          self.layout.arrange(cx, slot, |cx, size| {
            <Self as __artic::ElementImpl>::arrange_override(self, cx, size)
          })
        }

        fn collapsed(&self) -> bool {
          self.appearance.visibility.get() == __artic::Visibility::Collapsed
        }
      }

      impl __artic::Element for #ident {
        fn render(&self, cx: &mut __artic::RenderCx<'_>, scene: &mut __artic::SceneBuilder) {
          <Self as __artic::ElementImpl>::render(self, cx, scene)
        }

        fn hit_test(&self, cx: &__artic::HitTestCx<'_>, point: __artic::Point) -> __artic::HitTestResult {
          self.interaction.hit_test(cx, self.appearance.visibility.get(), point)
        }

        fn on_event(&mut self, cx: &mut __artic::EventCx<'_>, event: &__artic::InputEvent) -> __artic::Handled {
          if !self.interaction.accepts_input() {
            return __artic::Handled::No;
          }
          <Self as __artic::ElementImpl>::on_event(self, cx, event)
        }

        fn accessibility(&self, cx: &__artic::AccessCx<'_>) -> __artic::accesskit::Node {
          let mut node = self.interaction.accessibility(cx);
          <Self as __artic::ElementImpl>::accessibility(self, cx, &mut node);
          node
        }

        fn layers(&self, size: __artic::Size) -> __artic::Layers {
          self.appearance.layers(size, <Self as __artic::ElementImpl>::clip(self, size))
        }
      }

      #(#state_impls)*

      __artic::element_type!(#ident);
    };
  })
}

/// Rejects `UiType` and `Default` in the struct's derives, since `#[element]` writes both.
fn reject_written_derives(attributes: &[Attribute]) -> syn::Result<()> {
  for attribute in attributes.iter().filter(|attribute| attribute.path().is_ident("derive")) {
    let derives = attribute.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)?;
    for derive in &derives {
      let Some(name) = derive.segments.last().map(|segment| segment.ident.to_string()) else {
        continue;
      };
      let message = match name.as_str() {
        "UiType" => "`#[element]` writes `UiType`; remove `UiType` from this derive",
        "Default" => "`#[element]`'s derive writes `Default`; remove it, and change defaults with `#[ui(override_defaults)]`",
        _ => continue,
      };
      return Err(Error::new_spanned(derive, message));
    }
  }
  Ok(())
}

/// Removes the `element` attributes from the struct and its fields, which only `#[element]` reads;
/// the `ui` attributes stay for the derive.
fn strip_element_settings(item: &mut ItemStruct) {
  let is_setting = |attribute: &Attribute| attribute.path().is_ident("element");
  item.attrs.retain(|attribute| !is_setting(attribute));
  for field in &mut item.fields {
    field.attrs.retain(|attribute| !is_setting(attribute));
  }
}

/// The fields marked `#[element(state)]`, each with its type. Two fields whose types are written
/// the same way are an error naming both.
fn state_fields(fields: &FieldsNamed) -> syn::Result<Vec<(Ident, Type)>> {
  let mut states: Vec<(Ident, Type)> = Vec::new();
  for field in &fields.named {
    if !is_state(field)? {
      continue;
    }
    let field_ident = field.ident.clone().expect("named fields have names");
    let ty = &field.ty;
    let written = quote!(#ty).to_string();
    if let Some((first, _)) = states.iter().find(|(_, other)| quote!(#other).to_string() == written) {
      return Err(Error::new_spanned(
        ty,
        format!("`{field_ident}` has the same state type as `{first}`; a struct holds at most one field per state type"),
      ));
    }
    states.push((field_ident, ty.clone()));
  }
  Ok(states)
}

/// Whether the field is marked `#[element(state)]`. Any other `element` setting is an error.
fn is_state(field: &Field) -> syn::Result<bool> {
  let mut state = false;
  for attribute in field.attrs.iter().filter(|attribute| attribute.path().is_ident("element")) {
    attribute.parse_nested_meta(|meta| {
      if meta.path.is_ident("state") {
        state = true;
        Ok(())
      } else {
        Err(meta.error("unknown `element` setting; the only one is `state`"))
      }
    })?;
  }
  Ok(state)
}
