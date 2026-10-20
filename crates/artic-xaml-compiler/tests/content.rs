// Content: child elements go to the content method after every property, text goes to a content
// property, and content a type can't take is reported.

mod common;

use artic_core::property::Property;
use artic_core::tree::{NodeId, UiTree};
use artic_macros::UiType;
use artic_registry::{UiCollection, ValueError};
use common::{TEST_UI, compile};

/// A type whose content is a text property.
#[derive(Debug, UiType)]
struct Label {
  #[ui(content)]
  text: Property<String>,
}

/// A type whose content is text with its whitespace kept.
#[derive(Debug, UiType)]
#[ui(whitespace = "significant")]
struct Run {
  #[ui(content)]
  text: Property<String>,
}

/// A collection of values, which has no builder method.
#[derive(Debug, Default)]
struct Stops(Vec<NodeId>);

impl UiCollection for Stops {
  fn push_child(
    tree: &mut UiTree,
    owner: NodeId,
    child: NodeId,
    field: for<'a> fn(&'a mut UiTree, NodeId) -> Option<&'a mut Self>,
  ) -> Result<(), ValueError> {
    field(tree, owner).ok_or(ValueError::WrongNode)?.0.push(child);
    Ok(())
  }
}

/// A type whose content is a value collection.
#[derive(Debug, UiType)]
struct Gradient {
  #[ui(content)]
  stops: Stops,
}

/// `rust` without whitespace, so assertions don't depend on where the formatter breaks lines.
fn joined(rust: &str) -> String {
  rust.split_whitespace().collect()
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

// A type whose whitespace is significant gets its text as written.
#[test]
fn significant_content_keeps_raw_text() {
  let rust = compile(&format!(r#"<Run xmlns="{TEST_UI}">  two  spaces </Run>"#)).expect("a valid view");
  assert!(rust.contains(r#""  two  spaces ""#), "{rust}");
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
  let errors = compile(r#"<TextBlock xmlns="https://artic.rs/ui">Hi</TextBlock>"#).expect_err("no content");
  assert!(errors.contains("`TextBlock` takes no text content"), "{errors}");
}

// An element inside a type with no content is reported.
#[test]
fn element_in_type_without_content_reports_error() {
  let errors = compile(r#"<TextBlock xmlns="https://artic.rs/ui"><Border/></TextBlock>"#).expect_err("no content");
  assert!(errors.contains("`TextBlock` takes no child elements"), "{errors}");
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

// A value collection comes with #57.
#[test]
fn value_collection_reports_error() {
  let errors = compile(&format!(r#"<Gradient xmlns="{TEST_UI}"><Label/></Gradient>"#)).expect_err("values");
  assert!(errors.contains("value collections come with #57"), "{errors}");
}

// An object inside a content property comes with #57.
#[test]
fn object_in_content_property_reports_error() {
  let errors = compile(&format!(r#"<Label xmlns="{TEST_UI}"><Label/></Label>"#)).expect_err("a value object");
  assert!(errors.contains("value objects come with #57"), "{errors}");
}
