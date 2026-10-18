//! Font families, weights and styles, and parsing them from text.

use artic_macros::UiEnum;
use std::str::FromStr;
use std::sync::Arc;

/// A list of font families, tried in order: CSS's `font-family`.
///
/// It parses from a comma-separated list such as `'Segoe UI', Inter, sans-serif`. Each name is
/// quoted with `'` or `"`, or not. An unquoted name that is a generic keyword, ignoring case, is
/// that [`GenericFamily`]; a quoted one is a font with that name. The default is `system-ui`.
///
/// # Examples
///
/// ```
/// use artic_layout::text::{FamilyName, FontFamily, GenericFamily};
///
/// let family: FontFamily = "'Segoe UI', sans-serif".parse().unwrap();
/// assert_eq!(
///   family.names(),
///   [FamilyName::named("Segoe UI"), FamilyName::Generic(GenericFamily::SansSerif)]
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontFamily(Arc<[FamilyName]>);

impl FontFamily {
  /// The families, in the order they're tried.
  #[must_use]
  pub fn names(&self) -> &[FamilyName] {
    &self.0
  }
}

impl Default for FontFamily {
  /// `system-ui`.
  fn default() -> FontFamily {
    FontFamily::from(GenericFamily::SystemUi)
  }
}

impl From<GenericFamily> for FontFamily {
  /// A list of that generic family alone.
  fn from(generic: GenericFamily) -> FontFamily {
    FontFamily(Arc::from([FamilyName::Generic(generic)]))
  }
}

/// One entry of a [`FontFamily`]: a font's name or a generic family.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FamilyName {
  /// A font family's name, such as `Inter`.
  Named(Arc<str>),
  /// A generic family, which the text engine maps to one of its fonts.
  Generic(GenericFamily),
}

impl FamilyName {
  /// The family named `name`.
  #[must_use]
  pub fn named(name: &str) -> FamilyName {
    FamilyName::Named(Arc::from(name))
  }
}

/// A CSS generic font family: a kind of font, which the text engine maps to one of its fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenericFamily {
  /// `serif`: strokes end in small flourishes.
  Serif,
  /// `sans-serif`: plain stroke ends.
  SansSerif,
  /// `monospace`: every glyph is as wide as the others.
  Monospace,
  /// `cursive`: joined, handwritten strokes.
  Cursive,
  /// `fantasy`: decorative.
  Fantasy,
  /// `system-ui`: the desktop's own interface font.
  SystemUi,
  /// `emoji`: color emoji.
  Emoji,
  /// `math`: mathematical notation.
  Math,
}

impl GenericFamily {
  /// Every generic family, in CSS's order.
  const ALL: [GenericFamily; 8] = [
    GenericFamily::Serif,
    GenericFamily::SansSerif,
    GenericFamily::Monospace,
    GenericFamily::Cursive,
    GenericFamily::Fantasy,
    GenericFamily::SystemUi,
    GenericFamily::Emoji,
    GenericFamily::Math,
  ];

  /// The family's CSS keyword, such as `sans-serif`.
  #[must_use]
  pub const fn keyword(self) -> &'static str {
    match self {
      GenericFamily::Serif => "serif",
      GenericFamily::SansSerif => "sans-serif",
      GenericFamily::Monospace => "monospace",
      GenericFamily::Cursive => "cursive",
      GenericFamily::Fantasy => "fantasy",
      GenericFamily::SystemUi => "system-ui",
      GenericFamily::Emoji => "emoji",
      GenericFamily::Math => "math",
    }
  }
}

/// The error returned when text isn't a valid font family list.
///
/// The message repeats the text, says what is wrong, and shows the accepted forms.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid FontFamily '{text}': {problem}; expected forms like 'Inter, sans-serif' or 'system-ui'")]
pub struct ParseFontFamilyError {
  /// The text that failed to parse.
  pub text: String,
  /// What is wrong with it.
  pub problem: FamilyProblem,
}

