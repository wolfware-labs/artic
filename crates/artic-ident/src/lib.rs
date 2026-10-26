//! Rust identifier rules artic's code generators share, so a name is converted and checked the
//! same way wherever Rust is written from it.

/// Rust's strict and reserved keywords through the 2024 edition, which a name can't be without
/// `r#`. `self`, `Self`, `crate` and `super` can't be raw identifiers either.
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
/// after a lowercase letter or a digit, and at the last capital of an acronym.
#[must_use]
pub fn snake_case(name: &str) -> String {
  let characters: Vec<char> = name.chars().collect();
  let mut snake = String::with_capacity(name.len() + 4);
  for (index, &character) in characters.iter().enumerate() {
    if character.is_uppercase() && index > 0 {
      let previous = characters[index - 1];
      let after_lowercase = previous.is_lowercase() || previous.is_ascii_digit();
      let starts_word = previous.is_uppercase() && characters.get(index + 1).is_some_and(|next| next.is_lowercase());
      if after_lowercase || starts_word {
        snake.push('_');
      }
    }
    snake.extend(character.to_lowercase());
  }
  snake
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // A word starts at a capital after a lowercase letter or a digit, and at the last capital of an
  // acronym; an underscore the name already has is kept.
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
}
