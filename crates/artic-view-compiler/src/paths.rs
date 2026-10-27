//! The path generated code reaches each crate by.

use artic_registry::{crate_and_item, crate_of};
use std::collections::BTreeMap;

/// The path generated code reaches each crate by, when it isn't the crate's own name, such as a
/// crate re-exported by a facade: `artic_controls` as `::artic::controls`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CratePaths {
  paths: BTreeMap<String, String>,
}

impl CratePaths {
  /// No crate has a path set: each is reached at its root.
  #[must_use]
  pub fn new() -> CratePaths {
    CratePaths::default()
  }

  /// Generated code reaches `krate` as `path`, such as `artic_controls` as `::artic::controls`.
  /// A second call for the same crate replaces the first.
  pub fn path(&mut self, krate: &str, path: &str) {
    self.paths.insert(krate.to_owned(), path.to_owned());
  }

  /// The path for `krate`: the one set with [`path`](CratePaths::path), or `::krate`.
  #[must_use]
  pub fn path_of(&self, krate: &str) -> String {
    self.paths.get(krate).cloned().unwrap_or_else(|| format!("::{krate}"))
  }

  /// Every crate given a path, in sorted order.
  pub fn crates_named(&self) -> impl Iterator<Item = &str> {
    self.paths.keys().map(String::as_str)
  }

  /// Where generated code reaches the item at `rust_path`: its crate's path and the item's name, since
  /// a UI crate re-exports each item at its root. `artic_element::properties::LayoutMethods` gives
  /// `::artic::element::LayoutMethods` when `artic_element` is reached as `::artic::element`.
  #[must_use]
  pub fn item(&self, rust_path: &str) -> String {
    let (krate, item) = crate_and_item(rust_path);
    format!("{}::{item}", self.path_of(krate))
  }

  /// Where generated code reaches `module`: its crate's path, then the rest of `module`.
  /// `artic_core::tree` gives `::artic::core::tree` when `artic_core` is reached as `::artic::core`.
  #[must_use]
  pub fn module(&self, module: &str) -> String {
    let krate = crate_of(module);
    format!("{}{}", self.path_of(krate), &module[krate.len()..])
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // A crate nothing maps is reached at its root.
  #[test]
  fn path_of_an_unmapped_crate_is_its_root() {
    assert_eq!(CratePaths::new().path_of("acme_controls"), "::acme_controls");
  }

  // A mapped crate is reached by its path, and a second `path` replaces the first.
  #[test]
  fn path_maps_a_crate() {
    let mut paths = CratePaths::new();
    paths.path("artic_controls", "::artic::widgets");
    paths.path("artic_controls", "::artic::controls");

    assert_eq!(paths.path_of("artic_controls"), "::artic::controls");
  }

  // Every crate given a path is listed once, sorted.
  #[test]
  fn crates_named_lists_each_path_once_sorted() {
    let mut paths = CratePaths::new();
    paths.path("artic_layout", "::artic::layout");
    paths.path("acme_charts", "::charts");
    paths.path("artic_layout", "::artic::layout");

    assert_eq!(paths.crates_named().collect::<Vec<_>>(), ["acme_charts", "artic_layout"]);
  }

  // An item is reached at its crate's root, through the crate's path.
  #[test]
  fn item_keeps_the_name_at_the_crate_root() {
    let mut paths = CratePaths::new();
    paths.path("artic_element", "::artic::element");

    assert_eq!(
      paths.item("artic_element::properties::LayoutMethods"),
      "::artic::element::LayoutMethods"
    );
    assert_eq!(paths.item("acme::gauges::Gauge"), "::acme::Gauge");
  }

  // A module keeps its path below the crate; only the crate is translated.
  #[test]
  fn module_translates_only_the_crate() {
    let mut paths = CratePaths::new();
    paths.path("artic_controls", "::artic::controls");

    assert_eq!(paths.module("artic_controls::border"), "::artic::controls::border");
    assert_eq!(paths.module("acme"), "::acme");
  }
}
