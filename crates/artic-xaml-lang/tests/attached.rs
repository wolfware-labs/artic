// Attached properties: `Owner.Name` set on an element, as an attribute or a property element,
// becomes the owner's setter called once the element is built, where the owner's target rule
// allows it.

mod common;

use artic_macros::UiType;
use common::{Nodes, TEST_UI, compile, joined};

/// Owns `Rack.Slot`, which only its children may carry.
#[derive(Debug, UiType)]
#[ui(attached(name = "Slot", ty = usize, target = children))]
struct Rack {
  #[ui(content)]
  slots: Nodes,
}

// The attribute and the property element set the property alike, with the owner's setter called
// on the element once it's built.
#[test]
fn both_forms_call_the_owner_setter() {
  let attribute = compile(&format!(r#"<Rack xmlns="{TEST_UI}"><Rack Rack.Slot="3"/></Rack>"#)).expect("an attribute");
  let element = compile(&format!(
    r#"<Rack xmlns="{TEST_UI}"><Rack><Rack.Slot>3</Rack.Slot></Rack></Rack>"#
  ))
  .expect("a property element");
  for rust in [attribute, element] {
    assert!(joined(&rust).contains("Rack::set_slot(tree,__e0.id(),3_usize);"), "{rust}");
  }
}

// A property set where its target rule doesn't allow it is reported at its name, with the rule.
#[test]
fn attached_property_wrong_target_is_error() {
  let errors = compile(&format!(
    r#"<Rack xmlns="{TEST_UI}" xmlns:ui="https://artic.rs/ui"><ui:Border><Rack Rack.Slot="1"/></ui:Border></Rack>"#
  ))
  .expect_err("a slot outside a rack");
  assert!(
    errors.contains("`Rack.Slot` can only be set on a child of a `Rack`"),
    "{errors}"
  );
  assert!(errors.contains(":1:87:"), "{errors}");
  let errors = compile(&format!(r#"<Rack xmlns="{TEST_UI}" Rack.Slot="1"/>"#)).expect_err("a slot on the root");
  assert!(errors.contains("can only be set on a child of a `Rack`"), "{errors}");
}

// An owner without the property is reported at the name, naming both.
#[test]
fn attached_property_unknown_is_error() {
  let errors = compile(r#"<Border xmlns="https://artic.rs/ui" TextElement.FontColor="Red"/>"#).expect_err("unknown");
  assert!(
    errors.contains("`TextElement` has no attached property `FontColor`"),
    "{errors}"
  );
  assert!(errors.contains(":1:37:"), "{errors}");
}

// An owner that isn't found is reported as an unknown type.
#[test]
fn unknown_owner_is_error() {
  let errors = compile(r#"<Border xmlns="https://artic.rs/ui" Grid.Row="1"/>"#).expect_err("no grid yet");
  assert!(errors.contains("no type `Grid`"), "{errors}");
}

// One attached property set twice, in either form, is reported once, at the second.
#[test]
fn attached_property_set_twice_is_error() {
  let errors = compile(
    r#"<Border xmlns="https://artic.rs/ui" TextElement.FontSize="12"><TextElement.FontSize>14</TextElement.FontSize></Border>"#,
  )
  .expect_err("set twice");
  assert!(errors.contains("set twice"), "{errors}");
}

// A value that doesn't parse is reported at the value, for the attached property.
#[test]
fn a_bad_attached_value_is_reported() {
  let errors = compile(r#"<Border xmlns="https://artic.rs/ui" TextElement.FontSize="big"/>"#).expect_err("a bad value");
  assert!(errors.contains("for `TextElement.FontSize`"), "{errors}");
}

// An `Option` property's setter takes the whole option, so the value is written inside `Some`.
#[test]
fn an_optional_value_is_written_inside_some() {
  let rust = compile(r#"<Surface xmlns="https://artic.rs/ui" Desktop.Width="300"/>"#).expect("a valid view");
  assert!(
    joined(&rust).contains("Desktop::set_width(tree,__e0.id(),::core::option::Option::Some(300.0_f32)"),
    "{rust}"
  );
}

// A property any element may carry is set on the root too.
#[test]
fn a_property_for_any_element_is_set_on_the_root() {
  let rust = compile(r#"<StackPanel xmlns="https://artic.rs/ui" TextElement.FontSize="20"/>"#).expect("a valid view");
  assert!(
    joined(&rust).contains("TextElement::set_font_size(tree,__e0.id(),20.0_f32);"),
    "{rust}"
  );
}

// `Owner.Name` on an element of the owner's own type sets the element's property when the type
// has one of that name, whatever prefix the owner is written with.
#[test]
fn own_type_dotted_member_is_the_property() {
  let rust = compile(
    r#"<a:Border xmlns:a="https://artic.rs/ui" xmlns:b="https://artic.rs/ui"><b:Border.Padding>2</b:Border.Padding></a:Border>"#,
  )
  .expect("a valid view");
  assert!(joined(&rust).contains(".padding("), "{rust}");
}

// An attribute and a property element of the element's own type set one property, so setting
// both is "set twice".
#[test]
fn own_property_set_by_both_forms_is_set_twice() {
  let errors = compile(r#"<Border xmlns="https://artic.rs/ui" Padding="1"><Border.Padding>2</Border.Padding></Border>"#)
    .expect_err("twice");
  assert!(errors.contains("`Padding` is set twice"), "{errors}");
}

// A dotted member of the element's own type that names nothing is reported as its property.
#[test]
fn own_type_unknown_member_is_an_unknown_property() {
  let errors = compile(r#"<Border xmlns="https://artic.rs/ui" Border.Bogus="1"/>"#).expect_err("unknown");
  assert!(errors.contains("`Border` has no property `Bogus`"), "{errors}");
}

// A code type at the root is built as a local when an attached property is set on it, so the
// setter is called on it before `build` returns it.
#[test]
fn a_code_type_root_takes_its_attached_properties() {
  let rust = compile(concat!(
    r#"<UserControl xmlns="https://artic.rs/ui" xmlns:x="https://artic.rs/markup""#,
    r#" x:Class="app::views::login::LoginView" TextElement.FontSize="20"><Border/></UserControl>"#,
  ))
  .expect("a valid view");
  assert!(
    joined(&rust).contains(
      "let__e1=crate::views::login::LoginView::new(View{}).child(__e0).build(tree);\
       ::artic::controls::TextElement::set_font_size(tree,__e1.id(),20.0_f32);__e1}"
    ),
    "{rust}"
  );
}
