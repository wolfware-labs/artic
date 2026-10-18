//! Which crates a namespace URI stands for.
//!
//! A type's namespace is the crate it's defined in. XAML names namespaces with URIs instead, such
//! as `https://artic.rs/ui`, so the back ends that look types up start from a map of each URI to
//! its crates. The map knows nothing about types: the `artic` facade fills in artic's URI, an app
//! adds its own, and a back end turns a URI into crates before it asks the registry. A `using:`
//! prefix needs no entry: it's already a Rust path.

use std::collections::HashMap;

/// Which crates each namespace URI stands for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NamespaceMap {
  crates: HashMap<String, Vec<String>>,
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
}
