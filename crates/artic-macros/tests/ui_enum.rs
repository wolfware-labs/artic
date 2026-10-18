// Integration tests for `#[derive(UiEnum)]`: the generated `FromStr` matches variant names in
// any ASCII case and explains a miss.

use artic_core::property::{ParseEnumError, Property};
use artic_macros::{UiEnum, UiType};
use pretty_assertions::assert_eq;

#[derive(Debug, Clone, Copy, Default, PartialEq, UiEnum)]
enum Orientation {
  #[default]
  Horizontal,
  Vertical,
}

/// A UI type with an enum-valued property, which only compiles if the enum passes the
/// property value check.
#[derive(Debug, UiType)]
struct StackPanel {
  #[ui(affects = layout)]
  orientation: Property<Orientation>,
}

// A variant's name parses whatever its capitalization.
#[test]
fn parses_any_case() {
  for text in ["horizontal", "Horizontal", "HORIZONTAL", "hOrIzOnTaL"] {
    assert_eq!(text.parse::<Orientation>(), Ok(Orientation::Horizontal), "{text}");
  }
  assert_eq!("vertical".parse::<Orientation>(), Ok(Orientation::Vertical));
}

// Text that names no variant returns an error that repeats the text and lists every variant.
#[test]
fn unknown_value_error_lists_variants() {
  let error = "diagonal".parse::<Orientation>().expect_err("not a variant");

  assert_eq!(
    error,
    ParseEnumError {
      value: String::from("diagonal"),
      type_name: "Orientation",
      expected: &["Horizontal", "Vertical"],
    }
  );
  assert_eq!(
    error.to_string(),
    "'diagonal' is not a valid Orientation; expected one of: Horizontal, Vertical"
  );
}

// Only whole names match: a prefix or surrounding spaces don't.
#[test]
fn partial_or_padded_text_does_not_match() {
  assert!("horiz".parse::<Orientation>().is_err());
  assert!(" Horizontal".parse::<Orientation>().is_err());
}

// A derived enum is a valid property value: text parses into it through `FromStr`.
#[test]
fn enum_can_be_a_property_value() {
  let panel = StackPanel::default();

  panel.orientation.set("VERTICAL".parse().expect("a variant"));

  assert_eq!(panel.orientation.get(), Orientation::Vertical);
}