/// What is wrong with font family text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FamilyProblem {
  /// The text is empty, or only whitespace.
  #[error("it's empty")]
  Empty,
  /// A name between commas is empty, as in `Inter,,Arial`, or a quoted name is, as in `''`.
  #[error("a name is empty")]
  EmptyName,
  /// A quote isn't closed, as in `'Segoe UI`.
  #[error("a quote isn't closed")]
  UnclosedQuote,
  /// A name is only partly quoted, as `'Segoe' UI` and `Arial "Black"` are: a name is quoted
  /// whole or not at all.
  #[error("'{0}' is partly quoted")]
  PartlyQuoted(String),
}

impl FromStr for FontFamily {
  type Err = ParseFontFamilyError;

  /// Parses a comma-separated list of names, each quoted with `'` or `"` or not. Whitespace around
  /// a name is ignored, and whitespace inside an unquoted name becomes one space.
  fn from_str(text: &str) -> Result<FontFamily, ParseFontFamilyError> {
    parse_family(text)
      .map(|names| FontFamily(names.into()))
      .map_err(|problem| ParseFontFamilyError {
        text: text.to_owned(),
        problem,
      })
  }
}

fn parse_family(text: &str) -> Result<Vec<FamilyName>, FamilyProblem> {
  if text.trim().is_empty() {
    return Err(FamilyProblem::Empty);
  }
  split_names(text)?.into_iter().map(parse_name).collect()
}

fn split_names(text: &str) -> Result<Vec<&str>, FamilyProblem> {
  let mut names = Vec::new();
  let mut start = 0;
  let mut open = None;
  for (at, character) in text.char_indices() {
    match (open, character) {
      (Some(quote), _) if character == quote => open = None,
      (None, '\'' | '"') => open = Some(character),
      (None, ',') => {
        names.push(&text[start..at]);
        start = at + 1;
      }
      _ => {}
    }
  }
  if open.is_some() {
    return Err(FamilyProblem::UnclosedQuote);
  }
  names.push(&text[start..]);
  Ok(names)
}

fn parse_name(text: &str) -> Result<FamilyName, FamilyProblem> {
  let text = text.trim();
  if let Some(quote) = text.chars().next().filter(|first| matches!(first, '\'' | '"')) {
    return match text[1..].split_once(quote) {
      Some((name, "")) if name.trim().is_empty() => Err(FamilyProblem::EmptyName),
      Some((name, "")) => Ok(FamilyName::named(name)),
      _ => Err(FamilyProblem::PartlyQuoted(text.to_owned())),
    };
  }
  if text.contains(['\'', '"']) {
    return Err(FamilyProblem::PartlyQuoted(text.to_owned()));
  }
  let name = text.split_whitespace().collect::<Vec<_>>().join(" ");
  if name.is_empty() {
    return Err(FamilyProblem::EmptyName);
  }
  let generic = GenericFamily::ALL
    .into_iter()
    .find(|generic| generic.keyword().eq_ignore_ascii_case(&name));
  Ok(generic.map_or_else(|| FamilyName::named(&name), FamilyName::Generic))
}

/// How heavy a font's strokes are, from 1 to 1000, as in CSS: 400 is normal and 700 bold.
///
/// It parses from a whole number or from one of WPF's `FontWeights` names, ignoring case: `Thin`
/// 100, `ExtraLight` and `UltraLight` 200, `Light` 300, `Normal` and `Regular` 400, `Medium` 500,
/// `SemiBold` and `DemiBold` 600, `Bold` 700, `ExtraBold` and `UltraBold` 800, `Black` and `Heavy`
/// 900, `ExtraBlack` and `UltraBlack` 950. A family without a face of the weight asked for uses
/// its nearest face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontWeight(u16);

impl FontWeight {
  /// 100.
  pub const THIN: FontWeight = FontWeight(100);
  /// 200.
  pub const EXTRA_LIGHT: FontWeight = FontWeight(200);
  /// 300.
  pub const LIGHT: FontWeight = FontWeight(300);
  /// 400, the default.
  pub const NORMAL: FontWeight = FontWeight(400);
  /// 500.
  pub const MEDIUM: FontWeight = FontWeight(500);
  /// 600.
  pub const SEMI_BOLD: FontWeight = FontWeight(600);
  /// 700.
  pub const BOLD: FontWeight = FontWeight(700);
  /// 800.
  pub const EXTRA_BOLD: FontWeight = FontWeight(800);
  /// 900.
  pub const BLACK: FontWeight = FontWeight(900);
  /// 950.
  pub const EXTRA_BLACK: FontWeight = FontWeight(950);

