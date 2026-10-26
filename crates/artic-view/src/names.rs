//! The fields of a view's code that its names give.

/// The field a name gives in a view's code: the name in snake_case, such as `submit_button` for
/// `SubmitButton`, and a keyword as a raw identifier, such as `r#type` for `Type`; or why it
/// can't be one.
///
/// The name is taken as written, so spaces around it make it invalid. Its field, the name in
/// snake_case with each run of underscores inside it written as one, must be a Rust identifier
/// by Unicode's rules, other than `_`. Two names with one field, such as `Email` and `email`,
/// give it twice.
///
/// # Errors
///
/// Returns why the name can't be a field: a field `root`, which the root element's field takes;
/// a field `self`, `crate` or `super`, which can't be raw identifiers; a name written with `r#`;
/// or a name that isn't an identifier.
pub fn field_name(name: &str) -> Result<String, &'static str> {
  if name.starts_with("r#") {
    return Err("write the name without `r#`: a keyword becomes a raw identifier by itself");
  }
  let field = collapse_underscores(&snake_case(name));
  match field.as_str() {
    "root" => Err("`root` is the field of the view's root element"),
    "self" | "crate" | "super" => Err("`self`, `Self`, `crate` and `super` can't be raw identifiers"),
    _ if !is_identifier(&field) => Err("a name must be a Rust identifier"),
    _ if KEYWORDS.contains(&field.as_str()) => Ok(format!("r#{field}")),
    _ => Ok(field),
  }
}

/// The words a name becomes a raw identifier for: Rust's keywords, strict and reserved up to the
/// 2024 edition's `gen`, other than `self`, `crate` and `super`, which [`field_name`] refuses
/// before reaching it.
const KEYWORDS: &[&str] = &[
  "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn", "else", "enum", "extern",
  "false", "final", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override",
  "priv", "pub", "ref", "return", "static", "struct", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use",
  "virtual", "where", "while", "yield",
];

/// Whether `name` is an identifier by Unicode's XID rules, as Rust reads one, other than `_`.
fn is_identifier(name: &str) -> bool {
  let mut chars = name.chars();
  let Some(first) = chars.next() else {
    return false;
  };
  name != "_" && (first == '_' || unicode_ident::is_xid_start(first)) && chars.all(unicode_ident::is_xid_continue)
}

/// `SubmitButton` as `submit_button` and `URLField` as `url_field`: a word starts at a capital
/// after a lowercase letter or a digit, and at the last capital of an acronym.
fn snake_case(name: &str) -> String {
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

/// `field` with each run of underscores inside it written as one, as Rust's snake_case lint
/// asks; leading and trailing underscores are kept.
fn collapse_underscores(field: &str) -> String {
  let inner = field.trim_matches('_');
  let start = field.len() - field.trim_start_matches('_').len();
  let mut collapsed = String::with_capacity(field.len());
  collapsed.push_str(&field[..start]);
  for character in inner.chars() {
    if !(character == '_' && collapsed.ends_with('_')) {
      collapsed.push(character);
    }
  }
  collapsed.push_str(&field[start + inner.len()..]);
  collapsed
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  /// The field the rule built on `syn` gives `name`, which `field_name` replaces.
  fn syn_field(name: &str) -> Option<String> {
    match name {
      "root" | "self" | "Self" | "crate" | "super" => None,
      _ if name.starts_with("r#") => None,
      _ => syn::parse_str::<syn::Ident>(name)
        .or_else(|_| syn::parse_str::<syn::Ident>(&format!("r#{name}")))
        .ok()
        .map(|ident| ident.to_string()),
    }
  }

  // Every keyword, every word `syn` takes as a plain identifier though Rust reserves it in some
  // context, and a few non-ASCII and invalid names give the field `syn` would.
  #[test]
  fn keywords_match_syn() {
    let others = [
      "_",
      "auto",
      "crate",
      "default",
      "macro_rules",
      "raw",
      "root",
      "safe",
      "self",
      "Self",
      "super",
      "union",
      "r#type",
      "é",
      "ℕ",
      "a\u{301}",
      "_a",
      "1a",
      "a-b",
      "",
      "email",
    ];
    for word in KEYWORDS.iter().copied().filter(|word| *word != "gen").chain(others) {
      assert_eq!(field_name(word).ok(), syn_field(word), "{word:?}");
    }
  }

  // `gen`, reserved since Rust 2024 though `syn` still takes it as an identifier, becomes a raw
  // identifier, so the field compiles in a crate on any edition.
  #[test]
  fn gen_is_a_raw_identifier() {
    assert_eq!(field_name("gen"), Ok("r#gen".to_owned()));
  }

  // The rule every snake_case converter in artic follows: a word starts at a capital after a
  // lowercase letter or a digit, and at the last capital of an acronym.
  #[test]
  fn snake_case_matches_the_shared_table() {
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

  // A run of underscores inside a name gives one, so the field passes Rust's snake_case lint;
  // leading and trailing underscores are kept.
  #[test]
  fn underscore_runs_inside_a_name_collapse() {
    assert_eq!(field_name("first__name"), Ok("first_name".to_owned()));
    assert_eq!(field_name("Submit__Button"), Ok("submit_button".to_owned()));
    assert_eq!(field_name("__private__"), Ok("__private__".to_owned()));
  }

  // A name in another casing gives its snake_case field.
  #[test]
  fn pascal_case_name_gives_a_snake_case_field() {
    assert_eq!(field_name("SubmitButton"), Ok("submit_button".to_owned()));
  }

  // A name whose field is `root` is refused, since the root element's field has it.
  #[test]
  fn converted_root_is_refused() {
    for name in ["Root", "ROOT"] {
      assert_eq!(
        field_name(name),
        Err("`root` is the field of the view's root element"),
        "{name}"
      );
    }
  }

  // A name whose field is a keyword gives the raw identifier.
  #[test]
  fn converted_keyword_is_raw() {
    assert_eq!(field_name("Type"), Ok("r#type".to_owned()));
    assert_eq!(field_name("Match"), Ok("r#match".to_owned()));
  }

  // A name whose field is `self`, `crate` or `super` is refused, as those can't be raw.
  #[test]
  fn converted_self_is_refused() {
    for name in ["SELF", "Crate", "Super"] {
      assert_eq!(
        field_name(name),
        Err("`self`, `Self`, `crate` and `super` can't be raw identifiers"),
        "{name}"
      );
    }
  }

  // A name is taken as written: spaces or a comment around an identifier make it invalid, where
  // `syn`'s parser skipped them.
  #[test]
  fn padded_name_is_invalid() {
    for name in [" email ", "email ", "email/*c*/", "email//c"] {
      assert_eq!(field_name(name), Err("a name must be a Rust identifier"), "{name:?}");
    }
  }

  // Each refused name gives today's reason.
  #[test]
  fn refused_names_give_their_reason() {
    assert_eq!(field_name("root"), Err("`root` is the field of the view's root element"));
    assert_eq!(
      field_name("Self"),
      Err("`self`, `Self`, `crate` and `super` can't be raw identifiers")
    );
    assert_eq!(
      field_name("r#type"),
      Err("write the name without `r#`: a keyword becomes a raw identifier by itself")
    );
    assert_eq!(field_name("a-b"), Err("a name must be a Rust identifier"));
  }
}
