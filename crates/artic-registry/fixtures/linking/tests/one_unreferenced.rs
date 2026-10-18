// Only fixture A is named. Fixture B is a dependency too, but nothing here refers to it, so rustc
// leaves it out of the link and its entry never reaches UI_TYPES.

use artic_registry::Registry;
use artic_registry_fixture_a as _;

// An entry crate listed in Cargo.toml but never named is missing; the named one is found.
#[test]
fn entries_from_an_unreferenced_crate_are_missing() {
  let registry = Registry::from_link_time().expect("no duplicates");

  assert!(registry.type_by_rust_path("artic_registry_fixture_a::Alpha").is_some());
  assert!(registry.type_by_rust_path("artic_registry_fixture_b::Beta").is_none());
}
