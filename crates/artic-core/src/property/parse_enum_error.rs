/// The error returned when text names none of an enum's values.
///
/// `#[derive(UiEnum)]` returns it from the enum's `FromStr`. The message repeats what was written
/// and lists every valid name, so a typo in markup explains its own fix.
///
/// # Examples
///
/// ```
/// use artic_core::property::ParseEnumError;
///
/// let error = ParseEnumError {
///   value: String::from("diagonal"),
///   type_name: "Orientation",
///   expected: &["Horizontal", "Vertical"],
/// };
/// assert_eq!(
///   error.to_string(),
///   "'diagonal' is not a valid Orientation; expected one of: Horizontal, Vertical"
/// );
/// ```
#[derive(Debug, Clone, Eq, PartialEq, thiserror::Error)]
#[error("'{value}' is not a valid {type_name}; expected one of: {}", expected.join(", "))]
pub struct ParseEnumError {
  /// The text that matched no value.
  pub value: String,
  /// The enum's name, such as `Orientation`.
  pub type_name: &'static str,
  /// Every valid name, in declaration order.
  pub expected: &'static [&'static str],
}
