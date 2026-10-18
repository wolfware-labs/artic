// An app that uses only the facade gets artic's UI types in its registry: every crate artic's URI
// names is linked, even though this file never names those crates.

#![cfg(feature = "xaml")]

use artic::xaml::{ARTIC_UI, namespaces};
use artic_registry::Registry;

// Each crate artic's URI stands for registers at least one type in an app that links only the
// facade.
#[test]
fn artic_uri_crates_are_linked() {
  let registry = Registry::from_link_time().expect("no duplicates");
  let map = namespaces();

  for name in map.crates(ARTIC_UI).expect("artic's URI") {
    let probe = match name.as_str() {
      "artic_controls" => "Border",
      "artic_element" => "Surface",
      other => panic!("no probe type for {other}"),
    };
    assert!(
      registry.type_by_name(name, probe).found().is_some(),
      "{name}::{probe} isn't linked"
    );
  }
}
