//! The fields of a view's code that its names give.

/// The field a name gives in a view's code, such as `r#type` for `type`, or why it can't be one.
///
/// A name is a Rust identifier by Unicode's rules, other than `_`; a keyword becomes a raw
/// identifier.
///
/// # Errors
///
/// Returns why the name can't be a field: `root`, which the root element's field takes; `self`,
/// `Self`, `crate` and `super`, which can't be raw identifiers; a name written with `r#`; or a
/// name that isn't an identifier.
pub fn field_name(name: &str) -> Result<String, &'static str> {
  match name {
    "root" => Err("`root` is the field of the view's root element"),
    "self" | "Self" | "crate" | "super" => Err("`self`, `Self`, `crate` and `super` can't be raw identifiers"),
    _ if name.starts_with("r#") => Err("write the name without `r#`: a keyword becomes a raw identifier by itself"),
    _ if !is_identifier(name) => Err("a name must be a Rust identifier"),
    _ if KEYWORDS.contains(&name) => Ok(format!("r#{name}")),
    _ => Ok(name.to_owned()),
  }
}

/// The words a name becomes a raw identifier for: Rust's keywords, strict and reserved, other
/// than `self`, `Self`, `crate` and `super`, which [`field_name`] refuses before reaching it.
const KEYWORDS: &[&str] = &[
  "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn", "else", "enum", "extern",
  "false", "final", "fn", "for", "if", "impl", "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv",
  "pub", "ref", "return", "static", "struct", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use", "virtual",
  "where", "while", "yield",
];

/// Whether `name` is an identifier by Unicode's XID rules, as Rust reads one, other than `_`.
fn is_identifier(name: &str) -> bool {
  let mut chars = name.chars();
  let Some(first) = chars.next() else {
    return false;
  };
  name != "_" && (first == '_' || unicode_ident::is_xid_start(first)) && chars.all(unicode_ident::is_xid_continue)
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
      "gen",
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
    for word in KEYWORDS.iter().copied().chain(others) {
      assert_eq!(field_name(word).ok(), syn_field(word), "{word:?}");
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
