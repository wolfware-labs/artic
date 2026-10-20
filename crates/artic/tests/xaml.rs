// An app that uses only the facade gets artic's UI types in its registry: every crate artic's URI
// names is linked, even though this file never names those crates.

#![cfg(feature = "xaml")]

use artic::xaml::{ARTIC_UI, namespaces};
use artic_registry::{Registry, UI_TYPES};
use std::collections::BTreeSet;

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

// Every crate that defines a registered UI type or group has a facade path, so generated code
// reaches it in an app that depends only on the facade.
#[test]
fn defining_crates_have_facade_paths() {
  let map = namespaces();
  let unmapped: BTreeSet<_> = UI_TYPES
    .iter()
    .flat_map(|info| std::iter::once(info.rust_path).chain(info.groups.iter().map(|group| group.methods)))
    .map(|path| path.split("::").next().unwrap_or(path))
    .filter(|krate| !map.path_of(krate).starts_with("::artic::"))
    .collect();
  assert!(unmapped.is_empty(), "no facade path: {unmapped:?}");
}
