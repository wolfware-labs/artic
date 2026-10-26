//! Rust identifier rules artic's code generators share, so a name is converted and checked the
//! same way wherever Rust is written from it.

/// Rust's strict and reserved keywords through the 2024 edition, which a name can't be without
/// `r#`; [`is_path_keyword`] tells which of them can't be raw either. A new edition's reserved
/// words join the list, as `gen` did: no test can tell that one is missing.
pub const KEYWORDS: &[&str] = &[
  "Self", "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate", "do", "dyn", "else",
  "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "macro", "match", "mod", "move",
  "mut", "override", "priv", "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true", "try", "type",
  "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

/// Whether `word` is one of Rust's [`KEYWORDS`].
#[must_use]
pub fn is_keyword(word: &str) -> bool {
  KEYWORDS.contains(&word)
}

/// Whether `text` is an identifier by Unicode's XID rules, as Rust reads one, other than `_`.
/// A keyword is an identifier here; [`is_keyword`] tells them apart.
#[must_use]
pub fn is_identifier(text: &str) -> bool {
  let mut chars = text.chars();
  let Some(first) = chars.next() else {
    return false;
  };
  text != "_" && (first == '_' || unicode_ident::is_xid_start(first)) && chars.all(unicode_ident::is_xid_continue)
}

/// `SubmitButton` as `submit_button` and `URLField` as `url_field`: a word starts at a capital
/// after a lowercase letter or a digit, and at the last capital of an acronym. A run of
/// underscores inside the name becomes one, as Rust's `non_snake_case` lint asks; leading and
/// trailing underscores are kept.
#[must_use]
pub fn snake_case(name: &str) -> String {
  let characters: Vec<char> = name.chars().collect();
  let leading = characters.iter().take_while(|&&character| character == '_').count();
  let trailing = characters.iter().rev().take_while(|&&character| character == '_').count();
  let mut snake = String::with_capacity(name.len() + 4);
  for (index, &character) in characters.iter().enumerate() {
    let inside = index >= leading && index < characters.len() - trailing;
    if character == '_' && inside && snake.ends_with('_') {
      continue;
    }
    if character.is_uppercase() && index > 0 {
      let previous = characters[index - 1];
      let after_lowercase = previous.is_lowercase() || previous.is_numeric();
      let starts_word = previous.is_uppercase() && characters.get(index + 1).is_some_and(|next| next.is_lowercase());
      if after_lowercase || starts_word {
        snake.push('_');
      }
    }
    snake.extend(character.to_lowercase());
  }
  snake
}

/// `ColumnSpan` as `COLUMN_SPAN`: the [`snake_case`] name in capitals, for a constant.
#[must_use]
pub fn screaming_snake_case(name: &str) -> String {
  snake_case(name).to_uppercase()
}

/// `corner_radius` as `CornerRadius`: each word of a snake_case name capitalized, the underscores
/// dropped. A raw identifier's `r#` isn't part of its name, so `r#type` gives `Type`.
#[must_use]
pub fn pascal_case(name: &str) -> String {
  name
    .strip_prefix("r#")
    .unwrap_or(name)
    .split('_')
    .filter(|word| !word.is_empty())
    .map(|word| {
      let mut chars = word.chars();
      chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect::<String>())
        .unwrap_or_default()
    })
    .collect()
}

/// Whether `word` is `self`, `Self`, `crate` or `super`: the keywords that start a path, and so
/// the ones that can't be raw identifiers.
#[must_use]
pub fn is_path_keyword(word: &str) -> bool {
  matches!(word, "self" | "Self" | "crate" | "super")
}

/// Why a text can't be written as a Rust identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentError {
  /// It isn't an identifier by Unicode's XID rules, or it is `_` alone.
  NotIdentifier,
  /// It is `self`, `Self`, `crate` or `super`, which can't be raw identifiers.
  PathKeyword,
}