  /// The weight `value`, or `None` outside 1 to 1000.
  #[must_use]
  pub const fn new(value: u16) -> Option<FontWeight> {
    if value >= 1 && value <= 1000 {
      Some(FontWeight(value))
    } else {
      None
    }
  }

  /// The weight as a number from 1 to 1000.
  #[must_use]
  pub const fn value(self) -> u16 {
    self.0
  }
}

impl Default for FontWeight {
  /// [`FontWeight::NORMAL`].
  fn default() -> FontWeight {
    FontWeight::NORMAL
  }
}

const WEIGHT_NAMES: [(&str, FontWeight); 16] = [
  ("Thin", FontWeight::THIN),
  ("ExtraLight", FontWeight::EXTRA_LIGHT),
  ("UltraLight", FontWeight::EXTRA_LIGHT),
  ("Light", FontWeight::LIGHT),
  ("Normal", FontWeight::NORMAL),
  ("Regular", FontWeight::NORMAL),
  ("Medium", FontWeight::MEDIUM),
  ("SemiBold", FontWeight::SEMI_BOLD),
  ("DemiBold", FontWeight::SEMI_BOLD),
  ("Bold", FontWeight::BOLD),
  ("ExtraBold", FontWeight::EXTRA_BOLD),
  ("UltraBold", FontWeight::EXTRA_BOLD),
  ("Black", FontWeight::BLACK),
  ("Heavy", FontWeight::BLACK),
  ("ExtraBlack", FontWeight::EXTRA_BLACK),
  ("UltraBlack", FontWeight::EXTRA_BLACK),
];

/// The error returned when text isn't a valid font weight.
///
/// The message repeats the text, says what is wrong, and shows the accepted forms.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid FontWeight '{text}': {problem}; expected a number from 1 to 1000 or a name like 'Bold'")]
pub struct ParseFontWeightError {
  /// The text that failed to parse.
  pub text: String,
  /// What is wrong with it.
  pub problem: WeightProblem,
}

/// What is wrong with font weight text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WeightProblem {
  /// The text is neither a number nor one of the names [`FontWeight`] lists.
  #[error("it isn't a number or a weight's name")]
  UnknownName,
  /// The text is a number, but not a whole one from 1 to 1000.
  #[error("it isn't a whole number from 1 to 1000")]
  OutOfRange,
}

impl FromStr for FontWeight {
  type Err = ParseFontWeightError;

  /// Parses a whole number from 1 to 1000, written in decimal with an optional sign and a
  /// fraction of zeros, or a name [`FontWeight`] lists, ignoring case and the whitespace around it.
  fn from_str(text: &str) -> Result<FontWeight, ParseFontWeightError> {
    parse_weight(text.trim()).map_err(|problem| ParseFontWeightError {
      text: text.to_owned(),
      problem,
    })
  }
}

fn parse_weight(text: &str) -> Result<FontWeight, WeightProblem> {
  if let Some(&(_, weight)) = WEIGHT_NAMES.iter().find(|(name, _)| name.eq_ignore_ascii_case(text)) {
    return Ok(weight);
  }
  let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
  let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
  let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
  if whole.is_empty() || !digits(whole) || !digits(fraction) {
    return Err(WeightProblem::UnknownName);
  }
  if text.starts_with('-') || fraction.bytes().any(|byte| byte != b'0') {
    return Err(WeightProblem::OutOfRange);
  }
  whole
    .parse::<u16>()
    .ok()
    .and_then(FontWeight::new)
    .ok_or(WeightProblem::OutOfRange)
}

/// Whether a font's glyphs are upright or slanted.
///
/// A family without an italic face slants its upright one.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, UiEnum)]
pub enum FontStyle {
  /// Upright.
  #[default]
  Normal,
  /// The family's italic face: a slanted, often more cursive design.
  Italic,
  /// The family's upright face, slanted.
  Oblique,
}

