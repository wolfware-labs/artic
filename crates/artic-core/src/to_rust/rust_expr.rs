use crate::ToRust;
use std::collections::BTreeMap;
use std::fmt::Debug;

/// The expression being written, and the paths generated code reaches crates by.
///
/// A string builder: it checks nothing, so the code generator parses what it writes.
#[derive(Debug, Clone, Default)]
pub struct RustExpr {
  text: String,
  paths: BTreeMap<String, String>,
}

impl RustExpr {
  /// An empty expression that reaches every crate at `::<crate>`.
  #[must_use]
  pub fn new() -> RustExpr {
    RustExpr::default()
  }

  /// Generated code reaches `krate` as `path`, such as `artic_layout` as `::artic::layout`.
  pub fn path(&mut self, krate: &str, path: &str) {
    self.paths.insert(krate.to_owned(), path.to_owned());
  }

  /// Writes the path of an item at `path` in crate `krate`, such as `Thickness` in
  /// `artic_layout`, as generated code reaches it: `::artic::layout::Thickness`.
  pub fn type_path(&mut self, krate: &str, path: &str) {
    match self.paths.get(krate) {
      Some(prefix) => self.text.push_str(prefix),
      None => {
        self.text.push_str("::");
        self.text.push_str(krate);
      }
    }
    self.text.push_str("::");
    self.text.push_str(path);
  }

  /// Writes `value` as a literal that reads back to the same bits: `4.0_f32`, `-0.0_f32`,
  /// `f32::INFINITY`, `f32::NAN`, and any other NaN as `f32::from_bits(…)`.
  pub fn f32(&mut self, value: f32) {
    if value.to_bits() == f32::NAN.to_bits() {
      self.raw("f32::NAN");
    } else if value.is_nan() {
      self.raw(&format!("f32::from_bits({}_u32)", value.to_bits()));
    } else if value == f32::INFINITY {
      self.raw("f32::INFINITY");
    } else if value == f32::NEG_INFINITY {
      self.raw("f32::NEG_INFINITY");
    } else {
      self.suffixed(value, "f32");
    }
  }

  /// Writes `value` as a literal that reads back to the same bits, as [`f32`](RustExpr::f32)
  /// does.
  pub fn f64(&mut self, value: f64) {
    if value.to_bits() == f64::NAN.to_bits() {
      self.raw("f64::NAN");
    } else if value.is_nan() {
      self.raw(&format!("f64::from_bits({}_u64)", value.to_bits()));
    } else if value == f64::INFINITY {
      self.raw("f64::INFINITY");
    } else if value == f64::NEG_INFINITY {
      self.raw("f64::NEG_INFINITY");
    } else {
      self.suffixed(value, "f64");
    }
  }

  /// Writes a call to the function at `path` in crate `krate`, each argument written through its
  /// own [`ToRust`]: `::artic_layout::Point::new(1.5_f32, 2.0_f32)`.
  pub fn call(&mut self, krate: &str, path: &str, args: &[&dyn ToRust]) {
    self.type_path(krate, path);
    self.raw("(");
    self.list(args, |out, arg| arg.to_rust(out));
    self.raw(")");
  }

  /// Writes a struct literal of the type at `path` in crate `krate`, each field written through
  /// its own [`ToRust`]: `::artic_render::SolidColorBrush { color: …, opacity: 1.0_f32 }`.
  pub fn struct_literal(&mut self, krate: &str, path: &str, fields: &[(&str, &dyn ToRust)]) {
    self.type_path(krate, path);
    if fields.is_empty() {
      self.raw(" {}");
      return;
    }
    self.raw(" { ");
    self.list(fields, |out, (name, value)| {
      out.raw(name);
      out.raw(": ");
      value.to_rust(out);
    });
    self.raw(" }");
  }

  /// Writes each item with `write`, separated by `, `: the inside of an argument list or an
  /// array literal.
  pub fn list<T>(&mut self, items: impl IntoIterator<Item = T>, mut write: impl FnMut(&mut RustExpr, T)) {
    for (index, item) in items.into_iter().enumerate() {
      if index > 0 {
        self.raw(", ");
      }
      write(self, item);
    }
  }

  /// Writes `text` as an escaped string literal.
  pub fn str(&mut self, text: &str) {
    self.text.push_str(&format!("{text:?}"));
  }

  /// Writes Rust source as is, such as `::new(` or `, `.
  pub fn raw(&mut self, source: &str) {
    self.text.push_str(source);
  }

