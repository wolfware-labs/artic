//! The fields of a view's code that its names give.

use artic_ident::{IdentError, rust_identifier, snake_case};

/// The field a name gives in a view's code: the name in snake_case, such as `submit_button` for
/// `SubmitButton`, and a keyword as a raw identifier, such as `r#type` for `Type`; or why it
/// can't be one.
///
/// The name is taken as written, so spaces around it make it invalid. Its field is
/// `artic_ident`'s snake_case form of it, written as `artic_ident::rust_identifier` writes it.
/// Two names with one field, such as `Email` and `email`, give it twice.
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
  let field = snake_case(name);
  if field == "root" {
    return Err("`root` is the field of the view's root element");
  }
  rust_identifier(&field).map_err(|error| match error {
    IdentError::PathKeyword => "`self`, `Self`, `crate` and `super` can't be raw identifiers",
    IdentError::NotIdentifier => "a name must be a Rust identifier",
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use artic_ident::{KEYWORDS, is_path_keyword};
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
    let raw_only = KEYWORDS
      .iter()
      .copied()
      .filter(|word| *word != "gen" && !is_path_keyword(word));
    for word in raw_only.chain(others) {
      assert_eq!(field_name(word).ok(), syn_field(word), "{word:?}");
    }
  }

  // `gen`, reserved since Rust 2024 though `syn` still takes it as an identifier, becomes a raw
  // identifier, so the field compiles in a crate on any edition.
  #[test]
  fn gen_is_a_raw_identifier() {
    assert_eq!(field_name("gen"), Ok("r#gen".to_owned()));
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
