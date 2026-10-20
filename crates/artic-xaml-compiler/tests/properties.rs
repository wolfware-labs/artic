// Attributes and property elements: each property becomes its builder method with the value
// written at build time, a group's through its methods trait; members the compiler doesn't
// support yet name the issue that adds them.

mod common;

use artic_core::event::Event;
use artic_core::property::Property;
use artic_macros::UiType;
use artic_registry::{RustExpr, ToRust};
use common::{TEST_UI, compile};
use std::str::FromStr;

/// What a test event passes its handlers.
#[derive(Debug)]
struct Pressed;

/// A value type whose `ToRust` writes Rust that doesn't parse.
#[derive(Debug, Clone, Default, PartialEq)]
struct Broken;

impl FromStr for Broken {
  type Err = String;

  fn from_str(_: &str) -> Result<Broken, String> {
    Ok(Broken)
  }
}

impl ToRust for Broken {
  fn to_rust(&self, out: &mut RustExpr) {
    out.raw("(");
  }
}

/// A test type with an event and a property of the broken value type.
#[derive(Debug, UiType)]
struct Knob {
  #[ui]
  #[expect(dead_code, reason = "only the registry reads the event, by its name")]
  pressed: Event<Pressed>,
  #[ui]
  shape: Property<Broken>,
}

// A property becomes its builder method, with the literal parsed and written at build time.
#[test]
fn attribute_becomes_builder_call() {
  let rust = compile(r#"<Border xmlns="https://artic.rs/ui" Padding="4,8" CornerRadius="2"/>"#).expect("a valid view");
  let joined: String = rust.split_whitespace().collect();
  assert!(joined.contains(".padding(::artic::layout::Thickness::"), "{rust}");
  assert!(joined.contains(".corner_radius(::artic::layout::CornerRadius::"), "{rust}");
}

// A group's property is called through its methods trait, imported once from its crate's path.
#[test]
fn group_property_imports_its_trait() {
  let rust =
    compile(r#"<Border xmlns="https://artic.rs/ui" Margin="1" HorizontalAlignment="Left"/>"#).expect("a valid view");
  assert!(rust.contains("use ::artic::element::LayoutMethods as _;"), "{rust}");
  assert_eq!(rust.matches("LayoutMethods").count(), 1, "{rust}");
  assert!(rust.contains(".margin("), "{rust}");
}

// Group traits are imported in sorted order, whatever order the document sets them in.
#[test]
fn group_imports_are_sorted() {
  let one = compile(r#"<StackPanel xmlns="https://artic.rs/ui" Margin="1" Background="Red"/>"#).expect("a valid view");
  let other = compile(r#"<StackPanel xmlns="https://artic.rs/ui" Background="Red" Margin="1"/>"#).expect("a valid view");
  let imports = |rust: &str| rust.lines().filter(|line| line.starts_with("use ")).map(str::to_owned).collect::<Vec<_>>();
  assert_eq!(
    imports(&one),
    [
      "use ::artic::prelude::{Handle, UiTree};",
      "use ::artic::controls::PanelMethods as _;",
      "use ::artic::element::LayoutMethods as _;",
    ]
  );
  assert_eq!(imports(&one), imports(&other));
}

// A negative number is written as the method's argument, so unary minus binds to the literal.
#[test]
fn negative_literal_is_an_argument() {
  let rust = compile(r#"<StackPanel xmlns="https://artic.rs/ui" Spacing="-4"/>"#).expect("a valid view");
  assert!(rust.contains(".spacing(-4.0_f32)"), "{rust}");
}

// A property the type and its groups don't have is reported, naming the type.
#[test]
fn unknown_property_reports_error() {
  let errors = compile(r#"<Border xmlns="https://artic.rs/ui" Colour="Red"/>"#).expect_err("no Colour");
  assert!(errors.contains("`Border` has no property `Colour`"), "{errors}");
}

// Text its type's `FromStr` refuses is reported at the attribute with the parser's message.
#[test]
fn bad_literal_reports_error() {
  let errors = compile(r#"<Border xmlns="https://artic.rs/ui" Padding="wide"/>"#).expect_err("not a thickness");
  assert!(errors.contains("for `Padding`"), "{errors}");
}

// A value type whose `ToRust` writes invalid Rust fails the build naming the property.
#[test]
fn invalid_literal_rust_reports_error() {
  let errors = compile(&format!(r#"<Knob xmlns="{TEST_UI}" Shape="x"/>"#)).expect_err("invalid Rust");
  assert!(errors.contains("the value of `Shape` was written as invalid Rust: `(`"), "{errors}");
}

// An event handler comes with #82.
#[test]
fn event_reports_error() {
  let errors = compile(&format!(r#"<Knob xmlns="{TEST_UI}" Pressed="on_press"/>"#)).expect_err("an event");
  assert!(errors.contains("event handlers come with #82"), "{errors}");
}

// An attached property comes with #50.
#[test]
fn attached_property_reports_error() {
  let errors = compile(r#"<Border xmlns="https://artic.rs/ui" TextElement.FontSize="12"/>"#).expect_err("attached");
  assert!(errors.contains("attached properties come with #50"), "{errors}");
}

// Each markup extension names the issue that adds it, and any other is an unknown value provider.
#[test]
fn markup_extensions_report_their_issue() {
  for (value, message) in [
    ("{StaticResource Accent}", "`{StaticResource}` comes with #112"),
    ("{Binding Name}", "`{Binding}` comes with #94"),
    ("{DynamicResource Accent}", "`{DynamicResource}` comes with #113"),
    ("{TemplateBinding Padding}", "`{TemplateBinding}` comes with #85"),
    ("{x:Null}", "`{x:Null}` comes with #62"),
    ("{x:Static a::B}", "`{x:Static}` comes with #62"),
    ("{x:Type Border}", "`{x:Type}` comes with #62"),
    ("{Localize Hello}", "unknown value provider `Localize`"),
  ] {
    let xaml = format!(r#"<Border xmlns="https://artic.rs/ui" xmlns:x="https://artic.rs/markup" Padding="{value}"/>"#);
    let errors = compile(&xaml).expect_err(value);
    assert!(errors.contains(message), "{value}: {errors}");
  }
}

// An object as a property's value comes with #57.
#[test]
fn object_value_reports_error() {
  let errors = compile(
    r#"<Border xmlns="https://artic.rs/ui"><Border.Background><SolidColorBrush/></Border.Background></Border>"#,
  )
  .expect_err("a value object");
  assert!(errors.contains("value objects come with #57"), "{errors}");
}

// A property element holding text sets the property like an attribute.
#[test]
fn property_element_text_becomes_builder_call() {
  let rust = compile(r#"<Border xmlns="https://artic.rs/ui"><Border.Padding>4</Border.Padding></Border>"#)
    .expect("a valid view");
  assert!(rust.contains(".padding("), "{rust}");
}

// `x:Key` outside a resource dictionary comes with #111.
#[test]
fn key_reports_error() {
  let errors =
    compile(r#"<Border xmlns="https://artic.rs/ui" xmlns:x="https://artic.rs/markup" x:Key="card"/>"#).expect_err("a key");
  assert!(errors.contains("resources come with #111"), "{errors}");
}