#[cfg(test)]
mod tests {
  use super::*;
  use FamilyName::Generic;
  use GenericFamily::{SansSerif, Serif, SystemUi};
  use pretty_assertions::assert_eq;

  fn names(text: &str) -> Vec<FamilyName> {
    text.parse::<FontFamily>().unwrap().names().to_vec()
  }

  fn problem(text: &str) -> FamilyProblem {
    text.parse::<FontFamily>().unwrap_err().problem
  }

  fn named(name: &str) -> FamilyName {
    FamilyName::named(name)
  }

  fn weight(text: &str) -> Result<u16, WeightProblem> {
    text
      .parse::<FontWeight>()
      .map(FontWeight::value)
      .map_err(|error| error.problem)
  }

  // WPF's names and their aliases parse ignoring case, and whole numbers parse as themselves.
  #[test]
  fn font_weight_names_and_numbers() {
    let cases = [
      ("Thin", 100),
      ("extralight", 200),
      ("UltraLight", 200),
      ("Light", 300),
      ("Normal", 400),
      ("regular", 400),
      ("Medium", 500),
      ("SemiBold", 600),
      ("DEMIBOLD", 600),
      ("Bold", 700),
      ("ExtraBold", 800),
      ("UltraBold", 800),
      ("Black", 900),
      ("Heavy", 900),
      ("ExtraBlack", 950),
      ("UltraBlack", 950),
      ("1", 1),
      (" 450 ", 450),
      ("1000", 1000),
      ("+700", 700),
      ("700.0", 700),
      ("700.", 700),
      ("0700", 700),
    ];
    for (text, value) in cases {
      assert_eq!(weight(text), Ok(value), "{text}");
    }
  }

  // The constants match WPF's weights, the default is normal, and `new` keeps to 1 to 1000.
  #[test]
  fn font_weight_constants_and_range() {
    assert_eq!(FontWeight::default(), FontWeight::NORMAL);
    assert_eq!(FontWeight::BOLD.value(), 700);
    assert_eq!(FontWeight::EXTRA_BLACK.value(), 950);
    assert!(FontWeight::BOLD > FontWeight::NORMAL);
    assert_eq!(FontWeight::new(1).map(FontWeight::value), Some(1));
    assert_eq!(FontWeight::new(0), None);
    assert_eq!(FontWeight::new(1001), None);
  }

  // Text that is neither a number nor a weight's name is `UnknownName`, and the message quotes it.
  #[test]
  fn unknown_weight_name() {
    for text in ["Bolder", "", "semi bold", "NaN", "inf", "7e2", ".5"] {
      assert_eq!(weight(text), Err(WeightProblem::UnknownName), "{text}");
    }
    assert_eq!(
      "Bolder".parse::<FontWeight>().unwrap_err().to_string(),
      "invalid FontWeight 'Bolder': it isn't a number or a weight's name; expected a number from 1 to 1000 or a name like 'Bold'"
    );
  }

  // A number outside 1 to 1000, or one that isn't whole, is `OutOfRange`, and the message quotes it.
  #[test]
  fn weight_out_of_range() {
    for text in [
      "0",
      "1001",
      "-1",
      "-0",
      "700.5",
      "0.99999999",
      "700.00001",
      "999.99999",
      "99999999",
    ] {
      assert_eq!(weight(text), Err(WeightProblem::OutOfRange), "{text}");
    }
    assert_eq!(
      "1001".parse::<FontWeight>().unwrap_err().to_string(),
      "invalid FontWeight '1001': it isn't a whole number from 1 to 1000; expected a number from 1 to 1000 or a name like 'Bold'"
    );
  }

  // `FontStyle` parses through `UiEnum` ignoring case, and is normal by default.
  #[test]
  fn font_style_parses_ignoring_case() {
    assert_eq!("italic".parse::<FontStyle>(), Ok(FontStyle::Italic));
    assert_eq!("OBLIQUE".parse::<FontStyle>(), Ok(FontStyle::Oblique));
    assert_eq!(FontStyle::default(), FontStyle::Normal);
    assert_eq!(
      "slanted".parse::<FontStyle>().unwrap_err().expected,
      ["Normal", "Italic", "Oblique"]
    );
  }