  /// The expression written so far.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.text
  }

  /// Returns the expression and starts a new one, keeping the paths.
  pub fn take(&mut self) -> String {
    std::mem::take(&mut self.text)
  }

  /// Writes a float's shortest text that reads back exactly, with `suffix`.
  fn suffixed(&mut self, value: impl Debug, suffix: &str) {
    self.text.push_str(&format!("{value:?}_{suffix}"));
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use proptest::prelude::*;

  fn f32_text(value: f32) -> String {
    let mut out = RustExpr::new();
    out.f32(value);
    out.take()
  }

  fn f64_text(value: f64) -> String {
    let mut out = RustExpr::new();
    out.f64(value);
    out.take()
  }

  // An item's crate is reached at `::<crate>` when nothing maps it.
  #[test]
  fn type_path_uses_the_crate_root() {
    let mut out = RustExpr::new();
    out.type_path("artic_layout", "Thickness");
    assert_eq!(out.as_str(), "::artic_layout::Thickness");
  }

  // A mapped crate is reached by its path, such as a facade module.
  #[test]
  fn type_path_uses_a_mapped_path() {
    let mut out = RustExpr::new();
    out.path("artic_layout", "::artic::layout");
    out.type_path("artic_layout", "Thickness");
    assert_eq!(out.as_str(), "::artic::layout::Thickness");
  }

  // `take` returns the text and leaves the map, so one `RustExpr` writes many expressions.
  #[test]
  fn take_keeps_the_paths() {
    let mut out = RustExpr::new();
    out.path("artic_layout", "::artic::layout");
    out.type_path("artic_layout", "Point");
    assert_eq!(out.take(), "::artic::layout::Point");
    out.type_path("artic_layout", "Size");
    assert_eq!(out.as_str(), "::artic::layout::Size");
  }

  // NaN, the infinities and negative zero are written as constants or signed literals.
  #[test]
  fn floats_write_special_values() {
    assert_eq!(f32_text(f32::NAN), "f32::NAN");
    assert_eq!(f32_text(f32::INFINITY), "f32::INFINITY");
    assert_eq!(f32_text(f32::NEG_INFINITY), "f32::NEG_INFINITY");
    assert_eq!(f32_text(-0.0), "-0.0_f32");
    assert_eq!(f32_text(4.0), "4.0_f32");
    assert_eq!(f64_text(-1.5), "-1.5_f64");
  }

  // A NaN other than the constant, such as the one `"-NaN"` parses to, keeps its bits.
  #[test]
  fn other_nans_write_their_bits() {
    let negative = "-NaN".parse::<f32>().expect("a float");
    assert_eq!(f32_text(negative), format!("f32::from_bits({}_u32)", negative.to_bits()));
    let negative = "-NaN".parse::<f64>().expect("a float");
    assert_eq!(f64_text(negative), format!("f64::from_bits({}_u64)", negative.to_bits()));
  }

  proptest! {
    // Every finite f32 is written as a literal that reads back to the same bits.
    #[test]
    fn f32_reads_back_exactly(bits in any::<u32>()) {
      let value = f32::from_bits(bits);
      prop_assume!(value.is_finite());
      let text = f32_text(value);
      let literal = text.strip_suffix("_f32").expect("an f32 suffix");
      prop_assert_eq!(literal.parse::<f32>().expect("a float").to_bits(), bits);
    }
  }

  // A call writes its path and each argument through its own `ToRust`.
  #[test]
  fn call_writes_each_argument() {
    let mut out = RustExpr::new();
    out.path("artic_layout", "::artic::layout");
    out.call("artic_layout", "Point::new", &[&1.5_f32, &-2_i32]);
    assert_eq!(out.as_str(), "::artic::layout::Point::new(1.5_f32, -2_i32)");
  }

  // A struct literal writes each field by name, and an empty one writes empty braces.
  #[test]
  fn struct_literal_writes_each_field() {
    let mut out = RustExpr::new();
    out.struct_literal("artic_render", "Solid", &[("opacity", &1.0_f32), ("visible", &true)]);
    assert_eq!(out.take(), "::artic_render::Solid { opacity: 1.0_f32, visible: true }");
    out.struct_literal("artic_render", "Unit", &[]);
    assert_eq!(out.as_str(), "::artic_render::Unit {}");
  }

  // A list separates its items with commas, and writes nothing for no items.
  #[test]
  fn list_separates_items() {
    let mut out = RustExpr::new();
    out.list(["a", "b"], |out, name| out.str(name));
    assert_eq!(out.take(), r#""a", "b""#);
    out.list([0_u8; 0], |out, item| item.to_rust(out));
    assert_eq!(out.as_str(), "");
  }

  // Strings are escaped so quotes, backslashes and newlines survive.
  #[test]
  fn str_escapes_text() {
    let mut out = RustExpr::new();
    out.str("a\"b\\c\nd é");
    assert_eq!(out.as_str(), r#""a\"b\\c\nd é""#);
  }
}
