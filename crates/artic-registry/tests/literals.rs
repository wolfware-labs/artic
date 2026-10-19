// `PropertyInfo::literal`: with the `literals` feature, each derived property writes its text as
// the Rust that builds its value; without it, no property carries a writer, so an app's registry
// keeps none. CI runs this file both ways.

use artic_core::property::Property;
use artic_core::tree::NodeId;
use artic_macros::UiType;
use artic_registry::UiType;

#[derive(Debug, UiType)]
struct Gauge {
  #[ui]
  width: Property<f32>,
  #[ui]
  label: Property<Option<String>>,
  #[ui(content)]
  child: Property<Option<NodeId>>,
}

#[cfg(feature = "literals")]
fn literal(name: &str, text: &str) -> Result<String, String> {
  let info = Gauge::TYPE_INFO
    .properties
    .iter()
    .find(|info| info.name == name)
    .expect("the property exists");
  let write = info.literal.expect("the literals feature sets it");
  let mut out = artic_registry::RustExpr::new();
  write(text, &mut out).map(|()| out.take())
}

// A number writes its literal.
#[cfg(feature = "literals")]
#[test]
fn number_writes_its_literal() {
  assert_eq!(literal("Width", "4"), Ok("4.0_f32".to_owned()));
}

// An `Option<V>` property writes a `V`, which is what its builder method takes.
#[cfg(feature = "literals")]
#[test]
fn option_writes_its_inner_value() {
  assert_eq!(literal("Label", "hi"), Ok(r#"::std::string::String::from("hi")"#.to_owned()));
}

// Text the value type refuses returns its `FromStr` error.
#[cfg(feature = "literals")]
#[test]
fn bad_text_returns_the_parse_error() {
  assert_eq!(literal("Width", "wide"), Err("invalid float literal".to_owned()));
}

// A property that takes an element refuses text.
#[cfg(feature = "literals")]
#[test]
fn element_property_refuses_text() {
  assert_eq!(
    literal("Child", "x"),
    Err("this property takes an element, not text".to_owned())
  );
}

// Without the feature, no property has a writer.
#[cfg(not(feature = "literals"))]
#[test]
fn literal_is_none_without_the_feature() {
  assert!(Gauge::TYPE_INFO.properties.iter().all(|info| info.literal.is_none()));
}
