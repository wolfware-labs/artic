//! Which crates a namespace URI stands for.
//!
//! A type's namespace is the crate it's defined in. XAML names namespaces with URIs instead, such
//! as `https://artic.rs/ui`, so the back ends that look types up start from a map of each URI to
//! its crates. The map knows nothing about types: the `artic` facade fills in artic's URI, an app
//! adds its own, and a back end turns a URI into crates before it asks the registry. A `using:`
//! prefix needs no entry: it's already a Rust path.
//!
//! Generated code names a crate's items through the path the app reaches the crate by, which
//! [`NamespaceMap::path`] sets when it isn't the crate's own name, such as a crate re-exported by a
//! facade.

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Which crates each namespace URI stands for, and the path generated code reaches each crate by.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NamespaceMap {
  crates: HashMap<String, Vec<String>>,
  paths: BTreeMap<String, String>,
}

impl NamespaceMap {
  /// An empty map: no URI stands for anything.
  #[must_use]
  pub fn new() -> NamespaceMap {
    NamespaceMap::default()
  }

  /// Adds `crates` to what `uri` stands for. A crate it already stands for is kept once, so
  /// declaring an alias twice changes nothing.
  pub fn alias(&mut self, uri: &str, crates: &[&str]) {
    let known = self.crates.entry(uri.to_owned()).or_default();
    for name in crates {
      if !known.iter().any(|existing| existing == name) {
        known.push((*name).to_owned());
      }
    }
  }

  /// The crates `uri` stands for, in the order they were added, or `None` for a URI nobody
  /// declared.
  #[must_use]
  pub fn crates(&self, uri: &str) -> Option<&[String]> {
    self.crates.get(uri).map(Vec::as_slice)
  }

  /// Generated code reaches `krate` as `path`, such as `artic_controls` as `::artic::controls`.
  /// A second call for the same crate replaces the first.
  pub fn path(&mut self, krate: &str, path: &str) {
    self.paths.insert(krate.to_owned(), path.to_owned());
  }

  /// The path for `krate`: the one set with [`path`](NamespaceMap::path), or `::krate`.
  #[must_use]
  pub fn path_of(&self, krate: &str) -> String {
    self.paths.get(krate).cloned().unwrap_or_else(|| format!("::{krate}"))
  }

  /// Every crate the map names, through a URI or a path, each once and in sorted order.
  pub fn crates_named(&self) -> impl Iterator<Item = &str> {
    self
      .crates
      .values()
      .flatten()
      .chain(self.paths.keys())
      .map(String::as_str)
      .collect::<BTreeSet<_>>()
      .into_iter()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // An alias maps a URI to its crates, in order.
  #[test]
  fn alias_maps_a_uri_to_crates() {
    let mut map = NamespaceMap::new();
    map.alias("https://acme.test/ui", &["acme_controls", "acme_charts"]);

    assert_eq!(
      map.crates("https://acme.test/ui"),
      Some(&["acme_controls".to_owned(), "acme_charts".to_owned()][..])
    );
  }

  // A second alias for a URI adds to it.
  #[test]
  fn alias_adds_to_a_uri() {
    let mut map = NamespaceMap::new();
    map.alias("https://acme.test/ui", &["acme_controls"]);
    map.alias("https://acme.test/ui", &["acme_charts"]);

    assert_eq!(map.crates("https://acme.test/ui").map(<[String]>::len), Some(2));
  }

  // Declaring a crate twice for one URI keeps it once.
  #[test]
  fn repeated_alias_is_kept_once() {
    let mut map = NamespaceMap::new();
    map.alias("https://acme.test/ui", &["acme_controls"]);
    map.alias("https://acme.test/ui", &["acme_controls", "acme_controls"]);

    assert_eq!(map.crates("https://acme.test/ui"), Some(&["acme_controls".to_owned()][..]));
  }

  // A URI nobody declared is unknown.
  #[test]
  fn unknown_uri_is_none() {
    assert_eq!(NamespaceMap::new().crates("https://nobody.test"), None);
  }

  // A URI declared with no crates is known and stands for nothing.
  #[test]
  fn alias_to_no_crates_is_declared() {
    let mut map = NamespaceMap::new();
    map.alias("https://empty.test", &[]);

    assert_eq!(map.crates("https://empty.test"), Some(&[][..]));
  }

  // A crate nothing maps is reached at its root.
  #[test]
  fn path_of_an_unmapped_crate_is_its_root() {
    assert_eq!(NamespaceMap::new().path_of("acme_controls"), "::acme_controls");
  }

  // A mapped crate is reached by its path, and a second `path` replaces the first.
  #[test]
  fn path_maps_a_crate() {
    let mut map = NamespaceMap::new();
    map.path("artic_controls", "::artic::widgets");
    map.path("artic_controls", "::artic::controls");

    assert_eq!(map.path_of("artic_controls"), "::artic::controls");
  }

  // Every crate named through a URI or a path is listed once, sorted.
  #[test]
  fn crates_named_lists_each_crate_once() {
    let mut map = NamespaceMap::new();
    map.alias("https://acme.test/ui", &["acme_controls", "acme_charts"]);
    map.alias("https://acme.test/extra", &["acme_controls"]);
    map.path("artic_layout", "::artic::layout");
    map.path("acme_charts", "::charts");

    assert_eq!(
      map.crates_named().collect::<Vec<_>>(),
      ["acme_charts", "acme_controls", "artic_layout"]
    );
  }
}
