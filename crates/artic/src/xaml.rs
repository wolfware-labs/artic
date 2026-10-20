//! XAML's view of artic: which crates artic's namespace URI stands for.
//!
//! The runtime loader takes this map when the app sets it up, and an app's `build.rs` passes the
//! same map to the compiler, so both resolve the same names. An app adds its own URIs with
//! [`NamespaceMap::alias`].

pub use artic_xaml::namespaces::NamespaceMap;

/// Compiles a directory of views from a build script, and why it stopped.
#[cfg(feature = "xaml-compiler")]
pub use artic_xaml_compiler::{CompileError, compile_dir};

// The registry only knows linked crates, and a crate nothing refers to is left out of the link:
// these keep the crates `ARTIC_UI` names in every app that uses this module.
use artic_controls as _;
use artic_element as _;

/// The namespace URI of artic's own UI types.
pub const ARTIC_UI: &str = "https://artic.rs/ui";

/// A map holding artic's URI, standing for the artic crates that define UI types, and the facade
/// module each artic crate is reached by.
#[must_use]
pub fn namespaces() -> NamespaceMap {
  let mut map = NamespaceMap::new();
  map.alias(ARTIC_UI, &["artic_element", "artic_controls"]);
  map.path("artic_controls", "::artic::controls");
  map.path("artic_element", "::artic::element");
  map.path("artic_layout", "::artic::layout");
  map.path("artic_render", "::artic::render");
  map
}

#[cfg(test)]
mod tests {
  use super::*;

  // Each crate artic's URI names is the namespace of a real UI type, so renaming or splitting a
  // crate can't leave the map pointing at nothing.
  #[test]
  fn artic_uri_names_real_namespaces() {
    use artic_registry::UiType;

    let map = namespaces();
    let crates = map.crates(ARTIC_UI).expect("artic's URI");

    assert!(
      crates
        .iter()
        .any(|name| name == <artic_controls::Border as UiType>::TYPE_INFO.namespace())
    );
    assert!(
      crates
        .iter()
        .any(|name| name == <artic_element::Surface as UiType>::TYPE_INFO.namespace())
    );
  }

  // artic's URI stands for the crates that define its UI types, the same each time.
  #[test]
  fn facade_maps_the_artic_uri() {
    let map = namespaces();

    assert_eq!(
      map
        .crates(ARTIC_UI)
        .map(|crates| crates.iter().map(String::as_str).collect::<Vec<_>>()),
      Some(vec!["artic_element", "artic_controls"])
    );
    assert_eq!(namespaces(), map);
  }

  // Each artic crate is reached through the facade module that re-exports it.
  #[test]
  fn facade_maps_crates_to_its_modules() {
    let map = namespaces();

    assert_eq!(map.path_of("artic_controls"), "::artic::controls");
    assert_eq!(map.path_of("artic_element"), "::artic::element");
    assert_eq!(map.path_of("artic_layout"), "::artic::layout");
    assert_eq!(map.path_of("artic_render"), "::artic::render");
    assert_eq!(
      map.crates_named().collect::<Vec<_>>(),
      ["artic_controls", "artic_element", "artic_layout", "artic_render"]
    );
  }
}
