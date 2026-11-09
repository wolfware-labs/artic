// `x:Name` gives the `View` a field typed by its element; a name that can't be a field is reported.

mod common;

use common::compile;

/// A view whose `Border` child is named `name`.
fn named(name: &str) -> Result<String, String> {
  compile(&format!(
    r#"<StackPanel xmlns="https://artic.rs/ui" xmlns:x="https://artic.rs/markup"><Border x:Name="{name}"/></StackPanel>"#
  ))
}

// A name becomes a field typed by its element, set to the element's local.
#[test]
fn name_becomes_typed_field() {
  let rust = named("card").expect("a valid view");
  let joined: String = rust.split_whitespace().collect();
  assert!(joined.contains("pubcard:Handle<::artic::controls::Border>,"), "{rust}");
  assert!(joined.contains("View{root:__e1,card:__e0}"), "{rust}");
}

// A keyword becomes a raw identifier.
#[test]
fn keyword_name_is_raw() {
  let rust = named("type").expect("a valid view");
  assert!(rust.contains("pub r#type: Handle<"), "{rust}");
}

// A name in PascalCase gives its snake_case field, and the struct needs no `allow`.
#[test]
fn pascal_case_name_gives_a_snake_case_field() {
  let rust = named("SubmitButton").expect("a valid view");
  assert!(rust.contains("pub submit_button: Handle<"), "{rust}");
  assert!(!rust.contains("non_snake_case"), "{rust}");
}

// Two spellings of one field name two elements.
#[test]
fn two_spellings_of_one_field_report_error() {
  let errors = compile(
    r#"<StackPanel xmlns="https://artic.rs/ui" xmlns:x="https://artic.rs/markup"><Border x:Name="email"/><Border x:Name="Email"/></StackPanel>"#,
  )
  .expect_err("one field twice");
  assert!(errors.contains("`Email` names two elements"), "{errors}");
  assert!(errors.contains("`email` and `Email` both give the field `email`"), "{errors}");
}

// Names that can't be fields are reported, each with why.
#[test]
fn invalid_names_report_errors() {
  for name in [
    "self",
    "Self",
    "SELF",
    "crate",
    "super",
    "root",
    "Root",
    "two words",
    "1st",
    "r#type",
    "_",
  ] {
    let errors = named(name).expect_err(name);
    assert!(errors.contains(&format!("`{name}` isn't a valid name")), "{name}: {errors}");
  }
}

// A name given twice is reported, pointing at both.
#[test]
fn duplicate_name_reports_error() {
  let errors = compile(
    r#"<StackPanel xmlns="https://artic.rs/ui" xmlns:x="https://artic.rs/markup"><Border x:Name="a"/><Border x:Name="a"/></StackPanel>"#,
  )
  .expect_err("two a's");
  assert!(errors.contains("`a` names two elements"), "{errors}");
}

// `x:DataType` and `x:Uid` are accepted and add nothing yet.
#[test]
fn recorded_directives_are_accepted() {
  let rust = compile(
    r#"<Border xmlns="https://artic.rs/ui" xmlns:x="https://artic.rs/markup" x:DataType="app::vm::Card" x:Uid="card"/>"#,
  )
  .expect("a valid view");
  assert!(!rust.contains("app::"), "{rust}");
}

/// A view whose root is `root` with `attributes`, around a `StackPanel` holding `child`.
fn view(root: &str, attributes: &str, child: &str) -> Result<String, String> {
  compile(&format!(
    r#"<{root} xmlns="https://artic.rs/ui" xmlns:x="https://artic.rs/markup" {attributes}><StackPanel>{child}</StackPanel></{root}>"#
  ))
}

// `x:Class` on a `UserControl` root names the view's code type, which the root is built as.
#[test]
fn class_names_the_code_type() {
  let rust = view(
    "UserControl",
    r#"x:Class="app::views::login::LoginView""#,
    r#"<Border x:Name="card"/>"#,
  )
  .expect("a valid view");
  let joined: String = rust.split_whitespace().collect();
  assert!(
    joined.contains(r#"CODE_TYPE:Option<&str>=Some("app::views::login::LoginView");"#),
    "{rust}"
  );
  assert!(joined.contains("->Handle<crate::views::login::LoginView>{"), "{rust}");
  assert!(joined.contains("pub(super)card:Handle<::artic::controls::Border>,"), "{rust}");
  assert!(joined.contains("pubtypeRoot=::artic::controls::UserControl;"), "{rust}");
  assert!(
    joined.contains(
      "pubstructMembers{///Theroot'scontent,whichthecodetypetakesintheroot'splace.pubchild:::artic::element::Child,}"
    ),
    "{rust}"
  );
}

// `x:Class` below the root is reported at its value.
#[test]
fn class_below_the_root_is_reported() {
  let errors = view("UserControl", "", r#"<UserControl x:Class="app::views::Card"/>"#).expect_err("not the root");
  assert!(
    errors.contains("only the root element can name the view's code type"),
    "{errors}"
  );
}

// `x:Class` on a root a code type can't take the place of is reported.
#[test]
fn class_on_a_border_root_is_reported() {
  let errors = view("Border", r#"x:Class="app::views::Card""#, "").expect_err("not a user control");
  assert!(
    errors.contains("`Border` can't be the root of a view with a code type"),
    "{errors}"
  );
}

// `x:FieldModifier` sets the visibility of the field `x:Name` gives.
#[test]
fn field_modifier_sets_the_field_visibility() {
  let rust = named_with(r#"x:Name="card" x:FieldModifier="pub(crate)""#).expect("a valid view");
  let joined: String = rust.split_whitespace().collect();
  assert!(joined.contains("pub(crate)card:Handle<::artic::controls::Border>,"), "{rust}");
}

// `x:FieldModifier` on an element without `x:Name`, or with a value other than `pub` and
// `pub(crate)`, is reported.
#[test]
fn field_modifier_problems_are_reported() {
  let unnamed = named_with(r#"x:FieldModifier="pub""#).expect_err("no name");
  let internal = named_with(r#"x:Name="card" x:FieldModifier="internal""#).expect_err("not a visibility");
  assert!(
    unnamed.contains("a field visibility needs a name on its element"),
    "{unnamed}"
  );
  assert!(internal.contains("`internal` isn't a field visibility"), "{internal}");
}

/// A view whose `Border` child has `attributes`.
fn named_with(attributes: &str) -> Result<String, String> {
  compile(&format!(
    r#"<StackPanel xmlns="https://artic.rs/ui" xmlns:x="https://artic.rs/markup"><Border {attributes}/></StackPanel>"#
  ))
}

// A name is taken as written: spaces or a comment around it make it invalid rather than being
// dropped from the field.
#[test]
fn padded_name_is_invalid() {
  for name in [" email ", "email/*c*/"] {
    let errors = named(name).expect_err(name);
    assert!(errors.contains(&format!("`{name}` isn't a valid name")), "{name}: {errors}");
  }
}
