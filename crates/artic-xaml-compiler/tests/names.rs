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

// `x:Class`, `x:DataType` and `x:Uid` are accepted and add nothing yet.
#[test]
fn recorded_directives_are_accepted() {
  let rust = compile(
    r#"<Border xmlns="https://artic.rs/ui" xmlns:x="https://artic.rs/markup" x:Class="app::views::Card" x:DataType="app::vm::Card" x:Uid="card"/>"#,
  )
  .expect("a valid view");
  assert!(!rust.contains("app::"), "{rust}");
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