/// `text` as Rust writes it as an identifier: as it is, or a keyword as a raw identifier, such as
/// `r#type` for `type`.
///
/// # Errors
///
/// Returns why `text` can't be an identifier: it isn't one, or it is a path keyword.
pub fn rust_identifier(text: &str) -> Result<String, IdentError> {
  if is_path_keyword(text) {
    Err(IdentError::PathKeyword)
  } else if !is_identifier(text) {
    Err(IdentError::NotIdentifier)
  } else if is_keyword(text) {
    Ok(format!("r#{text}"))
  } else {
    Ok(text.to_owned())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // A word starts at a capital after a lowercase letter or a digit, and at the last capital of an
  // acronym; an underscore the name already has is kept, a run of them inside the name as one.
  #[test]
  fn snake_case_matches_the_table() {
    let table = [
      ("MyApp", "my_app"),
      ("XMLParser", "xml_parser"),
      ("Views", "views"),
      ("V2Controls", "v2_controls"),
      ("io", "io"),
      ("SubmitButton", "submit_button"),
      ("URLField", "url_field"),
      ("ZIndex", "z_index"),
      ("Email", "email"),
      ("Submit_Button", "submit_button"),
      ("first__name", "first_name"),
      ("Row__Span", "row_span"),
      ("__private__", "__private__"),
      ("a\u{663}B", "a\u{663}_b"),
    ];
    for (name, snake) in table {
      assert_eq!(snake_case(name), snake, "{name}");
    }
  }

  // An identifier follows Unicode's XID rules, as Rust reads one, and isn't `_` alone.
  #[test]
  fn identifiers() {
    for text in ["app", "_private", "v2", "é", "ℕ", "a\u{301}"] {
      assert!(is_identifier(text), "{text:?}");
    }
    for text in ["_", "2d", "my-app", "Vec<T>", " views", "", "a\u{2E2F}"] {
      assert!(!is_identifier(text), "{text:?}");
    }
  }

  // Every keyword is one `syn` refuses as a plain identifier and takes as a raw one, except
  // `self`, `Self`, `crate` and `super`, which it refuses both ways, and `gen`, reserved since
  // Rust 2024, which `syn` still takes plain.
  #[test]
  fn keywords_are_rust_keywords() {
    let plain = |word: &str| syn::parse_str::<syn::Ident>(word).is_ok();
    let raw = |word: &str| syn::parse_str::<syn::Ident>(&format!("r#{word}")).is_ok();
    for &word in KEYWORDS {
      match word {
        "self" | "Self" | "crate" | "super" => assert!(!plain(word) && !raw(word), "{word}"),
        "gen" => assert!(plain(word) && raw(word), "{word}"),
        _ => assert!(!plain(word) && raw(word), "{word}"),
      }
    }
  }

  // `is_keyword` answers for the list and nothing else.
  #[test]
  fn is_keyword_matches_the_list() {
    assert!(is_keyword("type") && is_keyword("Self") && is_keyword("gen"));
    assert!(!is_keyword("Type") && !is_keyword("union") && !is_keyword("email"));
  }

  // A constant's name is the snake_case name in capitals.
  #[test]
  fn screaming_snake_case_is_snake_case_in_capitals() {
    assert_eq!(screaming_snake_case("ColumnSpan"), "COLUMN_SPAN");
    assert_eq!(screaming_snake_case("corner_radius"), "CORNER_RADIUS");
  }

  // A snake_case name in PascalCase: each word capitalized, underscores dropped.
  #[test]
  fn pascal_case_capitalizes_each_word() {
    assert_eq!(pascal_case("corner_radius"), "CornerRadius");
    assert_eq!(pascal_case("font__size"), "FontSize");
    assert_eq!(pascal_case("width"), "Width");
    assert_eq!(pascal_case("r#type"), "Type");
  }

  // `self`, `Self`, `crate` and `super` start a path, and are the keywords that can't be raw.
  #[test]
  fn path_keywords() {
    for word in ["self", "Self", "crate", "super"] {
      assert!(is_path_keyword(word), "{word}");
    }
    assert!(!is_path_keyword("type") && !is_path_keyword("Crate"));
  }

  // An identifier is written as it is, a keyword as a raw identifier; a path keyword or a
  // non-identifier can't be one.
  #[test]
  fn rust_identifier_writes_raw_keywords() {
    assert_eq!(rust_identifier("email"), Ok("email".to_owned()));
    assert_eq!(rust_identifier("type"), Ok("r#type".to_owned()));
    assert_eq!(rust_identifier("gen"), Ok("r#gen".to_owned()));
    for word in ["self", "Self", "crate", "super"] {
      assert_eq!(rust_identifier(word), Err(IdentError::PathKeyword), "{word}");
    }
    for text in ["_", "a-b", ""] {
      assert_eq!(rust_identifier(text), Err(IdentError::NotIdentifier), "{text:?}");
    }
  }
}
