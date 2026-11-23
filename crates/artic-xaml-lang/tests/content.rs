// Content: child elements go to the content method after every property, text goes to a content
// property, and content a type can't take is reported.

mod common;

use artic_core::property::Property;
use artic_macros::UiType;
use common::{Nodes, TEST_UI, compile, joined};

/// A type that takes no content.
#[derive(Debug, UiType)]
struct Divider;

/// A type whose content is a text property.
#[derive(Debug, UiType)]
struct Label {
  #[ui(content)]
  text: Property<String>,
}

/// A type whose content is a text property and whose whitespace is significant.
#[derive(Debug, UiType)]
#[ui(whitespace = "significant")]
struct Run {
  #[ui(content)]
  text: Property<String>,
}

/// A type whose content is a collection of its own.
#[derive(Debug, UiType)]
struct Gradient {
  #[ui(content)]
  stops: Nodes,
}

/// Each text `rust` sets through a `text` builder method, in order.
fn texts(rust: &str) -> Vec<&str> {
  rust
    .split(".text(::std::string::String::from(\"")
    .skip(1)
    .map(|rest| &rest[..rest.find('"').expect("a closing quote")])
    .collect()
}

// A `Child` content takes the child's handle, after the property methods.
#[test]
fn child_comes_after_properties() {
  let rust =
    compile(r#"<Border xmlns="https://artic.rs/ui" Padding="2"><TextBlock Text="Hi"/></Border>"#).expect("a valid view");
  let border = joined(&rust);
  let border = &border[border.find("let__e1=").expect("the border's let")..];
  assert!(
    border.starts_with("let__e1=::artic::controls::Border::new().padding("),
    "{rust}"
  );
  assert!(border.contains(".child(__e0).build(tree);"), "{rust}");
}

// A `Children` content takes every child's id in document order, each built before its parent.
#[test]
fn children_are_built_first_in_order() {
  let rust = compile(
    r#"<StackPanel xmlns="https://artic.rs/ui"><TextBlock Text="a"/><Border><TextBlock Text="b"/></Border></StackPanel>"#,
  )
  .expect("a valid view");
  let joined = joined(&rust);
  assert!(joined.contains(".children([__e0.id(),__e2.id()]).build(tree);"), "{rust}");
  assert!(joined.contains("View{root:__e3}"), "{rust}");
}

// A panel with nothing inside, or only whitespace, writes no content call.
#[test]
fn empty_panel_writes_no_children_call() {
  for xaml in [
    r#"<StackPanel xmlns="https://artic.rs/ui"/>"#,
    "<StackPanel xmlns=\"https://artic.rs/ui\">\n  \n</StackPanel>",
  ] {
    let rust = compile(xaml).expect("a valid view");
    assert!(!rust.contains(".children("), "{rust}");
  }
}

// Text content sets the content property through its builder method.
#[test]
fn text_content_sets_content_property() {
  let rust = compile(&format!(r#"<Label xmlns="{TEST_UI}">  Hello   world </Label>"#)).expect("a valid view");
  assert!(
    rust.contains(r#".text(::std::string::String::from("Hello world"))"#),
    "{rust}"
  );
}

// A type whose whitespace is significant still collapses its text, as XAML does, and
// `xml:space="preserve"` keeps it as written.
#[test]
fn significant_content_collapses_unless_preserved() {
  let rust = compile(&format!(r#"<Run xmlns="{TEST_UI}">  two  spaces </Run>"#)).expect("a valid view");
  assert!(rust.contains(r#""two spaces""#), "{rust}");
  let rust = compile(&format!(
    r#"<Run xmlns="{TEST_UI}" xml:space="preserve">  two  spaces </Run>"#
  ))
  .expect("a valid view");
  assert!(rust.contains(r#""  two  spaces ""#), "{rust}");
}

// Text in a `TextBlock` becomes runs beside its inlines, with one space kept between two inlines
// and the markup's line breaks and indentation collapsed.
#[test]
fn text_in_a_text_block_becomes_runs() {
  let rust =
    compile("<TextBlock xmlns=\"https://artic.rs/ui\">\n  Hello\n  <Bold>world</Bold>\n  <Italic>again</Italic>\n</TextBlock>")
      .expect("a valid view");
  assert_eq!(texts(&rust), ["Hello ", "world", " ", "again"], "{rust}");
}

// Under `xml:space="preserve"`, text in a `TextBlock` keeps its line breaks and indentation.
#[test]
fn preserved_text_in_a_text_block_keeps_its_whitespace() {
  let rust =
    compile("<TextBlock xmlns=\"https://artic.rs/ui\" xml:space=\"preserve\">\n  Hello <Bold>world</Bold></TextBlock>")
      .expect("a valid view");
  assert!(rust.contains(r#"String::from("\n  Hello ")"#), "{rust}");
}

// Text in a collection with no text wrapper is still reported.
#[test]
fn text_in_a_collection_without_a_wrapper_reports_error() {
  let errors = compile(r#"<StackPanel xmlns="https://artic.rs/ui">Hi</StackPanel>"#).expect_err("text");
  assert!(errors.contains("`StackPanel` takes no text content"), "{errors}");
}

// Blank content of a collapsing type sets nothing.
#[test]
fn blank_content_sets_nothing() {
  let rust = compile(&format!("<Label xmlns=\"{TEST_UI}\">\n  </Label>")).expect("a valid view");
  assert!(!rust.contains(".text("), "{rust}");
}

// A property given as an attribute and as content is set twice.
#[test]
fn property_set_by_attribute_and_content_reports_error() {
  let errors = compile(&format!(r#"<Label xmlns="{TEST_UI}" Text="a">b</Label>"#)).expect_err("set twice");
  assert!(errors.contains("`Text` is set twice"), "{errors}");
}

// Text inside a type with no content is reported.
#[test]
fn text_in_type_without_content_reports_error() {
  let errors = compile(&format!(r#"<Divider xmlns="{TEST_UI}">Hi</Divider>"#)).expect_err("no content");
  assert!(errors.contains("`Divider` takes no text content"), "{errors}");
}

// An element inside a type with no content is reported.
#[test]
fn element_in_type_without_content_reports_error() {
  let errors = compile(&format!(r#"<Divider xmlns="{TEST_UI}"><Divider/></Divider>"#)).expect_err("no content");
  assert!(errors.contains("`Divider` takes no child elements"), "{errors}");
}

// Text in a collection of elements is reported.
#[test]
fn text_in_element_collection_reports_error() {
  let errors = compile(r#"<StackPanel xmlns="https://artic.rs/ui">Hi<Border/></StackPanel>"#).expect_err("text");
  assert!(errors.contains("`StackPanel` takes no text content"), "{errors}");
}

// A second child of a `Child` content is reported, pointing at both.
#[test]
fn second_child_reports_error() {
  let errors = compile(r#"<Border xmlns="https://artic.rs/ui"><Border/><Border/></Border>"#).expect_err("two children");
  assert!(errors.contains("`Border` takes one child"), "{errors}");
}

// A collection of its own takes the children through the method named after its field.
#[test]
fn own_collection_takes_children_by_its_field_name() {
  let rust = compile(&format!(r#"<Gradient xmlns="{TEST_UI}"><Label/></Gradient>"#)).expect("a valid view");
  assert!(joined(&rust).contains(".stops([__e0.id()]).build(tree);"), "{rust}");
}

// An object inside a content property comes with #57.
#[test]
fn object_in_content_property_reports_error() {
  let errors = compile(&format!(r#"<Label xmlns="{TEST_UI}"><Label/></Label>"#)).expect_err("a value object");
  assert!(errors.contains("value objects come with #57"), "{errors}");
}

// Elements inside a type with no content are still checked.
#[test]
fn elements_in_type_without_content_are_checked() {
  let errors = compile(&format!(r#"<Divider xmlns="{TEST_UI}"><Bogus/></Divider>"#)).expect_err("two errors");
  assert!(errors.contains("`Divider` takes no child elements"), "{errors}");
  assert!(errors.contains("no type `Bogus`"), "{errors}");
}

// A child of a type the content doesn't take is reported where it's written, and its own
// problems are still checked.
#[test]
fn child_of_a_type_the_content_does_not_take_reports_error() {
  let errors = compile(r#"<TextBlock xmlns="https://artic.rs/ui"><Run Text="a"/><Border/><Bogus/></TextBlock>"#)
    .expect_err("a border in a text block");
  assert!(
    errors.contains("`TextBlock` takes only `Run`, `Bold` or `Italic`"),
    "{errors}"
  );
  assert!(errors.contains("no type `Bogus`"), "{errors}");
  assert_eq!(errors.matches("takes only").count(), 1, "{errors}");
}

// Indentation before the first inline and after the last adds nothing; between two inlines it is
// one space.
#[test]
fn indentation_around_inlines_is_trimmed() {
  let rust = compile("<TextBlock xmlns=\"https://artic.rs/ui\">\n  <Bold>a</Bold>\n  <Italic>b</Italic>\n</TextBlock>")
    .expect("a valid view");
  assert_eq!(texts(&rust), ["a", " ", "b"], "{rust}");
}
