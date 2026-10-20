//! The `View` fields `x:Name` gives.

use syn::Ident;

/// The field for the name `name`: the name itself, or the raw identifier for a keyword, such as
/// `r#type`. Returns why the name can't be a field otherwise.
pub(crate) fn field_name(name: &str) -> Result<Ident, &'static str> {
  match name {
    "root" => Err("`root` is the field of the view's root element"),
    "self" | "Self" | "crate" | "super" => Err("`self`, `Self`, `crate` and `super` can't be raw identifiers"),
    _ if name.starts_with("r#") => Err("write the name without `r#`: a keyword becomes a raw identifier by itself"),
    _ => syn::parse_str::<Ident>(name)
      .or_else(|_| syn::parse_str::<Ident>(&format!("r#{name}")))
      .map_err(|_| "a name must be a Rust identifier"),
  }
}
