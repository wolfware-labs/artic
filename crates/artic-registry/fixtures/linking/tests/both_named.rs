// Both fixture crates are named, so both are part of the link and their entries reach UI_TYPES.

use artic_registry::Registry;
use artic_registry_fixture_a as _;
use artic_registry_fixture_b as _;

// Entries registered from two separate crates are both found.
#[test]
fn entries_from_both_named_crates_are_found() {
  let registry = Registry::from_link_time().expect("no duplicates");

  assert!(registry.type_by_rust_path("artic_registry_fixture_a::Alpha").is_some());
  assert!(registry.type_by_rust_path("artic_registry_fixture_b::Beta").is_some());
}

// Two crates each register a `Twin`: each is found in its own namespace and by its Rust path.
#[test]
fn same_name_in_two_crates_is_found_in_each() {
  let registry = Registry::from_link_time().expect("no duplicates");

  for namespace in ["artic_registry_fixture_a", "artic_registry_fixture_b"] {
    let found = registry.type_by_name(namespace, "Twin").found().expect("found");
    assert_eq!(found.rust_path, format!("{namespace}::Twin"));
  }
}