  // Quoted and unquoted names in one list, with a generic keyword recognized when unquoted.
  #[test]
  fn font_family_parses_list() {
    assert_eq!(
      names(r#"'Segoe UI', "Noto Sans", Inter, sans-serif"#),
      vec![named("Segoe UI"), named("Noto Sans"), named("Inter"), Generic(SansSerif)]
    );
  }

  // Generic keywords match ignoring case, and a quoted keyword is a font with that name.
  #[test]
  fn generic_keywords_ignore_case_unless_quoted() {
    assert_eq!(
      names("SERIF, System-UI, 'monospace'"),
      vec![Generic(Serif), Generic(SystemUi), named("monospace")]
    );
  }

  // Every generic keyword parses to its family.
  #[test]
  fn every_generic_keyword_parses() {
    for generic in GenericFamily::ALL {
      assert_eq!(names(generic.keyword()), vec![Generic(generic)]);
    }
  }

  // Whitespace around names is dropped and runs of it inside an unquoted name become one space;
  // a quoted name keeps its spaces and its commas.
  #[test]
  fn whitespace_and_commas_in_names() {
    assert_eq!(names("  Segoe \t UI ,Arial "), vec![named("Segoe UI"), named("Arial")]);
    assert_eq!(names("'A,  B' , C"), vec![named("A,  B"), named("C")]);
  }

  // The default family is system-ui.
  #[test]
  fn default_family_is_system_ui() {
    assert_eq!(FontFamily::default().names(), [Generic(SystemUi)]);
  }

  // Empty or blank text is `Empty`, and the message quotes it and shows the forms.
  #[test]
  fn empty_family_is_empty() {
    assert_eq!(problem(""), FamilyProblem::Empty);
    assert_eq!(
      "  ".parse::<FontFamily>().unwrap_err().to_string(),
      "invalid FontFamily '  ': it's empty; expected forms like 'Inter, sans-serif' or 'system-ui'"
    );
  }

  // A name missing between commas, before or after them, or an empty quoted name, is `EmptyName`.
  #[test]
  fn missing_name_is_empty_name() {
    for text in ["Inter,,Arial", "Inter,", ",Inter", "''", "Inter, \"\""] {
      assert_eq!(problem(text), FamilyProblem::EmptyName, "{text}");
    }
    assert_eq!(
      "Inter,,Arial".parse::<FontFamily>().unwrap_err().to_string(),
      "invalid FontFamily 'Inter,,Arial': a name is empty; expected forms like 'Inter, sans-serif' or 'system-ui'"
    );
  }

  // A quote without its closing quote, even with the other kind of quote later, is `UnclosedQuote`.
  #[test]
  fn open_quote_is_unclosed_quote() {
    assert_eq!(problem("'Segoe UI"), FamilyProblem::UnclosedQuote);
    assert_eq!(problem(r#"Arial, "Segoe UI'"#), FamilyProblem::UnclosedQuote);
    assert_eq!(
      "'Segoe UI".parse::<FontFamily>().unwrap_err().to_string(),
      "invalid FontFamily ''Segoe UI': a quote isn't closed; expected forms like 'Inter, sans-serif' or 'system-ui'"
    );
  }

  // A name with a quote that doesn't wrap all of it is `PartlyQuoted` with the whole name, and a
  // comma inside its quotes doesn't split it.
  #[test]
  fn partly_quoted_name_is_reported() {
    let partly = |name: &str| FamilyProblem::PartlyQuoted(String::from(name));
    assert_eq!(problem("'Segoe' UI , Arial"), partly("'Segoe' UI"));
    assert_eq!(problem(r#"Arial "A, B", Inter"#), partly(r#"Arial "A, B""#));
    assert_eq!(problem("'a' 'b, c'"), partly("'a' 'b, c'"));
    assert_eq!(problem("Arial'"), FamilyProblem::UnclosedQuote);
    assert_eq!(
      "'Segoe' UI".parse::<FontFamily>().unwrap_err().to_string(),
      "invalid FontFamily ''Segoe' UI': ''Segoe' UI' is partly quoted; expected forms like 'Inter, sans-serif' or 'system-ui'"
    );
  }
}
